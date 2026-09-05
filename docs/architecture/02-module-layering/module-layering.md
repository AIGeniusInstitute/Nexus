# Nexus 企业级 AI Agent 平台 — 核心组件模块分层与模块详细设计

> 产物编号：任务二-2 · 核心组件模块分层与模块详细设计
> 基座：`~/Nexus`（基于 OpenAI Codex Harness，codex-rs 106 crate Rust 工作区）
> 日期：2026-09-06 · 配套图：`module-layering.svg` / `.png` · 交互报告：`module-layering-report.html`

---

## 0. 设计判断（结论先行）

> **一句话**：八层架构中 L5 复用 Codex Harness 的 106 个 Rust crate 作为黑盒执行内核，L1-L3 与 L6-L7 全自建，L4 是薄壳适配层。模块间的依赖方向严格"上 -> 下"，禁止逆序依赖；L5 与控制面之间仅经 app-server JSON-RPC 协议通信，不共享内存状态。

**五个模块分层的核心判断**：

| # | 判断 | 理由 |
|---|---|---|
| 1 | L5 Harness crate 全量复用、零内核改动 | `core`（`run_turn` 七阶段）、`app-server`（主集成面）、`execpolicy`（Starlark 规则引擎）、`sandboxing`（OS 沙箱）等 106 crate 已覆盖 Agent 执行全链路。改动任何内核 crate 会在数周内变成上游合并地狱 |
| 2 | app-server-protocol 是唯一集成面 | `app-server` + `app-server-protocol` crate 提供 JSON-RPC 2.0 双向事件流、Thread/Turn/Item 三原语、`thread/resume`/`fork`/`rollback`、审批回写。用 `generate-json-schema`/`generate-ts` 生成类型纳入 CI，协议变更自动检出 |
| 3 | 控制面自建七大子系统，各自独立、经事件总线松耦合 | 身份租户/任务编排/审批中心/策略中心/配额计费/连接器治理/知识库 RAG——每个子系统独立部署、独立扩缩，经 Temporal Workflow 或事件总线编排，不做单体 |
| 4 | 模块依赖严格分层，禁止逆序 | L1 依赖 L2，L2 依赖 L3，L3 依赖 L4/L7，L4 依赖 L5/L6/L7；L5 不反向依赖任何上层。跨层调用经接口（协议/事件/API），不直接引用上层 crate/模块 |
| 5 | 配置即政策，不分支化 | 租户差异表达为运行时注入的 `config.toml` + `execpolicy.rules` + `enabled_tools` + `AGENTS.md`，不维护任何内核分支 |

---

## 1. 八层模块清单总览

| 层 | 职责 | 子模块数 | 是否复用 Codex | 关键 crate/技术 |
|---|---|---|---|---|
| L1 接入层 | 多渠道统一入口 | 6 | 否（自建） | React+WS / IM Bot / IDE / REST / CLI |
| L2 网关层 | 南北向流量、鉴权、实时推送 | 5 | 否（自建） | API Gateway / WS 网关 / OIDC/SAML / 限流 |
| L3 控制平面 | 平台的大脑与账本 | 7 子系统 × 3-4 子模块 | 否（自建核心） | Temporal / Postgres RLS / 策略引擎 |
| L4 执行平面 | Harness 托管外壳 | 7 | 薄壳 + 复用 | K8s Pod / MCP Gateway / 凭据代理 |
| L5 Harness | Agent 执行内核 | 40+ crate | **是（黑盒）** | `core` / `app-server` / `execpolicy` / `sandboxing` |
| L6 模型层 | 模型访问与计量 | 6 | 部分复用 | `model-provider` / `responses-api-proxy` / `ollama` |
| L7 存储与治理 | 持久化与可观测 | 6 | 否（自建） | Postgres / S3 / pgvector / WORM / OTel |
| 贯穿 | 安全与合规 | 6 域 | 否（自建） | KMS / NetworkPolicy / 审计 / 红队 |

---

## 2. L1 接入层 — 模块详细设计

### 2.1 模块清单

| 模块 | 形态 | 职责 | 输入 | 输出 | 上游依赖 | 下游依赖 |
|---|---|---|---|---|---|---|
| Web 门户 | React + WebSocket | 会话列表、任务时间线（Item 流）、审批抽屉、产物预览、Diff 查看 | 用户操作 | REST 请求 + WS 订阅 | 无 | L2 API Gateway + WS 网关 |
| IM Bot | 飞书/钉钉/企微/Slack | 审批推送（卡片消息）、任务通知、简易交互 | IM 事件回调 | REST 请求 | 无 | L2 API Gateway |
| IDE 插件 | VS Code/JetBrains 扩展 | 远端 Thread 映射到本地、代码 diff 预览、审批 | 编辑器操作 | app-server 协议（经网关代理） | 无 | L2 网关代理 |
| OpenAPI | REST + Webhook | Agent 能力被业务系统调用、任务完成回调 | 外部系统 HTTP 请求 | REST 响应 + Webhook 回调 | 无 | L2 API Gateway |
| CLI | `codex` + 自定义登录 | 开发者体验入口、批处理脚本 | 命令行参数 | JSONL / 交互输出 | 无 | L2 网关（自定义 OAuth） |
| Webhook 回调 | REST | 任务完成、审批结果异步通知 | 内部事件 | HTTP POST 到外部 URL | L3 事件总线 | 无 |

