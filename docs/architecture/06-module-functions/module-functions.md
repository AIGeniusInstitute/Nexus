# Nexus 企业级 AI Agent 平台 — 系统模块功能清单

> 产物编号：任务二-6 · 系统模块功能清单矩阵
> 基座：`~/Nexus`（基于 OpenAI Codex Harness，codex-rs 105 crate Rust 工作区）
> 日期：2026-09-06 · 配套图：`module-functions.svg` / `.png` · 交互报告：`module-functions-report.html`

---

## 0. 概述

本文档按八层架构逐模块列出功能清单矩阵。每模块含：模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate 名）| 是否自建 | 优先级（P0/P1/P2）| 阶段（M1–M12）。

**图例**：
- **复用类型**：`Codex` = 直接复用 Codex crate（黑盒不改）；`自建` = 全新自研；`部分` = 部分复用 Codex + 自建外壳
- **优先级**：P0 = MVP 必须；P1 = 多租户/治理必须；P2 = 规模化/生态
- **阶段**：M0 = PoC；M1–M4 = 单租户 MVP；M5–M7 = 多租户与隔离；M8–M10 = 可靠性与治理；M11–M12+ = 规模化与生态

**复用/自建统计**：

| 类型 | 模块数 | 占比 |
|---|---:|---:|
| 复用 Codex（黑盒） | 10 | 22% |
| 自建 | 26 | 57% |
| 部分复用 | 10 | 22% |
| **合计** | **46** | 100% |

---

## 1. L1 接入层 · Access Layer

| # | 模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate） | 是否自建 | 优先级 | 阶段 |
|---|---|---|---|---|---|---|---|---|---|---|
| 1.1 | Web 门户 | L1 | React+WS 前端：会话列表、任务时间线（Item 流）、审批抽屉、产物预览、Diff 查看 | 用户操作、WS 事件流 | 渲染页面、操作指令 | L2 API Gateway、L2 WS 网关 | 否 | 是 | P0 | M1–M2 |
| 1.2 | IM Bot | L1 | 飞书/钉钉/企微/Slack Bot：审批推送（卡片消息）、任务通知、简易交互 | 审批事件、任务状态变更 | IM 卡片、审批回写 | L3 审批中心、L2 API Gateway | 否 | 是 | P0 | M3 |
| 1.3 | IDE 插件 | L1 | VS Code / JetBrains 扩展：远端 Thread 映射本地、代码 Diff 内联审批 | IDE 上下文、用户指令 | 本地编辑器操作、远端任务指令 | L2 API Gateway、L5 app-server-protocol | 部分（`app-server-protocol` 协议复用） | 是（外壳） | P1 | M4 |
| 1.4 | OpenAPI+Webhook | L1 | REST API 供业务系统调用 Agent 能力 + Webhook 任务完成回调 | 外部 API 请求 | JSON 响应、Webhook POST | L2 API Gateway、L3 任务编排 | 否 | 是 | P0 | M2 |
| 1.5 | CLI | L1 | 复用 codex CLI + 自定义登录：开发者体验入口 | 命令行参数 | 终端输出、任务提交 | L2 认证中间件 | 部分（`cli` crate） | 是（登录层） | P1 | M1 |

### L1 模块功能详述

**1.1 Web 门户** — React 单页应用 + WebSocket 实时连接。核心视图：
- **会话列表**：按 `thread` 实体展示，支持搜索/筛选/归档
- **任务时间线**：渲染 `item` 事件流（user_message / agent_message / reasoning / command_exec / file_change / mcp_call / approval / error），实时增量更新
- **审批抽屉**：侧滑面板展示 `ApprovalTicket` 详情（参数脱敏、Diff 预览、风险等级、影响范围），支持批准/拒绝/修改后批准
- **产物预览**：对象存储产物在线预览（代码、文档、图片）
- **Diff 查看**：文件变更差异对比，支持行级审查

**1.2 IM Bot** — 审批推送最佳渠道。用交互式卡片消息承载"批准/拒绝/修改后批准"三按钮，回调签名验证防伪造。按风险等级选渠道：高危→全渠道推送+邮件；中危→IM+Web；低危→仅 Web 抽屉。

**1.3 IDE 插件** — 直接复用 Codex 的 `app-server-protocol`（JSON-RPC）与 IDE 扩展思路，把远端 Thread 映射到本地编辑器。代码 Diff 内联展示、工具审批可在 IDE 内完成。**统一约束：所有入口不直连 Harness，必经 L2 网关**。

**1.4 OpenAPI+Webhook** — RESTful API（`POST /v1/threads`、`POST /v1/threads/{id}/turns`、`GET /v1/threads/{id}/items`）。Webhook 在任务完成/审批请求/配额告警时回调业务系统，支持 HMAC 签名验证与幂等投递。

**1.5 CLI** — 复用 `codex` CLI crate 的本地交互能力，叠加企业登录（OIDC token 刷新）、远端 Thread 操作（start/resume/interrupt/approve）。开发者调试与 CI 集成入口。

---

## 2. L2 网关层 · Gateway Layer

| # | 模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate） | 是否自建 | 优先级 | 阶段 |
|---|---|---|---|---|---|---|---|---|---|---|
| 2.1 | API Gateway | L2 | REST 路由、请求校验、幂等键（`Idempotency-Key`）、租户级+用户级+IP 级限流 | HTTP 请求 | 路由后请求、429/403 响应 | L3 身份租户、L3 配额计费 | 否 | 是 | P0 | M1 |
| 2.2 | WebSocket 网关 | L2 | 会话事件实时推送；订阅关系由"用户对 Thread 的读权限"驱动，权限变更立即断连 | WS 连接请求、事件流 | WS 推送消息 | L3 身份租户、L5 app-server 事件流 | 否 | 是 | P0 | M1–M2 |
| 2.3 | 认证中间件 | L2 | OIDC/SAML 对接企业 IdP、SCIM 同步组织架构、服务账号 mTLS | SSO token、mTLS 证书 | 认证上下文（userId、tenantId、roles） | L3 身份租户 | 否 | 是 | P0 | M1 |
| 2.4 | 配额预扣 | L2 | 网关层粗粒度拦截：请求准入时预扣 token/工具调用配额，防超卖 | 请求上下文 | 准入/拒绝决策 | L3 配额计费 | 否 | 是 | P0 | M2 |