### 2.2 关键接口

- **统一约束**：所有入口都不直连 Harness（L5）。必须经 L2 网关，否则审计链路断裂。
- **Web 门户 ↔ WS 网关**：WebSocket 连接，订阅 Thread 事件流；权限变更立即断连。
- **IM Bot 审批卡片**：飞书/钉钉卡片消息承载"批准/拒绝/修改后批准"三按钮，回调经 L2 网关鉴权后写入 L3 审批中心。
- **CLI**：复用 Codex `cli` crate，登录流程改为 OAuth device flow → L2 认证中间件。

### 2.3 不做

- 不在接入层做业务逻辑（编排、策略、计费全在 L3）。
- 不在接入层直连 Harness（禁止绕过审计）。

---

## 3. L2 网关层 — 模块详细设计

### 3.1 模块清单

| 模块 | 职责 | 输入 | 输出 | 上游依赖 | 下游依赖 |
|---|---|---|---|---|---|
| API Gateway | REST 路由、请求校验、幂等（`Idempotency-Key`）、限流 | HTTP 请求 | 路由到 L3 后端服务 | L1 入口 | L3 控制面各子系统 |
| WebSocket 网关 | 会话事件推送、订阅权限驱动 | WS 连接请求 | 事件流推送 | L1 Web 门户 | L3 事件总线 + L7 Postgres（权限校验） |
| 认证中间件 | OIDC/SAML 对接企业 IdP、SCIM 同步组织架构、服务账号 mTLS | HTTP 请求头 | 认证上下文（user_id, tenant_id, roles） | L1 入口 | L3 身份租户子系统 |
| 限流引擎 | 租户级 + 用户级 + IP 级限流 | 请求特征 | 放行/拒绝/排队 | L1 入口 | 无 |
| 配额预扣 | 粗粒度拦截（预估 token + 沙箱时长） | 任务请求 | 预扣结果 | L3 配额计费 | L3 配额计费 |

### 3.2 关键接口

- **认证中间件 → L3 身份租户**：OIDC token → 验证 → 注入 `tenant_id`/`user_id`/`roles` 到请求上下文。SCIM endpoint 接收 IdP 推送的组织架构变更。
- **WS 网关订阅模型**：`subscribe(thread_id)` → 校验用户对该 Thread 的读权限 → 建立事件推送通道。权限变更（用户被移出/角色变更）→ 立即断连。
- **幂等键**：`Idempotency-Key` header → Redis SET NX → 重复请求直接返回缓存结果，避免重复计费。

### 3.3 不做

- 不在网关层做细粒度策略求值（在 L3 策略中心）。
- 不在网关层做细粒度计量结算（在 L3 配额计费）。

---

## 4. L3 控制平面 — 七大自建子系统详细设计

> 控制平面是平台的大脑与账本，自建七大子系统。各子系统独立部署、独立扩缩，经 Temporal Workflow 或事件总线编排。

### 4.1 身份与租户子系统

#### 模块清单

| 模块 | 职责 | 关键接口 | 输入 | 输出 | 上下游 |
|---|---|---|---|---|---|
| Tenant 模型 | 租户/OrgUnit/Role 层级管理 | `createTenant` / `createOrgUnit` / `assignRole` | 管理操作 | 租户/组织/角色记录 | 上：L2 认证中间件；下：L7 Postgres |
| Agent 身份管理 | 服务账号管理（Agent ≠ 用户） | `createServiceAccount` / `grantAgentPermission` | 管理操作 | 服务账号+权限子集 | 上：L3 任务编排；下：L7 Postgres |
| 连接器身份 | MCP/OAuth 委托令牌管理 | `createConnectorIdentity` / `refreshDelegateToken` | 连接器注册 | 委托令牌（按租户隔离） | 上：L3 连接器治理；下：L7 密钥库 |
| 权限交集求值 | 四取交集权限计算 | `evaluatePermission(user, workspace, agent, action)` | 权限请求 | allow/deny | 上：L3 策略中心；下：L7 Postgres |

#### 数据模型

```
Tenant(id, name, plan, isolation_tier, cmk_id, quota_profile, status)
  └── OrgUnit(id, tenant_id, parent_id, path, name)  -- 可多级，映射部门树
        └── User(id, tenant_id, idp_subject, email, display_name)
        └── ServiceAccount(id, tenant_id, name, role_id, cert_fingerprint)
              └── Role(id, tenant_id, name, permissions_json)
  └── Workspace(id, tenant_id, name, env_tag, repos_json, connectors_json,
                knowledge_scope_json, sandbox_mode, approval_policy, max_risk_level)
        └── Membership(id, tenant_id, user_id, org_unit_id, role_id, scope_json)
```

#### 权限继承规则

```
Agent 可用权限 = 用户权限 ∩ 工作区权限 ∩ Agent 角色上限 ∩ 策略中心允许
```

四者取交集，任一为空即拒绝。**三个必须区分的身份**：
1. 用户身份（人在用）
2. Agent 身份（服务账号，权限是用户的子集且需显式授予）
3. 连接器身份（MCP/OAuth 委托令牌，按租户隔离存储）

#### 授权模型

- RBAC 打底：owner / admin / developer / auditor / viewer
- ABAC 兜底：属性 = `tenant_id` / `org_path` / `env` / `data_classification` / `risk_level` / `time_window`

---

### 4.2 任务编排子系统（Temporal Workflow）

#### 模块清单

| 模块 | 职责 | 关键接口 | 输入 | 输出 | 上下游 |
|---|---|---|---|---|---|
| Workflow 引擎 | 持久化可恢复编排（Temporal） | `startWorkflow(taskDef)` / `signalWorkflow(id, signal)` / `queryWorkflow(id, query)` | 任务定义 | Workflow 执行状态 | 上：L2 API Gateway；下：L4 Runtime 池 |
| 外层循环 | 平台资源与账本管理（长周期） | `provisionPod` / `consumeEvents` / `settleUsage` | 任务请求 | 资源分配 + 账本记录 | 上：L3 各子系统；下：L4/L7 |
| 内层循环关联 | 与 Harness `run_turn` 关联（短周期） | 事件流关联（不混为一个循环） | app-server 事件 | 事件路由 | 上：L4 事件桥接；下：L5 app-server |
| 调度策略 | 租户权重队列 + 优先级 + 并发上限 | `enqueue(task)` / `dequeue()` | 任务+租户上下文 | Pod 分配结果 | 上：L3 Workflow；下：L4 Runtime 池 |

#### 两层循环设计

```
外层（平台）Workflow                    内层（Harness）run_turn
  ├─ 申请 Pod                             ├─ 准入（策略快照注入）
  ├─ 建连 app-server                      ├─ 快照（上下文序列化）
  ├─ 下发任务（thread/start 或 resume）     ├─ 采样（模型调用）
  ├─ 消费事件流 → 写 Postgres              ├─ 工具调度（ToolRouter）
  ├─ 处理审批（等几小时）                   ├─ 结果写回（Item 落盘）
  ├─ 收尾结算（用量+审计）                  ├─ 压缩判定（auto compact）
  └─ 销毁 Pod                             └─ 完成/中断
        ↑ 事件流关联，不混为一个循环 ↑
```

**为什么用 Temporal 而非自建状态机**：审批可能等几小时、Pod 会崩、网络会断——只有持久化工作流能天然表达"等三天再继续"。

---

### 4.3 审批中心子系统（HITL）

#### 模块清单

| 模块 | 职责 | 关键接口 | 输入 | 输出 | 上下游 |
|---|---|---|---|---|---|
| ApprovalTicket 管理 | 审批生命周期（pending→decided） | `createTicket` / `decideTicket(id, decision)` | app-server 审批请求事件 | 审批记录 | 上：L4 事件桥接；下：L7 Postgres |
| 多渠道推送 | Web 抽屉 + IM 卡片 + 邮件 | `pushApproval(ticket, channels)` | ApprovalTicket | 推送结果 | 上：L3 审批中心；下：L1 Web/IM |
| 边界处理 | Pod 崩溃/超时/权限撤销 | `handleTimeout` / `replayDecision` | 超时/重建事件 | 恢复结果 | 上：L3 Workflow；下：L7 Postgres |
| 批量审批 | 同类操作作用域批准 | `createScopeApproval(scope)` | 批量请求 | 作用域规则 | 上：L3 审批中心；下：L7 Postgres |

#### 审批流程（跨进程 HITL 桥接）

```
① app-server 发出审批请求事件
   ↓
② 适配层解析 → 控制面创建 ApprovalTicket（pending）
   ├─ 内容：thread_id / turn_id / item_seq / 工具名 / 参数(脱敏) / diff预览 / 风险等级
   ├─ 策略：谁可批（单人/双人/指定角色）、超时动作（默认拒绝）
   └─ 快照：审批时上下文（事后可看"批的是什么"）
   ↓
③ 推送：Web 抽屉 + IM 卡片 + 邮件（按风险选渠道）
   ↓
④ 用户决策（批准/拒绝/修改后批准/转交）
   ↓
⑤ 决策先落库（decided）再回写 app-server
   ↓
⑥ app-server 继续/中止；结果回事件流闭环
```

#### 六个边界情况

| 情况 | 处理 |
|---|---|
| Pod 等待时崩溃 | 审批状态在 DB，Pod 重建后 resume，`item_seq` 去重，已决策直接重放 |
| 审批超时 | 默认**拒绝**（非批准），通知申请人 |
| 审批期间权限撤销 | 决策时重新校验审批人权限，失效则 ticket 作废 |
| 修改参数后批准 | 重新走策略求值（改参数=新请求） |
| 批量相似请求 | 作用域有限定：仅该目录/仅该工具/≤1h |
| 审计 | 请求快照+决策人+时间+理由不可篡改 |

---

### 4.4 策略中心子系统