### L2 模块功能详述

**2.1 API Gateway** — 统一入口路由。关键设计：
- **幂等**：`Idempotency-Key` Header 避免重复提交产生双份计费
- **限流三层**：租户级（套餐配额）→ 用户级（并发上限）→ IP 级（防爆破）
- **请求校验**：JSON Schema 校验请求体，拒绝畸形输入

**2.2 WebSocket 网关** — 事件推送通道。订阅权限模型：
- 用户订阅 Thread 事件前，校验 `membership(user, workspace) ∩ thread.owner_workspace`
- 权限变更（角色降级、成员移除）→ 立即断开该用户对该 Thread 的 WS 连接
- 事件先落 Postgres 后推 WS（可回放），WS 仅展示不作为真相

**2.3 认证中间件** — 企业身份集成：
- **OIDC**：Authorization Code + PKCE 对接 Keycloak / Okta / Azure AD
- **SAML**：遗留企业 IdP 兼容
- **SCIM 2.0**：自动同步组织架构（用户/部门/组），增删改推送到平台
- **mTLS**：服务账号（Agent 身份）用客户端证书认证，证书指纹绑定 `service_account`

**2.4 配额预扣** — 粗粒度拦截层：
- 请求准入时预扣 token 配额（按历史均值 × 安全系数）
- 预扣失败 → 429 告知排队位置
- 细粒度结算在 L3 配额计费（实际用量 vs 预扣量，多退少补）

---

## 3. L3 控制平面 · Control Plane（自建核心）

| # | 模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate） | 是否自建 | 优先级 | 阶段 |
|---|---|---|---|---|---|---|---|---|---|---|
| 3.1 | 身份租户 | L3 | Tenant/OrgUnit/User/ServiceAccount/Role/Membership 模型；RBAC+ABAC 授权引擎 | 用户/Agent 身份、资源请求 | 授权决策（allow/deny） | L2 认证中间件 | 否 | 是 | P0 | M1, M5 |
| 3.2 | 任务编排 | L3 | Temporal Workflow 持久化编排：申请 Pod→建连→下发任务→消费事件→处理审批→收尾结算；两层循环（外层平台+内层 Harness） | 任务提交、Pod 事件、审批回写 | Workflow 状态、调度指令 | L4 Runtime 池调度、L5 app-server | 否 | 是 | P0 | M2 |
| 3.3 | 审批中心 | L3 | ApprovalTicket 生命周期（pending→decided）；HITL 跨进程桥接；6 种边界处理；IM/Web/邮件多渠道推送 | app-server 审批请求事件 | 审批决策回写 | L2 WS 网关、L1 IM Bot/Web | 否 | 是 | P0 | M3 |
| 3.4 | 策略中心 | L3 | 策略对象求值（tenant/role/workspace/tool/risk → allow/deny/require_approval/dual）；漂移防护；execpolicy 规则集生成下发 | 策略对象、请求上下文 | 决策结果、config.toml + execpolicy 规则集 | L5 ExecPolicy、L5 OS沙箱 | 部分（`execpolicy` crate 规则语言复用） | 是（下发层） | P0 | M3 |
| 3.5 | 配额计费 | L3 | 四维计量（token/tool_call/sandbox_second/storage）；归因（tenant→org→user→thread→turn→model）；预算软硬阈值；优雅暂停 | 实时用量事件 | 配额状态、计费记录、熔断信号 | L2 配额预扣、L5 Agent Loop | 否 | 是 | P0 | M4, M6 |
| 3.6 | 连接器治理 | L3 | 连接器分级（official/enterprise_private/community）；质量分（可用性/P95/错误率/最小权限）；MCP Gateway 管理；凭据代理 | 连接器注册、健康检查 | 质量评分、上下线决策 | L4 MCP Gateway、L4 凭据代理 | 否 | 是 | P1 | M7 |
| 3.7 | 知识库/RAG | L3 | ACL 随索引写入（chunk 携带 tenant_id+acl_tags+permission_version）；混合召回（稠密+稀疏）+ rerank；引用溯源 | 检索请求、知识库 chunk | 检索结果（附 chunk_id + 权限版本） | L4 MCP Gateway | 否 | 是 | P2 | M11 |

### L3 模块功能详述

**3.1 身份租户** — 平台身份体系核心：

```
Tenant（租户）
  └── OrgUnit（组织单元，可多级，映射企业部门树）
        └── User / ServiceAccount
              └── Role（owner / admin / developer / auditor / viewer）
  └── Workspace（工作环境：绑定仓库、数据集、连接器、知识库范围）
        └── Membership（成员 × 角色 × 资源作用域）
```

- **RBAC 打底 + ABAC 兜底**：ABAC 属性取 `tenant_id`、`org_path`、`env(prod/staging)`、`data_classification`、`risk_level`、`time_window`
- **三个身份区分**：用户身份（人）、Agent 身份（服务账号，权限 = 用户权限子集 ∩ 显式授予）、连接器身份（MCP/OAuth 委托令牌）
- **权限继承**：`Agent 可用权限 = 用户权限 ∩ 工作区权限 ∩ Agent 角色上限 ∩ 策略中心允许`（四者取交集，任何一项为空即拒绝）

**3.2 任务编排** — 不自建 while-loop，用 Temporal 持久化 Workflow：
- **外层（平台）Workflow**：管资源与账本（Pod 生命周期、配额、审计），长周期
- **内层（Harness）run_turn**：管模型与工具，短周期
- **调度策略**：租户权重队列 + 优先级 + 并发上限；Prod 与实验任务分池
- **可恢复**：审批等几小时 → Workflow 挂起；Pod 崩 → Workflow 恢复重建

**3.3 审批中心** — 最复杂的桥接层（§4.3 专题）：