#### 模块清单

| 模块 | 职责 | 关键接口 | 输入 | 输出 | 上下游 |
|---|---|---|---|---|---|
| 策略对象管理 | 策略 CRUD（tenant/role/tool/action/risk_level） | `createPolicy` / `updatePolicy` / `deletePolicy` | 管理操作 | 策略记录 | 上：L2 管理接口；下：L7 Postgres |
| 决策引擎 | 求值策略 → allow/deny/require_approval/dual_approval/audit_only | `evaluate(tenant, role, workspace, tool, action, risk)` | 策略求值请求 | 决策结果 | 上：L3 编排+审批；下：L7 Postgres |
| 漂移防护 | 策略快照写入任务上下文 | `snapshotPolicy(taskContext)` | 任务上下文 | 策略快照哈希 | 上：L3 编排；下：L7 Postgres |
| execpolicy 生成 | 按策略生成 Starlark 规则集 | `generateExecPolicy(tenant, role, riskLevel)` | 策略上下文 | `execpolicy.rules` 文件 | 上：L3 配置生成器；下：L4 沙箱 Pod |

#### 求值时机

- 任务准入时一次
- **每次高危工具调用前一次**（不把准入当永久通行证）

#### 漂移防护

- 策略快照写入任务上下文（`permission_snapshot_hash`）
- 运行中策略变更：不溯及已批准动作，对新动作用新策略

---

### 4.5 配额与计费子系统

#### 模块清单

| 模块 | 职责 | 关键接口 | 输入 | 输出 | 上下游 |
|---|---|---|---|---|---|
| 四维计量 | token(prompt/cached/reasoning/output) + 工具调用 + 沙箱时长 + 存储流量 | `recordUsage(metric, quantity)` | 事件流 | usage_record | 上：L3 事件消费者；下：L7 Postgres |
| 预算控制 | 软告警 → 降档经济模型 → 硬阈值熔断 | `checkBudget(tenant)` / `enforceQuota(tenant)` | 租户上下文 | 放行/告警/熔断 | 上：L2 配额预扣+L3 编排；下：L7 Postgres |
| 优雅暂停 | 保存 rollout 后回收 Pod，预算恢复可 resume | `pauseTask(taskId)` / `resumeTask(taskId)` | 熔断信号 | rollout 保存 + Pod 回收 | 上：L3 编排；下：L4/L7 |
| 成本归因 | tenant → org_unit → user → thread → turn → model | `getCostBreakdown(tenant, dateRange)` | 查询请求 | 归因报告 | 上：L2 管理接口；下：L7 Postgres |

#### 数据模型

```sql
usage_record(id, tenant_id, org_unit_id, user_id, thread_id, turn_id,
             metric,                           -- token_in/token_out/token_cached/tool_call/sandbox_second/storage_byte
             quantity, model, unit_cost_micros, occurred_at)
```

---

### 4.6 连接器治理子系统

#### 模块清单

| 模块 | 职责 | 关键接口 | 输入 | 输出 | 上下游 |
|---|---|---|---|---|---|
| 分级管理 | 官方认证/企业私有/社区（默认禁用） | `registerConnector` / `approveConnector` / `disableConnector` | 连接器注册 | 连接器记录 | 上：L2 管理接口；下：L7 Postgres |
| MCP Gateway 控制 | 凭据注入 + 工具白名单 + 出站审计 | `configureMcpGateway(connectorId, config)` | 连接器配置 | Gateway 配置 | 上：L3 连接器治理；下：L4 MCP Gateway |
| 质量分 | 可用性/P95延迟/错误率/权限最小化 → 低于阈值自动降级/下线 | `calculateQualityScore(connectorId)` | 监控指标 | 质量分 + 降级决策 | 上：L3 连接器治理；下：L7 监控 |
| 凭据代理 | 短期令牌签发/吊销 | `issueToken(taskContext)` / `revokeToken(tokenId)` | 任务上下文 | JWT 令牌 | 上：L3 编排；下：L4 沙箱 Pod |

#### 安全要点

- MCP stdio 型服务器是重大风险面（配置即代码执行）
- 强制：`.codex/config.toml` 只在受信任工作区生效
- `enabled_tools` 白名单优先于 `disabled_tools`
- 破坏性注解工具恒定需审批
- `config.toml` 里不出现任何真实密钥

---

### 4.7 知识库 / RAG 子系统

#### 模块清单

| 模块 | 职责 | 关键接口 | 输入 | 输出 | 上下游 |
|---|---|---|---|---|---|
| ACL 随索引 | 每 chunk 携带 tenant_id + acl_tags + permission_version | `indexDocument(doc, acl)` | 文档+ACL | 向量+元数据 | 上：L2 管理接口；下：L7 pgvector |
| 混合召回 | metadata/ACL 过滤 → 稠密+稀疏混合召回 → rerank | `search(query, tenantId, aclTags)` | 查询+权限 | 支撑片段（附 chunk_id + 权限版本） | 上：L5 工具调用；下：L7 pgvector |
| 权限强制 | 检索权限在 Gateway 侧强制 | `enforceSearchPermission(tenantId, aclTags)` | 检索请求 | 放行/拒绝 | 上：L4 MCP Gateway；下：L7 Postgres |
| 知识注入 | 知识检索做成 MCP 工具或自定义 Tool | MCP tool: `knowledge_search` | 工具调用 | 检索结果 | 上：L5 codex-mcp；下：L7 pgvector |

#### 检索流程

```
metadata/ACL 过滤 → 稠密+稀疏混合召回 → rerank → 只回填支撑片段（附 chunk_id 与权限版本）
```

**关键**：先过滤后召回，绝不先召回后过滤。权限在 Gateway 侧强制，不依赖模型自觉遵守。

---

## 5. L4 执行平面 — 薄壳详细设计

### 5.1 模块清单

| 模块 | 职责 | 输入 | 输出 | 上游依赖 | 下游依赖 | 复用 Codex |
|---|---|---|---|---|---|---|
| Runtime 池调度 | K8s Pod 调度 + 预热池 + 销毁回收 | L3 调度指令 | Pod 分配 | L3 任务编排 | L5 app-server | 否 |
| 沙箱 Pod | 容器隔离（Seccomp/AppArmor/Kata） | Pod 规格 | 运行中的 Pod | L4 Runtime 池 | L5 Harness | 否（容器层，Codex 沙箱是第二层） |
| MCP Gateway 侧车 | 凭据注入 + 工具白名单 + 出站审计 + 脱敏 | L3 连接器配置 | 代理后的 MCP 调用 | L3 连接器治理 | 外部 MCP Server | 否 |
| Workspace 供给 | git worktree / PVC 快照 / AGENTS.md 注入 | 任务上下文 | 可用工作目录 | L3 编排 | L5 Harness | 否 |
| 凭据代理 | 短期 JWT 签发/吊销 | L3 凭据代理 | 任务绑定令牌 | L3 凭据代理 | L5/L6 | 否 |
| 健康探针 | app-server 存活探测 + 僵尸进程处理 + rollout 上传 | 健康检查请求 | 存活状态/回收信号 | L4 Runtime 池 | L3 编排 | 否 |
| 出站白名单 | NetworkPolicy 默认拒绝，仅放行 Model Gateway + MCP Gateway | 网络策略 | 放行/拒绝 | 无 | L5/L6 | 否 |

### 5.2 三层沙箱（缺一不可）

| 层 | 技术 | 防什么 | 对应 Codex crate |
|---|---|---|---|
| ① 容器/微虚拟机层 | K8s Pod + Seccomp/AppArmor；高敏租户用 Kata/Firecracker | 租户间横向逃逸、内核攻击面 | 无（Nexus 自建容器层） |
| ② 命令级 OS 沙箱 | macOS Seatbelt；Linux Landlock+seccomp+bubblewrap；Windows restricted token | Agent 在本工作区乱跑命令、读 SSH key、外传数据 | `sandboxing` / `linux-sandbox` / `bwrap` / `windows-sandbox-rs` |
| ③ 网络层 | NetworkPolicy 默认拒绝全部出站，仅放行 Model Gateway 与 MCP Gateway | 数据外泄、C2 回连、依赖投毒外联 | 无（K8s NetworkPolicy） |

### 5.3 sandbox 与 approval 正交配置

| 场景 | `sandbox.mode` | `approval_policy` | 说明 |
|---|---|---|---|
| 新租户默认 | `read-only` | `untrusted` | 只读+逐条确认，最保守 |
| 成熟场景 | `workspace-write` | `on-failure` | 工作区内可写，失败才问 |
| 高危场景 | `read-only` | `always` | 强管控，任何动作都要人批 |
| 禁止场景 | — | — | `danger-full-access` 不向终端用户开放 |

---

## 6. L5 Harness — Codex Crate 映射详细设计（重点）

> L5 是唯一复用 Codex 的层，106 个 Rust crate 按功能域映射为 8 个模块组。**黑盒不改**，仅薄适配层桥接。

### 6.1 核心引擎模块组

| Codex crate | 职责 | Nexus 中的角色 | 关键接口/方法 |
|---|---|---|---|
| `core` (`codex-core`) | Agent 主循环 `run_turn` 七阶段：准入 → 快照 → 采样 → 工具调度 → 结果写回 → 压缩判定 → 完成/中断 | **执行内核**，黑盒调用 | `run_turn()` / `run_auto_compact()` |
| `core-api` | 门面 API，对外暴露 Core 的安全子集 | 适配层调用入口 | `CoreAPI` trait |
| `core-plugins` | 插件系统，支持扩展 Core 行为 | 企业 Skill 扩展基础 | `Plugin` trait |
| `protocol` (`codex-protocol`) | 协议层，定义 Turn/Item/Event 类型 | 事件类型映射来源 | `Turn` / `Item` / `Event` 类型 |
| `context-fragments` | 上下文碎片管理 | 上下文工程基础 | `ContextFragment` |
| `prompts` | 提示词管理 | 系统提示注入点 | `SystemPrompt` |
| `app-server` (`codex-app-server`) | **主集成面**，JSON-RPC 2.0 长驻进程 | Nexus 唯一 Harness 集成入口 | `thread/start` / `turn/*` / `thread/resume` / `thread/fork` / `thread/rollback` |
| `app-server-protocol` | app-server 协议定义（v1/v2） | 类型生成来源（`generate-json-schema` / `generate-ts`） | `protocol/v2/thread.rs` / `notification.rs` / `command_exec.rs` |
| `app-server-client` | app-server 客户端库 | 适配层连接管理 | `AppServerClient` |
| `app-server-transport` | 传输层（stdio/unix socket/WS） | Pod 内通信 | `Transport` trait |
| `app-server-daemon` | 守护进程模式 | Pod 内 app-server 守护 | `Daemon` |
| `codex-api` | 实验性 API | 内部工具使用 | `experimental_api.rs` |