```
① app-server 发出审批请求事件
② 适配层解析 → 控制面创建 ApprovalTicket（pending）
   ├─ 内容：thread_id / turn_id / item_seq / 工具名 / 参数（脱敏）/ diff / 风险等级
   ├─ 策略：谁可批（单人/双人/角色）、超时动作（默认拒绝）
   └─ 快照：审批时上下文（事后可回溯"批了什么"）
③ 推送：Web 抽屉 + IM 卡片 + 邮件（按风险等级选渠道）
④ 用户决策（批准/拒绝/修改后批准/转交）
⑤ 决策先落库（decided）再回写 app-server
⑥ app-server 继续/中止 → 结果回事件流，闭环
```

**六种边界处理**：

| 边界 | 处理 |
|---|---|
| Pod 在等待审批时崩了 | 审批状态在 DB，Pod 重建 resume，`item_seq` 去重，已决策直接重放 |
| 审批超时 | 按策略默认动作（建议**拒绝**），通知申请人 |
| 审批期间用户权限被撤销 | 决策时重新校验审批人权限，失效则 ticket 作废 |
| 用户修改参数后批准 | 重新走策略求值（改参数 = 新请求） |
| 批量相似请求 | "本次任务内同类操作一律批准"作用域（限目录/工具/有效期 ≤ 1h） |
| 审计 | 请求快照、决策人、时间、理由全部不可篡改留存 |

**3.4 策略中心** — 权限第三层的政策下发载体：
- **策略对象**：`{tenant, org_path, role, workspace, tool, action, risk_level, data_classification}`
- **决策结果**：`allow` / `deny` / `require_approval` / `require_dual_approval`（四眼原则）/ `allow_with_audit_only`
- **求值时机**：任务准入一次 + **每次高危工具调用前一次**（准入结果非永久通行证）
- **漂移防护**：策略快照写入任务上下文；运行中策略变更"不溯及已批准动作、对新动作用新策略"
- **下发产物**：按 `(tenant, role, workspace, risk_level)` 生成 `config.toml` + `execpolicy.rules` + `enabled_tools` 白名单 + `AGENTS.md`，运行时注入 Pod，任务结束即焚

**3.5 配额计费** — 四维计量 + 归因 + 预算控制：
- **四维**：token（prompt/cached/reasoning/output）、工具调用次数、沙箱运行时长、存储与出站流量
- **归因链**：`tenant → org_unit → user → thread → turn → model`
- **预算控制**：软阈值告警 → 降档到经济模型 → 硬阈值熔断（优雅暂停：保存 rollout 后回收 Pod，预算恢复可 resume）

**3.6 连接器治理** — MCP 连接器全生命周期管理：
- **分类分级**：官方认证 / 企业私有 / 社区（默认禁用，需管理员显式开启）
- **质量分**：可用性 × P95 延迟 × 错误率 × 权限最小化程度 → 低于阈值自动降级/下线
- **安全约束**：项目级 `.codex/config.toml` 只在受信任工作区生效；`enabled_tools` 白名单优先于 `disabled_tools`；破坏性注解工具恒定需审批

**3.7 知识库/RAG** — 企业知识检索（ACL 是核心）：
- **ACL 随索引写入**：每个 chunk 携带 `tenant_id + acl_tags + permission_version`
- **检索流程**：metadata/ACL 过滤 → 稠密 + 稀疏混合召回 → rerank → 只回填支撑结论的片段（附 `chunk_id` 与权限版本）
- **与 Harness 衔接**：知识检索做成 MCP 工具或自定义 Tool；检索权限在 Gateway 侧强制，不依赖模型"自觉遵守"

---

## 4. L4 执行平面 · Execution Plane（Harness 托管外壳）

| # | 模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate） | 是否自建 | 优先级 | 阶段 |
|---|---|---|---|---|---|---|---|---|---|---|
| 4.1 | Runtime 池调度 | L4 | 一 Turn 一 Pod；预热池（warm pool）；冷启动 < 5s；租户级并发上限 + 全局上限 + 队列等待位；空闲超时销毁（15–30 min） | 调度指令、Pod 状态 | Pod 实例、调度结果 | L3 任务编排、L7 Postgres | 否 | 是 | P0 | M2, M11 |
| 4.2 | 三层沙箱 | L4 | ①容器/微虚拟机层（K8s Pod + Seccomp/AppArmor；高敏用 Kata/Firecracker）②命令级 OS 沙箱（Codex 自带）③网络层（NetworkPolicy 默认全禁出站） | Pod 创建请求 | 隔离的执行环境 | L5 OS沙箱、L7 网络策略 | 部分（`sandboxing`/`linux-sandbox`/`bwrap`/`windows-sandbox-rs`） | 是（容器层+网络层） | P0 | M0, M2 |
| 4.3 | Workspace 供给 | L4 | Git 仓库 shallow clone / PVC 快照挂载 / git worktree 并行隔离 / AGENTS.md 规范注入 / 非代码场景挂载租户对象存储前缀 | 任务上下文（仓库/数据集/连接器配置） | 就绪的工作目录 | L7 对象存储、`worktree` crate | 部分（`worktree` crate） | 是（供给逻辑） | P0 | M2 |
| 4.4 | MCP Gateway | L4 | 同 Pod 侧车：凭据注入（短期委托令牌从控制面换取）、`enabled_tools` 白名单强制、出站请求审计 + 敏感字段脱敏、转发到真实 MCP Server | MCP 工具调用请求 | 代理后请求、审计日志 | L3 连接器治理、L4 凭据代理 | 否 | 是 | P1 | M7 |
| 4.5 | 凭据代理 | L4 | 签发短期令牌（JWT，audience 限定 Model Gateway / MCP Gateway，TTL = 任务超时+缓冲）；绑定 `tenant_id + thread_id + turn_id + 权限快照哈希`；支持即时吊销 | 令牌签发请求、吊销指令 | 短期 JWT、吊销列表 | L3 身份租户、L3 策略中心 | 否 | 是 | P1 | M7 |

### L4 模块功能详述

**4.1 Runtime 池调度** — Pod 生命周期管理：

| 项 | 设计 |
|---|---|
| 任务粒度 | 一个 Turn = 一个 Pod（长任务可复用，有最大时长，超期强制结算并 resume 到新 Pod） |
| 镜像 | 预装 Codex 二进制 + 语言工具链的基础镜像；按语言/场景分多个镜像 |
| 冷启动优化 | 预热池 + Workspace 快照（PVC snapshot / stash）；目标 < 5s |
| 并发 | 租户级并发上限 + 全局上限 + 队列等待位；超限排队并实时告知位置 |
| 销毁 | 任务结束或空闲超时后销毁；销毁前必做：上传 rollout、结算、审计 |

**4.2 三层沙箱** — 缺一不可：

| 层 | 技术 | 防什么 | 复用 |
|---|---|---|---|
| ① 容器/微虚拟机层 | K8s Pod + 受限 Seccomp/AppArmor；高敏租户用 Kata/Firecracker | 租户间横向逃逸、内核攻击面 | 自建 |
| ② 命令级 OS 沙箱 | macOS Seatbelt；Linux Landlock+seccomp+bubblewrap；Windows restricted token | Agent 在工作区内乱跑命令、读 SSH key、外传数据 | **Codex 自带**（`sandboxing`/`linux-sandbox`/`bwrap`/`windows-sandbox-rs`） |
| ③ 网络层 | NetworkPolicy 默认拒绝全部出站，仅放行 Model Gateway + MCP Gateway | 数据外泄、C2 回连、依赖投毒外联 | 自建 |

> **注意**：Linux 下 Codex 沙箱在容器中可能因宿主不支持 Landlock/seccomp 而失效。因此容器层不能省，且需启动自检验证沙箱可用性（自检失败 = 禁止调度生产任务）。

**4.3 Workspace 供给** — 工作目录准备：
- **代码场景**：从 Git 仓库 shallow clone 或挂载 PVC 快照；git worktree 做并行任务隔离
- **非代码场景**：挂载租户对象存储前缀下工作目录（只读 + 可写 scratch 分区）
- **AGENTS.md 注入**：企业规范、项目约定、安全红线写成 AGENTS.md 随 Workspace 下发——最廉价的"企业规则下达"

**4.4 MCP Gateway** — 凭据注入的关键：

```
Codex (MCP Client)
   │  stdio / http
   ▼
MCP Gateway Sidecar（同 Pod，独立凭据域）
   ├─ 从控制面换取短期凭据（TTL ≤ 任务时长）
   ├─ 强制 enabled_tools 白名单
   ├─ 出站请求审计 + 敏感字段脱敏
   └─ 转发到真实 MCP Server / 企业 API
```

**要点**：Codex 的 `config.toml` 里**不出现任何真实密钥**，只出现指向 Gateway 的本地地址与任务令牌。

**4.5 凭据代理** — 短期令牌签发与吊销：
- JWT `audience` 限定为 Model Gateway / MCP Gateway
- TTL = 任务超时 + 缓冲（通常 ≤ 2h）
- 绑定 `tenant_id + thread_id + turn_id + 权限快照哈希`，任一不符即拒绝
- 即时吊销：用户点"停止任务" → 控制面吊销 → Gateway 拒绝后续调用

**沙箱启动自检清单**（自检不过禁止调度）：
- [ ] Landlock / seccomp / Seatbelt 可用性验证通过
- [ ] 出站网络仅能到达两个白名单地址
- [ ] 镜像内无长期密钥文件（镜像扫描）
- [ ] 只读根文件系统 + 非 root 用户
- [ ] 资源限额（CPU / 内存 / 磁盘 / PID）已生效

---

## 5. L5 Harness · Agent 内核（复用 Codex，黑盒不改）

| # | 模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate） | 是否自建 | 优先级 | 阶段 |
|---|---|---|---|---|---|---|---|---|---|---|
| 5.1 | Agent Loop | L5 | `run_turn` 七阶段：admission → snapshot → sampling → tool dispatch → writeback → compaction → complete/interrupt | 用户消息、模型采样、工具结果 | Turn 状态、Item 事件流 | L6 模型层、`tools` crate | 是（`core`） | 否 | P0 | M0 |
| 5.2 | 工具路由 | L5 | 统一工具定义（JSON Schema）、MCP 工具适配、动态工具解析、四层调用形态 | 工具调用请求 | 工具执行结果 | `core` Agent Loop | 是（`tools`） | 否 | P0 | M0 |
| 5.3 | ExecPolicy | L5 | Starlark 规则引擎：命令 allow/deny 评估（parser + evaluator + rule types），与会话层解耦 | 命令行、策略规则集 | allow/deny 决策 | L3 策略中心（下发规则集） | 是（`execpolicy`） | 否 | P0 | M0 |
| 5.4 | OS 沙箱 | L5 | macOS Seatbelt（sandbox-exec + .sbpl）、Linux Landlock+seccomp+bubblewrap、Windows restricted token | 进程创建请求 | 受限进程 | L4 三层沙箱（容器层兜底） | 是（`sandboxing`/`linux-sandbox`/`bwrap`/`windows-sandbox-rs`） | 否 | P0 | M0 |
| 5.5 | 上下文压缩 | L5 | 自动上下文压缩（`run_auto_compact`）、推理轨迹保留（retained reasoning）、compact_remote_v2 远程压缩 | 上下文将满信号 | 压缩后上下文 | `core` Agent Loop | 是（`core`） | 否 | P0 | M0 |
| 5.6 | Skills/Hooks | L5 | 可复用 Markdown 程序化技能（`skills` crate）；生命周期钩子（`hooks` crate）——企业 Skill 市场基础 | Skill 定义、生命周期事件 | Skill 执行结果 | `core` Agent Loop | 是（`skills`/`hooks`） | 否 | P1 | M4, M11 |
| 5.7 | MCP 客户端 | L5 | 连接外部 MCP 服务器（`codex-mcp`）；把 Codex 自身暴露成 MCP 工具给其他 Agent 调（`rmcp-client`） | MCP 工具调用 | MCP 响应 | L4 MCP Gateway | 是（`codex-mcp`/`rmcp-client`） | 否 | P0 | M0 |
| 5.8 | 协议集成面 | L5 | app-server JSON-RPC 2.0（stdio/unix socket/WS）；Thread→Turn→Item 三原语；`generate-ts`/`generate-json-schema` 生成类型 | JSON-RPC 请求 | JSON-RPC 事件流 | L3 任务编排、L2 WS 网关 | 是（`app-server`/`app-server-protocol`） | 否 | P0 | M0 |
| 5.9 | 持久化 | L5 | Thread 跨进程重启可恢复/fork/回滚；本地 SQLite（`state`）+ rollout 文件（`rollout`）；历史记录（`history`/`message-history`/`rollout-trace`） | 事件流、Thread 操作 | 本地持久化状态 | L3 任务编排（事件桥接云端） | 是（`state`/`thread-store`/`rollout`/`history`） | 否 | P0 | M0 |
| 5.10 | 协作编排 | L5 | 子 Agent 派生与协作模板；Agent 角色定义；Agent 身份管理；Agent 图存储 | 协作请求 | 子 Agent 实例、协作结果 | L3 任务编排 | 是（`collaboration-mode-templates`/`agent-roles`/`agent-identity`/`agent-graph-store`） | 否 | P2 | M11–M12 |