### 6.2 持久化与沙箱模块组

| Codex crate | 职责 | Nexus 中的角色 | 关键接口/方法 |
|---|---|---|---|
| `state` (`codex-state`) | 本地 SQLite 状态管理 | **可丢弃缓存**（云端 Postgres 为真相） | `State` / SQLite tables |
| `thread-store` | Thread 跨进程重启恢复 | 会话持久化桥接基础 | `ThreadStore` |
| `rollout` | 事件回放文件 | **改造点**：事件外送到云端 + 对象存储 | `Rollout` / `write_rollout` |
| `history` | 历史记录 | 会话历史查询 | `History` |
| `sandboxing` (`codex-sandboxing`) | 沙箱总控 | **直接用**，不重写 | `Sandbox` trait / `sandbox-exec` |
| `linux-sandbox` | Linux Landlock + seccomp | 容器内需自检 | `LinuxSandbox` |
| `bwrap` | Bubblewrap 沙箱 | Linux 容器层补充 | `BwrapSandbox` |
| `windows-sandbox-rs` | Windows restricted token | Windows 部署 | `WindowsSandbox` |
| `execpolicy` (`codex-execpolicy`) | Starlark 规则引擎（命令 allow/deny 评估） | **按租户下发规则集** | `evaluate(command)` / `ExecPolicy` |
| `mxc-sandbox` | macOS 沙箱 | macOS 部署 | `MxcSandbox` |
| `process-hardening` | 进程加固 | 安全基础 | `ProcessHardening` |

### 6.3 工具/技能/模型模块组

| Codex crate | 职责 | Nexus 中的角色 | 关键接口/方法 |
|---|---|---|---|
| `codex-mcp` | MCP 客户端 | 经 MCP Gateway 代理 | `McpClient` |
| `rmcp-client` | Rust MCP 客户端（新版） | MCP 协议实现 | `RmcpClient` |
| `skills` (`codex-skills`) | 可复用 Markdown 程序化技能 | 企业 Skill 市场基础 | `Skill` / `debugSkill` |
| `hooks` (`codex-hooks`) | 生命周期钩子 | 企业生命周期扩展 | `Hook` trait |
| `tools` (`codex-tools`) | 工具路由（ToolRouter） | 统一工具定义 + 动态工具解析 | `ToolRouter` / `ToolManifest` |
| `model-provider` | 模型 provider 抽象 | 接自有 Model Gateway | `ModelProvider` trait |
| `model-provider-info` | Provider 信息管理 | 模型配置 | `ProviderInfo` |
| `responses-api-proxy` | Responses API 代理 | **复用**：指向自有 Model Gateway | `ResponsesApiProxy` |
| `ollama` | Ollama 本地模型 provider | 私有化部署用 | `OllamaProvider` |
| `lmstudio` | LM Studio 本地模型 provider | 私有化部署用 | `LmStudioProvider` |
| `models-manager` | 模型管理 | 模型配置中心 | `ModelsManager` |
| `connectors` | 连接器 | 外部系统对接 | `Connector` trait |

### 6.4 多 Agent 协作模块组

| Codex crate | 职责 | Nexus 中的角色 | 关键接口/方法 |
|---|---|---|---|
| `collaboration-mode-templates` | 协作模板（编排者-工作者/对等/批评对抗） | 团队 Agent 编排参考 | `CollaborationMode` |
| `agent-roles` | Agent 角色定义 | 多角色协作 | `AgentRole` |
| `agent-identity` | Agent 身份管理 | 多租户 Agent 身份映射 | `AgentIdentity` |
| `agent-graph-store` | Agent 关系图存储 | 协作拓扑持久化 | `AgentGraphStore` |

### 6.5 执行与 CLI 模块组

| Codex crate | 职责 | Nexus 中的角色 | 关键接口/方法 |
|---|---|---|---|
| `cli` (`codex-cli`) | CLI 入口 | 开发者体验（自定义登录） | `codex` binary |
| `tui` (`codex-tui`) | TUI 交互界面 | 内部调试用 | `Tui` |
| `codex-client` | Codex 客户端库 | 适配层客户端 | `CodexClient` |
| `exec` (`codex-exec`) | 一次性执行模式 | CI 场景用 | `codex exec` binary |
| `exec-server` | 执行服务器 | 远程执行支持 | `ExecServer` |
| `exec-server-protocol` | 执行服务器协议 | 协议定义 | `ExecServerProtocol` |

### 6.6 可观测模块组

| Codex crate | 职责 | Nexus 中的角色 | 关键接口/方法 |
|---|---|---|---|
| `otel` (`codex-otel`) | OpenTelemetry 集成 | trace 串联（用户→任务→工具→模型） | `otel::init()` / span |
| `analytics` (`codex-analytics`) | 分析数据收集 | 使用分析 | `Analytics` |
| `diagnostics` (`codex-diagnostics`) | 诊断工具 | 故障定位 | `Diagnostics` |
| `rollout-trace` | Rollout 追踪 | 会话回放与审计 | `RolloutTrace` |
| `otel-trace-websocket` | OTel trace WS 传输 | 实时 trace 推送 | `OtelTraceWebSocket` |

### 6.7 适配层（唯一贴着 Codex 写的代码）

薄适配，职责严格限定四件事：

| # | 职责 | 做什么 | 不做什么 |
|---|---|---|---|
| 1 | 协议桥接 | app-server JSON-RPC 事件流 → 内部事件总线（Kafka/NATS/Postgres LISTEN/NOTIFY）；用 `generate-json-schema`/`generate-ts` 生成类型纳入 CI | 不改协议本身 |
| 2 | 配置生成 | 按 `tenant+role+workspace+risk_level` 生成 `config.toml`、`execpolicy.rules`、MCP 声明、Skills 清单 | 不改配置格式 |
| 3 | 命令包装 | 暴露 `start`/`resume`/`interrupt`/`approve`/`fork`/`archive` 六动作 | 不改 `run_turn` |
| 4 | 健康检查与回收 | 探测 app-server 存活、处理僵尸进程、Pod 退出前上传 rollout | 不改工具路由/压缩算法/execpolicy 求值器 |

---

## 7. L6 模型层 — 模块详细设计

### 7.1 模块清单

| 模块 | 职责 | 输入 | 输出 | 上游依赖 | 下游依赖 | 复用 Codex |
|---|---|---|---|---|---|---|
| Model Gateway | 统一入口（LiteLLM/自建） | 模型调用请求 | 模型响应 + token 用量 | L5 Harness（出站） | 外部模型 API | 部分（`responses-api-proxy`） |
| 多模型路由 | 按任务复杂度分档路由 | 任务特征 | 模型选择 | L5 Harness | Model Gateway | 否 |
| Responses 代理 | Codex Responses API 代理 | Codex 格式请求 | Responses 格式响应 | L5 `responses-api-proxy` | Model Gateway | 是（`responses-api-proxy`） |
| Token 计量 | 四维计量（prompt/cached/reasoning/output） | 模型调用日志 | usage_record | L5 Harness | L3 配额计费 | 否 |
| 故障转移 | 主→备→重试 | 模型调用失败 | 降级/重试决策 | Model Gateway | 备用模型 | 否 |
| Prompt Caching | 版本化前缀缓存 | 系统提示+工具描述 | 缓存命中率 | L5 `prompts` | Model Gateway | 否 |

### 7.2 关键设计

- **统一入口**：所有模型调用经 Model Gateway。Codex 侧复用 `model-provider` + `responses-api-proxy` 抽象指向自有端点。
- **路由策略**：分类/抽取走经济模型，规划/复杂工具编排走强模型，长任务中段用中档。必须在自有评测集上校准。
- **故障转移**：主超时/限流 → 备模型 → 仍失败任务进入"待重试"。
- **私有化**：数据不出域租户指向自建 vLLM/Ollama（Codex 内置 `ollama`/`lmstudio` provider）。

---

## 8. L7 存储与治理 — 模块详细设计

### 8.1 模块清单

| 模块 | 职责 | 存储选型 | 关键设计 | 上游依赖 |
|---|---|---|---|---|
| Postgres | 结构化元数据 + 会话事件 | PostgreSQL（RLS + 分区） | RLS 兜底租户隔离；`item` 表按 tenant_id+时间分区；`audit_log`/`usage_record` 只追加 | L3 控制面 |
| 对象存储 | rollout/快照/产物 | S3/MinIO（按租户前缀 + CMK） | 大对象外置；`content_ref` 指向对象存储；禁用租户 CMK → 数据不可解密 | L3/L4/L5 |
| 向量库 | 知识库向量索引 | pgvector / Milvus | chunk 携带 `tenant_id + acl_tags + permission_version`；先过滤后召回 | L3 知识库 RAG |
| 审计日志 | WORM 追加写 + SIEM 投递 | 追加写存储 | 应用层无 DELETE/UPDATE 权限；独立角色写入，审计账号只读 | L3 全子系统 |
| OTel 追踪 | 全链路 trace | ClickHouse + Grafana | trace 串"用户→任务→工具→模型"；每次失败可定位到 LLM/tool/data/policy 层 | L3/L5 |
| 评测中心 | 模型/提示/工具/策略变更回归 | 自建 + LLM-as-judge | 黄金集+对抗集+生产采样集；CI 门禁：任一变更必须跑回归 | L3/L5/L6 |