### L5 模块功能详述

**5.1 Agent Loop** — Codex 核心 `run_turn` 七阶段（`core` crate）：

| 阶段 | 名称 | 职责 |
|---|---|---|
| 1 | admission | 准入检查：approval_policy + sandbox.mode + execpolicy 预检 |
| 2 | snapshot | 上下文快照：当前 Thread 状态 + 工具可见性 |
| 3 | sampling | 模型采样：经 Model Gateway 调用 LLM，获取推理 + 工具调用意图 |
| 4 | tool dispatch | 工具调度：ToolRouter 路由到具体工具（shell/MCP/custom） |
| 5 | writeback | 结果写回：工具结果回灌上下文，生成 Item 事件 |
| 6 | compaction | 压缩判定：上下文将满时触发 auto compact |
| 7 | complete/interrupt | Turn 完成（产物+rollout 上传）或中断（用户 interrupt/审批拒绝） |

**5.2 工具路由** — `tools` crate 的 ToolRouter：
- 统一工具定义（JSON Schema 输入输出规范）
- MCP 工具适配：自动发现 MCP server 暴露的工具
- 动态工具解析：按上下文动态启用/禁用工具
- 四层调用形态：direct call / MCP call / shell exec / custom tool

**5.3 ExecPolicy** — `execpolicy` crate 的 Starlark 规则引擎：
- parser：解析命令行令牌
- evaluator：按规则集求值（allow/deny/require_approval）
- rule types：支持通配符、路径限制、参数约束
- 与会话层解耦：规则集由 L3 策略中心按租户/角色生成下发

**5.4 OS 沙箱** — Codex 自带三平台命令级沙箱：

| 平台 | crate | 技术 |
|---|---|---|
| macOS | `sandboxing` | Seatbelt（sandbox-exec + .sbpl profile） |
| Linux | `linux-sandbox` + `bwrap` | Landlock + seccomp + bubblewrap |
| Windows | `windows-sandbox-rs` | Restricted token + AppContainer |

> 注意：Linux 容器内可能因宿主不支持 Landlock/seccomp 而失效，容器层（L4）不能省。

**5.5 上下文压缩** — `core` crate 的 compact 逻辑：
- `run_auto_compact`：上下文接近 token 上限时自动触发
- 保留推理轨迹（retained reasoning）：压缩不丢失关键推理链
- `compact_remote_v2`：支持远程压缩（大上下文 offload）

**5.6 Skills/Hooks** — 企业 Skill 市场基础：
- `skills` crate：Markdown 程序化技能，可复用、可分享、可版本管理
- `hooks` crate：生命周期钩子（Turn 开始/结束、工具调用前后、审批请求时）

**5.7 MCP 客户端** — `codex-mcp`（客户端）+ `rmcp-client`（协议库）：
- 连接外部 MCP 服务器（stdio / http）
- 把 Codex 自身暴露成 MCP 工具给其他 Agent 调
- 经 L4 MCP Gateway 代理（凭据注入 + 审计脱敏）

**5.8 协议集成面** — `app-server` + `app-server-protocol`：
- JSON-RPC 2.0 over stdio / unix socket / WebSocket
- Thread → Turn → Item 三原语
- `generate-ts` / `generate-json-schema` 生成 TS 类型与 JSON Schema（纳入 CI，协议变更可自动检出）
- **主集成面**：L3 任务编排经此协议驱动 Harness

**5.9 持久化** — Codex 本地持久化（`state`/`thread-store`/`rollout`/`history`/`message-history`/`rollout-trace`）：
- Thread 跨进程重启可恢复、可 fork、可回滚
- **改造点**：事件外送到云端（L3 任务编排消费 → Postgres）；本地状态视为"可丢弃缓存"
- rollout 文件同步到对象存储（L7），Pod 销毁后可下载恢复

**5.10 协作编排** — 多 Agent 协作（`collaboration-mode-templates`/`agent-roles`/`agent-identity`/`agent-graph-store`）：
- 主从式 / 流水线式 / 专家路由式三种协作模式
- 子 Agent 派生（独立 Thread + 受限权限）
- 协作模板复用
- Agent 身份管理与角色定义

---

## 6. L6 模型层 · Model Layer

| # | 模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate） | 是否自建 | 优先级 | 阶段 |
|---|---|---|---|---|---|---|---|---|---|---|
| 6.1 | Model Gateway | L6 | 统一模型入口：路由/计量/降级/限流；所有模型调用经此 | 模型请求 + 令牌 | 模型响应、计量记录 | L4 凭据代理 | 部分（`model-provider`/`model-provider-info`） | 是（网关层） | P0 | M2 |
| 6.2 | 多模型路由 | L6 | 按任务复杂度分档：经济模型（分类/抽取/简单问答）→中档（长任务中段）→强模型（规划/复杂工具编排）；在自有评测集上校准 | 任务分类标签 | 路由到具体模型 | 6.1 Model Gateway | 否 | 是 | P1 | M9 |
| 6.3 | Responses 代理 | L6 | 复用 Codex `responses-api-proxy` 指向自有端点；OpenAI 兼容 API 代理 | 模型 API 请求 | 代理后响应 | 6.1 Model Gateway | 是（`responses-api-proxy`） | 否 | P1 | M2 |
| 6.4 | Prompt Caching | L6 | 相同前缀启用 prompt caching；系统提示与工具描述做版本化提升命中率 | 模型请求前缀 | 缓存命中/未命中 | 6.1 Model Gateway | 否 | 是 | P1 | M9 |
| 6.5 | 故障转移 | L6 | 主模型超时/限流 → 降级备模型 → 仍失败则任务进入"待重试"而非直接失败 | 模型错误/超时 | 降级/重试决策 | 6.1 Model Gateway、6.2 多模型路由 | 否 | 是 | P0 | M9 |
| 6.6 | 私有化部署 | L6 | 数据不出域租户指向自建 vLLM/Ollama 端点；Codex 内置 ollama/lmstudio provider | 私有模型端点配置 | 私有模型响应 | 6.1 Model Gateway | 是（`ollama`/`lmstudio`） | 否 | P2 | M12 |

### L6 模块功能详述

**6.1 Model Gateway** — 统一入口（可用 LiteLLM / 自建）：
- 沙箱出站只到 Model Gateway，令牌按任务绑定、TTL ≤ 任务超时
- 计量：区分 prompt/cached/reasoning/output token
- 降级：主模型不可用时自动切备模型

**6.2 多模型路由** — 经济性优化：
- 简单子任务（分类/抽取/简单问答）→ 经济模型
- 复杂子任务（规划/工具编排）→ 强模型
- 长任务中段 → 中档模型
- 参考阿里百炼 AUTO 路由思路，但**必须在自有评测集上校准**

**6.3 Responses 代理** — `responses-api-proxy` crate：
- Codex 侧可指向自有端点（model_provider 指向 Model Gateway 地址）
- OpenAI 兼容 API 代理，支持 streaming

**6.6 私有化** — 数据不出域：
- Codex 内置 `ollama` provider（本地 Ollama 端点）
- Codex 内置 `lmstudio` provider（本地 LM Studio 端点）
- 高合规租户指向自建 vLLM 集群

---

## 7. L7 存储与治理 · Storage & Governance

| # | 模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate） | 是否自建 | 优先级 | 阶段 |
|---|---|---|---|---|---|---|---|---|---|---|
| 7.1 | Postgres 主库 | L7 | 结构化元数据 + 会话事件存储；RLS（行级安全）兜底租户隔离；`item` 表按 tenant+时间分区；大字段外置对象存储 | SQL 读写 | 查询结果 | L3 全部子系统 | 否 | 是 | P0 | M1, M5 |
| 7.2 | 对象存储 | L7 | rollout/产物/快照存储；按租户前缀 + 按租户 CMK 加密；禁用租户 CMK → 数据不可解密 | 文件上传/下载 | 对象 URL + 引用 | L3 任务编排、L4 Workspace | 否 | 是 | P0 | M2 |
| 7.3 | 向量库 | L7 | 知识库 embedding 存储；pgvector（与 Postgres 同库）或独立 Milvus | embedding 向量 | ANN 检索结果 | L3 知识库/RAG | 否 | 是 | P2 | M11 |
| 7.4 | 审计日志 | L7 | WORM（追加写存储）+ SIEM 投递；应用层账号无 DELETE/UPDATE 权限；独立角色写入 | 审计事件 | 不可篡改审计记录 | 全部层 | 否 | 是 | P1 | M10 |
| 7.5 | OTel 可观测 | L7 | 全链路 trace（用户请求→任务→工具→模型）；指标→Grafana；日志→ClickHouse/ES | trace span、metric | 可视化看板 | 全部层 | 部分（`otel-trace-websocket`/`diagnostics`） | 是（聚合层） | P1 | M4, M9 |
| 7.6 | 评测中心 | L7 | 三类数据集（黄金集/对抗集/生产采样集）+ 五评测平面（完成率/工具正确率/安全拦截率/成本/体验）+ CI 门禁 | 评测请求、变更触发 | 评测报告、门禁决策 | L5 全部、L3 策略中心 | 否 | 是 | P1 | M8 |

### L7 模块功能详述

**7.1 Postgres 主库** — 会话真相存储：

| 数据类型 | 存储方式 | 理由 |
|---|---|---|
| 结构化元数据（租户/用户/权限/任务） | Postgres 主表 | 强一致、事务 |
| 会话事件（Item 流） | Postgres 分区表 + 冷归档到对象存储 | 实时查询与回放；按月分区 |
| `item` 表 | 按 `tenant_id` + 时间分区 | 最大最热的表 |
| `audit_log` / `usage_record` | 只追加（WORM） | 合规要求不可篡改 |

> **RLS 兜底**：即使应用层漏加 `tenant_id` 条件，数据库层强制行级安全策略拒绝跨租户查询。

**7.2 对象存储** — 大对象隔离：
- rollout 文件：每 N 个 Item 或每 T 秒上传一次，结束时必传
- 产物：上传后经敏感信息/恶意文件扫描才对用户可见
- 按租户前缀 + 按租户 CMK 加密：禁用租户 CMK → 数据不可解密

**7.5 OTel 可观测** — 全链路追踪：
- trace 串起"用户请求 → 任务 → 工具 → 模型"
- 复用 Codex `otel-trace-websocket` crate 做 trace 事件 WS 推送
- `diagnostics` crate 提供诊断信息收集
- 指标 → Grafana；日志 → ClickHouse / ES