### 8.2 存储分层约束

- `item` 表是最大最热的表，按 `tenant_id` + 时间做分区；大字段外置（超 64KB 只存对象存储引用 + 摘要）。
- `audit_log` 与 `usage_record` 只追加；应用层账号没有 DELETE/UPDATE 权限。
- `permission_snapshot_hash` 必须有——没有它事后无法证明"任务运行时的权限边界"。
- 所有跨租户查询强制 RLS；即使应用层已加 `tenant_id`。

---

## 9. 安全与合规贯穿层 — 模块详细设计

### 9.1 模块清单

| 域 | 职责 | 关键设计 | 贯穿层 |
|---|---|---|---|
| 租户隔离 | 四重取证（逻辑+运行时+密钥+存储） | Postgres RLS 兜底 + namespace + NetworkPolicy + 按租户 CMK + 对象存储前缀 | L3+L4+L7 |
| KMS | 按租户 CMK | 租户禁用后其数据不可解密 | L7 对象存储 + L3 连接器 |
| 网络策略 | 沙箱出站默认全禁 | 仅放行 Model Gateway + MCP Gateway | L4 沙箱 Pod |
| 审计留存 | 不可篡改审计 | WORM + SIEM 投递；独立写入角色 | L3 全子系统 + L7 |
| 内容安全 | 产物病毒/敏感扫描 | 扫描通过后才对用户可见 | L4 产物上传 |
| 红队演练 | 季度跨租户越权演练 | 尝试从 A 租户沙箱访问 B 资源 | 全层 |

### 9.2 沙箱内零长期密钥

| 密钥类型 | 处理 |
|---|---|
| 模型 API Key | 绝不进沙箱，只到 Model Gateway，任务令牌换调用 |
| MCP/企业 API 凭据 | MCP Gateway 侧车持有，短期委托令牌向控制面换 |
| Git 凭据 | 凭据代理 + 只读/限定分支，禁推保护分支 |
| 云厂商 AK/SK | IRSA/Workload Identity 换短期角色凭证，不落盘 |
| 用户 OAuth 令牌 | 加密存控制面密钥库，按需换短期 access token，结束吊销 |

### 9.3 沙箱启动自检清单

- [ ] Landlock/seccomp/Seatbelt 可用性验证通过
- [ ] 出站网络仅能到达两个白名单地址
- [ ] 镜像内无长期密钥文件（镜像扫描）
- [ ] 只读根文件系统 + 非 root 用户
- [ ] 资源限额（CPU/内存/磁盘/PID）已生效

---

## 10. 模块间依赖关系总结

### 10.1 跨层依赖矩阵

```
L1 接入层 ──→ L2 网关层 ──→ L3 控制平面 ──→ L4 执行平面 ──→ L5 Harness
                    │              │              │              │
                    │              │              │              ├──→ L6 模型层
                    │              │              │              │
                    │              │              ↓              ↓
                    └──────────────┴──────────→ L7 存储与治理 ←──┘
                                              │
                                    安全合规贯穿（全层）
```

### 10.2 依赖方向约束

| 依赖方向 | 允许 | 禁止 |
|---|---|---|
| 上 → 下（L1→L2→...→L7） | 经接口（协议/事件/API） | 直接引用下层 crate/模块 |
| L5 → L6（模型调用） | 经 Model Gateway 出站 | L5 直连外部模型 API |
| L5 → L7（rollout 上传） | 经对象存储 SDK | L5 直连 Postgres |
| L3 → L5（任务控制） | 经 app-server JSON-RPC | L3 直接调用 L5 crate |
| L5 → L3（反向） | **禁止** | L5 不感知控制面存在 |
| L6 → L3（反向） | **禁止** | L6 不感知控制面存在 |

### 10.3 L5 内部 crate 依赖关键路径

```
core (run_turn)
  ├──→ app-server-protocol (协议类型)
  ├──→ state / thread-store / rollout (持久化)
  ├──→ sandboxing → linux-sandbox / bwrap / windows-sandbox-rs
  ├──→ execpolicy (命令策略求值)
  ├──→ codex-mcp / rmcp-client (MCP 工具)
  ├──→ tools (ToolRouter)
  ├──→ model-provider → responses-api-proxy / ollama / lmstudio
  ├──→ collaboration-mode-templates → agent-roles → agent-identity → agent-graph-store
  └──→ otel / analytics / diagnostics / rollout-trace (可观测)
```

---

## 11. 配套产物索引

| 产物 | 文件 |
|---|---|
| 分层依赖图（SVG 源） | `module-layering.svg` |
| 分层依赖图（PNG 位图） | `module-layering.png` |
| Harness crate 拓扑图 | `harness-crate-topology.svg` |
| 控制面子系统设计图 | `control-plane-subsystems.svg` |
| 交互报告（HTML） | `module-layering-report.html` |
| 生成脚本 | `_gen_svg.py` |

本方案与 `../Nexus 基于CodexHarness的企业级Agent平台_系统设计与实施路线图.md` §2.2 八层架构、§3 分层详细设计对齐，为其工程化落地版的模块级展开。