**7.6 评测中心** — 迭代权保障：
- **三类数据集**：黄金集（主路径）、对抗集（越权/注入/工具失败/数据缺失）、生产采样集（脱敏）
- **五个评测平面**：任务完成率、工具调用正确率、安全拦截率、成本/任务、体验（步骤效率、澄清次数）
- **CI 门禁**：模型/提示/工具/策略任一变更必须跑回归

---

## 8. 安全与合规 · 贯穿层

| # | 模块名 | 所属层 | 功能描述 | 输入 | 输出 | 依赖模块 | 复用 Codex（crate） | 是否自建 | 优先级 | 阶段 |
|---|---|---|---|---|---|---|---|---|---|---|
| 8.1 | 四重隔离取证 | 贯穿 | 逻辑隔离（RLS + tenant_id）+ 运行时隔离（namespace + 节点亲和 + NetworkPolicy）+ 密钥隔离（按租户 CMK）+ 存储隔离（对象存储按租户前缀） | 租户配置 | 隔离验证报告 | L7 Postgres、L4 三层沙箱、L7 对象存储 | 否 | 是 | P0 | M5 |
| 8.2 | KMS 按租户 CMK | 贯穿 | HashiCorp Vault / 云 KMS；按租户客户主密钥；租户禁用 → 数据不可解密 | 加密/解密请求 | 密文/明文 | L7 对象存储、L4 凭据代理 | 否 | 是 | P1 | M5 |
| 8.3 | 网络策略 | 贯穿 | NetworkPolicy 默认拒绝全部出站；沙箱仅放行 Model Gateway + MCP Gateway；高敏租户独立节点池 | 网络策略声明 | 网络隔离 | L4 三层沙箱 | 否 | 是 | P0 | M0, M2 |
| 8.4 | 内容安全 | 贯穿 | PII 检测、提示注入防护（数据面与控制面分离：不可信内容不得改变系统策略）、产物敏感扫描、出站脱敏 | 文本/产物 | 安全决策 | L4 MCP Gateway、L7 对象存储 | 否 | 是 | P1 | M10 |
| 8.5 | 红队演练 | 贯穿 | 季度跨租户越权演练（提示注入、越权访问、供应链攻击）；结果作为准入条件 | 渗透测试计划 | 漏洞报告 + 闭环 | 全部层 | 否 | 是 | P1 | M10+ |

### 安全模块详述

**8.1 四重隔离取证** — 向客户证明隔离的可验证答案：

| 取证层 | 方法 | 验证 |
|---|---|---|
| 逻辑 | 所有查询强制带 `tenant_id` + Postgres RLS 兜底 | SQL 注入不跨租户 |
| 运行时 | namespace + 节点亲和性 + NetworkPolicy | Pod 间不可达 |
| 密钥 | 按租户 CMK，租户禁用后数据不可解密 | 密钥撤销验证 |
| 存储 | 对象存储按租户前缀 + 独立桶策略 | 禁止跨前缀列举 |

**8.5 红队演练** — 每季度一次：
- 跨租户越权：从 A 租户沙箱尝试访问 B 资源
- 提示注入：在用户输入中注入恶意指令
- 供应链攻击：恶意 MCP 工具/依赖投毒
- 结果作为准入条件（非可选项），发现项 100% 闭环

---

## 9. 模块优先级与阶段分布总览

### 9.1 按优先级分布

| 优先级 | 模块数 | 代表模块 |
|---|---:|---|
| P0（MVP 必须） | 26 | Web 门户、API Gateway、身份租户、任务编排、审批中心、策略中心、Agent Loop、Postgres… |
| P1（多租户/治理） | 14 | IDE 插件、CLI、连接器治理、MCP Gateway、凭据代理、KMS、审计日志、评测中心… |
| P2（规模化/生态） | 6 | 知识库/RAG、协作编排、私有化部署、向量库、红队… |

### 9.2 按阶段分布

| 阶段 | 目标 | 交付模块 |
|---|---|---|
| M0（PoC 4 周） | 三大假设验证 | Agent Loop、OS 沙箱、协议集成面、三层沙箱、网络策略、Agent Loop 全链路 |
| M1 | 身份 + 骨架 | 身份租户（单租户）、API Gateway、WS 网关、认证中间件、Web 门户骨架、CLI |
| M2 | 执行闭环 + 会话落库 | Runtime 池调度、Workspace 供给、任务编排、OpenAPI+Webhook、Model Gateway、Responses 代理、对象存储、配额预扣、Agent Loop 落库 |
| M3 | 审批 + 策略 | 审批中心、策略中心、IM Bot |
| M4 | 产物 + 计量 | 配额计费（单租户）、OTel 基础、IDE 插件、Skills/Hooks |
| M5 | 多租户 + 隔离 | 身份租户（多租户）、四重隔离取证、KMS、Postgres RLS |
| M6 | 配额 + 计费 | 配额计费（多租户）、成本归因看板 |
| M7 | 连接器 + 凭据 | MCP Gateway、连接器治理、凭据代理 |
| M8 | 评测体系 | 评测中心 |
| M9 | 可观测 + 稳定 | 多模型路由、Prompt Caching、故障转移、OTel 全链路 |
| M10 | 合规 + 安全 | 审计日志 WORM、内容安全、红队演练 |
| M11 | 性能 + 知识库 | Runtime 池优化（冷启动<5s）、知识库/RAG、向量库、协作编排 |
| M12 | 生态 + 私有化 | 私有化部署、连接器生态 |

### 9.3 按复用类型分布

| 复用类型 | 模块数 | crate 清单 |
|---|---:|---|
| **Codex 黑盒** | 10 | `core`、`tools`、`execpolicy`、`sandboxing`/`linux-sandbox`/`bwrap`/`windows-sandbox-rs`、`skills`/`hooks`、`codex-mcp`/`rmcp-client`、`app-server`/`app-server-protocol`、`state`/`thread-store`/`rollout`/`history`/`message-history`/`rollout-trace`、`collaboration-mode-templates`/`agent-roles`/`agent-identity`/`agent-graph-store` |
| **部分复用** | 10 | IDE 插件（`app-server-protocol`）、CLI（`cli`）、策略中心（`execpolicy`）、三层沙箱（`sandboxing`/`linux-sandbox`/`bwrap`/`windows-sandbox-rs`）、Workspace 供给（`worktree`）、Model Gateway（`model-provider`/`model-provider-info`）、Responses 代理（`responses-api-proxy`）、私有化部署（`ollama`/`lmstudio`）、OTel 可观测（`otel-trace-websocket`/`diagnostics`）、持久化（`state`/`thread-store`/`rollout`/`history`） |
| **自建** | 26 | Web 门户、IM Bot、OpenAPI+Webhook、API Gateway、WS 网关、认证中间件、配额预扣、身份租户、任务编排、审批中心、配额计费、连接器治理、知识库/RAG、Runtime 池调度、MCP Gateway、凭据代理、多模型路由、Prompt Caching、故障转移、Postgres、对象存储、向量库、审计日志、四重隔离取证、KMS、网络策略、内容安全、红队演练、评测中心 |

---

## 10. 模块依赖关系图（文字版）

```
L1 接入层 ──→ L2 网关层 ──→ L3 控制面 ──→ L4 执行面 ──→ L5 Harness ──→ L6 模型层
    │              │              │              │              │              │
    │              │              │              │              │              ▼
    │              │              │              │              │        L7 存储治理
    │              │              │              │              │              │
    └──────────────┴──────────────┴──────────────┴──────────────┴──────────────┘
                                    安全与合规（贯穿）
```

**关键依赖链**：
1. 用户请求 → L1 → L2 API Gateway → L2 认证中间件 → L3 身份租户 → L2 配额预扣 → L3 任务编排
2. L3 任务编排 → L4 Runtime 池调度 → L4 Workspace 供给 → L4 三层沙箱 → L5 app-server → L5 Agent Loop
3. L5 Agent Loop → L6 Model Gateway → L6 模型路由 → 模型响应
4. L5 Agent Loop → L5 工具路由 → L5 ExecPolicy → L4 MCP Gateway → 外部 API
5. L5 app-server 事件流 → L3 任务编排消费 → L7 Postgres（落库）→ L2 WS 网关 → L1 Web/IM（推送）
6. L3 审批中心 ← L5 app-server 审批请求 → L1 IM Bot/Web → 用户决策 → L3 审批中心回写 → L5 app-server 继续

---

## 附录 A · Codex crate 完整清单（105 crate）

本平台复用的 Codex crate 按功能域分组：

| 功能域 | crate | L5 模块 |
|---|---|---|
| Agent 主循环 | `core` | 5.1 Agent Loop、5.5 上下文压缩 |
| 工具路由 | `tools`、`apply-patch`、`file-search`、`file-system`、`exec`、`shell-command`、`shell-escalation` | 5.2 工具路由 |
| 执行策略 | `execpolicy` | 5.3 ExecPolicy |
| OS 沙箱 | `sandboxing`、`linux-sandbox`、`bwrap`、`windows-sandbox-rs`、`windows-sandbox-service`、`mxc-sandbox`、`process-hardening`、`guardian-context` | 5.4 OS 沙箱 |
| Skills/Hooks | `skills`、`hooks`、`plugin`、`core-plugins` | 5.6 Skills/Hooks |
| MCP | `codex-mcp`、`rmcp-client`、`connectors` | 5.7 MCP 客户端 |
| 集成接口 | `app-server`、`app-server-protocol`、`app-server-transport`、`app-server-client`、`app-server-test-client`、`app-server-daemon`、`app-server-protocol-noop-macros`、`exec-server`、`exec-server-protocol`、`codex-client`、`codex-api`、`core-api`、`protocol`、`stdio-to-uds` | 5.8 协议集成面 |
| 持久化 | `state`、`thread-store`、`rollout`、`rollout-trace`、`history`、`message-history` | 5.9 持久化 |
| 协作编排 | `collaboration-mode-templates`、`agent-roles`、`agent-identity`、`agent-graph-store` | 5.10 协作编排 |
| 模型抽象 | `model-provider-info`、`model-provider`、`responses-api-proxy`、`ollama`、`lmstudio`、`chatgpt`、`models-manager` | L6 模型层 |
| 配置 | `config`、`config-schema`、`codex-home`、`install-context`、`features`、`toggles` | 配置基础设施 |
| 网络/安全 | `network-proxy`、`workload-identity`、`aws-auth`、`secrets`、`http-client` | 安全基础设施 |
| 工具链 | `git-utils`、`worktree`、`terminal-detection`、`arg0`、`ansi-escape`、`file-watcher`、`context-fragments`、`feedback`、`attachment-store` | 工具基础设施 |
| 可观测 | `otel-trace-websocket`、`diagnostics`、`analytics`、`build-info`、`response-debug-context` | L7 OTel |
| CLI/TUI | `cli`、`tui` | L1 CLI |
| 其他 | `async-utils`、`websocket-client`、`cloud-tasks`、`cloud-tasks-mock-client`、`cloud-config`、`external-agent-migration`、`code-mode`、`code-mode-runtime`、`code-mode-host`、`code-mode-protocol`、`voice-host`、`realtime-webrtc`、`v8-poc`、`login`、`prompts`、`uds`、`backend-client`、`codex-backend-openapi-models`、`codex-experimental-api-macros`、`test-binary-support`、`thread-manager-sample` | 辅助/实验 |

---

## 附录 B · 与路线图对照

本清单与路线图 §1.1（可复用资产清单）和 §3（分层详细设计）完全对齐：

- §1.1 可复用资产 → 本文档 L5 全部 + L6 部分 + L7 部分
- §1.2 缺口清单 → 本文档 L1–L4 + L3 全部 + L7 自建部分
- §3.1–§3.7 分层设计 → 本文档 §1–§7 逐层对应
- §7.1–§7.6 实施路线图 → 本文档 §9.2 阶段分布

> 参见路线图：`~/Nexus/docs/architecture/Nexus 基于CodexHarness的企业级Agent平台_系统设计与实施路线图.md`
