# Nexus 企业级 AI Agent 平台 — 系统架构设计方案

> 产物编号：任务二-1 · 完整系统架构设计方案
> 基座：`~/Nexus`（基于 OpenAI Codex Harness，codex-rs 106 crate Rust 工作区）
> 日期：2026-09-06 · 配套图：`system-architecture.svg` / `.png` · 交互报告：`system-architecture-report.html`

---

## 0. 架构判断（结论先行）

> **一句话**：Codex Harness 给的是「一台单用户、本地、可打断的 Agent 引擎」；Nexus 要建的是「让这台引擎变成多租户、可计量、可审计、崩溃不丢会话的企业服务」。前者是执行内核（L5），后者是控制平面（L1–L4 + L6–L7），两者必须严格分离，且不改内核。

**五个决定成败的设计判断**：

| # | 判断 | 理由 |
|---|---|---|
| 1 | 必须走 `app-server`（JSON-RPC）集成，而非 `codex exec` 或 SDK | 只有 app-server 提供长生命周期 Thread、双向事件流、`turn/interrupt`、协议级审批回写、`thread/resume`/`fork`/`rollback`/`revert`。企业会话持久化、跨小时审批、断线重连全靠这些 |
| 2 | 会话真相在云端 DB，Harness 只持有"可重建的执行态" | Codex 自身持久化是本地 SQLite（`state`/`thread-store`）+ 本地 rollout 文件；控制面消费 app-server 事件流写 Postgres，rollout 同步对象存储，Pod 死不丢会话 |
| 3 | 沙箱内零长期密钥 | 沙箱 Pod 拿短期、按任务绑定、可撤销令牌；模型调用经自建 Model Gateway，MCP 凭据由 MCP Gateway 侧车注入，绝不进 `config.toml` |
| 4 | 权限分三层，execpolicy 是天然政策下发载体 | 平台身份 → 工作区/环境 → 工具与命令；Codex 的 `execpolicy`（Starlark 规则引擎）+ `approval_policy` + `sandbox.mode` 承接第三层，按租户/角色动态生成下发 |
| 5 | 不改 Harness 内核 | 上游日均 10+ commit、106 crate；原则是"配置驱动 + 事件桥接 + 外壳包装"，确需 patch 放 `patches/` |

---

## 1. 总体架构：八层分层模型

Nexus 采用「控制平面 / 执行平面物理分离」的八层架构。L1–L3 与 L6–L7 自建，L4 是自建外壳，L5 复用 Codex Harness（黑盒）。

| 层 | 职责 | 关键模块 | 是否复用 Codex |
|---|---|---|---|
| L1 接入层 | 多渠道统一入口 | Web 门户、IM Bot（飞书/钉钉/企微/Slack）、IDE 插件、OpenAPI、Webhook、CLI | 否（可复用 IDE 扩展协议） |
| L2 网关层 | 南北向流量、鉴权、实时推送 | API Gateway、WebSocket 网关、认证中间件（OIDC/SAML/SCIM）、限流 | 否 |
| L3 控制平面 | **平台的大脑与账本** | 身份租户、任务编排（Temporal Workflow）、审批中心、策略中心、配额计费、连接器治理、知识库/RAG | 否（自建核心） |
| L4 执行平面 | **Harness 托管外壳** | Runtime 池调度、沙箱 Pod、MCP Gateway、Workspace 供给、凭据代理 | 薄壳 + 复用 |
| L5 Harness | Agent 执行内核 | Agent Loop（run_turn 七阶段）、Tool Router、ExecPolicy、OS 沙箱、上下文压缩、Skills/Hooks | **是（黑盒，不改）** |
| L6 模型层 | 模型访问与计量 | Model Gateway、多模型路由、Responses 代理、Token 计量 | 部分（`model-provider`、`responses-api-proxy`） |
| L7 存储与治理 | 持久化与可观测 | Postgres（RLS+分区）、对象存储、向量库（pgvector）、审计日志（WORM）、OTel、评测中心 | 否 |
| 贯穿 | 安全与合规 | 租户隔离（四重取证）、KMS（按租户 CMK）、网络策略、审计留存、内容安全、红队 | 否 |

### 1.1 控制平面 / 执行平面的物理切分（最关键的一条线）

```
┌─────────────── 控制平面（长期有状态 · 多租户 · 强一致）───────────────┐
│  API/WS 网关 │ 任务编排器 │ 审批中心 │ 策略中心 │ 计量 │ 知识库 │       │
│  Postgres（tenant/user/thread/turn/item/approval/usage/audit）        │
│  对象存储（artifacts/rollouts/snapshots，按租户前缀+CMK）             │
└───────────────────────────┬──────────────────────────────────────────┘
                            │ ① 调度指令（K8s Job/Queue）
                            │ ② app-server JSON-RPC（exec/port-forward/unix socket）
                            │ ③ 事件流回传
┌───────────────────────────▼──────────────────────────────────────────┐
│  执行平面（一次性 · 单租户单任务 · 无状态 · 可销毁）                    │
│  ┌──────────── Sandbox Pod（NetworkPolicy: 默认全禁）────────────────┐ │
│  │  codex app-server  ←── config.toml + execpolicy（运行时注入）      │ │
│  │  Workspace（git worktree / PVC 快照）                              │ │
│  │  MCP Gateway sidecar（凭据注入 · 工具白名单 · 出站代理）           │ │
│  │  出站：仅 Model Gateway 与 MCP Gateway 两个白名单地址              │ │
│  └───────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────┘
```

**为什么必须物理切分**：
- 沙箱一定跑不受信代码（依赖安装、用户脚本、MCP 子进程 stdio）——放进控制面等于把整个平台暴露
- 沙箱生命周期是分钟到小时级，控制面是年计；混在一起发布/灰度/灾备全复杂化
- 只有物理隔离，才能对"租户 A 的 Agent 拿到租户 B 的数据"给出可验证答案（网络策略 + 独立密钥 + 独立存储前缀，三重取证）

### 1.2 六条设计原则

1. **控制/执行分离**：控制面长期有状态、多租户、强一致；执行面无状态、一次性、可随时销毁；只通过 app-server 协议与对象存储通信
2. **Harness 不持有企业真相**：租户/权限/凭据/计费/审计全在控制面；沙箱只有"本次任务的短期令牌 + 最小工作集"
3. **事件即事实**：所有对用户可见状态都来自 app-server 事件流；前端不直连 Harness，避免绕过审计
4. **默认最小权限**：新租户默认只读沙箱 + 全量审批；权限只能显式提升，不能默认放宽
5. **可重建优于高可用**：不为沙箱 Pod 做复杂 HA，而是"任何 Pod 死亡都能从云端状态重建"
6. **配置即政策**：租户差异表达为下发的 `config.toml` + execpolicy 规则集，而非代码分支

---

## 2. 控制平面（L1–L3）详细架构

### 2.1 L1 接入层

| 入口 | 形态 | 要点 |
|---|---|---|
| Web 门户 | React + WebSocket | 会话列表、任务时间线（Item 流）、审批抽屉、产物预览、Diff 查看 |
| IM Bot | 飞书/钉钉/企微/Slack | 审批推送最佳渠道；卡片消息承载"批准/拒绝/修改后批准" |
| IDE 插件 | VS Code/JetBrains | 复用 Codex app-server 协议，把远端 Thread 映射到本地 |
| OpenAPI | REST + Webhook | Agent 能力被业务系统调用；Webhook 任务完成回调 |
| CLI | 复用 `codex` + 自定义登录 | 开发者体验入口 |

**统一约束：所有入口都不直连 Harness。** 必须经 L2 网关，否则审计链路断裂。

### 2.2 L2 网关层

- **API Gateway**：REST 路由、请求校验、幂等（`Idempotency-Key`）、限流（租户级 + 用户级 + IP 级）
- **WebSocket 网关**：会话事件推送；订阅关系由"用户对 Thread 的读权限"驱动，权限变更立即断连
- **认证中间件**：OIDC/SAML 对接企业 IdP；SCIM 同步组织架构；服务账号 mTLS
- **配额预扣**：网关层粗粒度拦截，细粒度结算在 L3

### 2.3 L3 控制平面（自建核心，六大子系统）

#### 2.3.1 身份与租户
```
Tenant（租户）
  └── OrgUnit（组织单元，可多级，映射部门树）
        └── User / ServiceAccount
              └── Role（owner/admin/developer/auditor/viewer）
  └── Workspace（工作环境：绑定仓库、数据集、连接器、知识库范围）
        └── Membership（成员 × 角色 × 资源作用域）
```
- **授权模型**：RBAC 打底 + ABAC 兜底（属性：`tenant_id`/`org_path`/`env`/`data_classification`/`risk_level`/`time_window`）
- **三个必须区分的身份**：用户身份、Agent 身份（服务账号，权限是用户的子集且需显式授予）、连接器身份（MCP/OAuth 委托令牌，按租户隔离）
- **权限继承规则**：`Agent 可用权限 = 用户权限 ∩ 工作区权限 ∩ Agent 角色上限 ∩ 策略中心允许`，四者取交集，任一为空即拒绝

#### 2.3.2 任务编排与调度
- 用持久化工作流引擎（Temporal/Cadence），把"申请 Pod → 建连 → 下发任务 → 消费事件 → 处理审批 → 收尾结算"建模为可恢复 Workflow
- **两层循环**：外层（平台）Workflow 管资源与账本（长周期）；内层（Harness）`run_turn` 管模型与工具（短周期）；两者通过事件流关联，不混为一个循环
- **调度策略**：租户权重队列 + 优先级 + 并发上限；Prod 与实验任务分池

#### 2.3.3 审批中心（HITL）
见 §4 专题。核心理念：审批是控制面一等公民资源，先落库后回写，跨小时跨设备可审计。

#### 2.3.4 策略中心
- 策略对象：`{tenant, org_path, role, workspace, tool, action, risk_level, data_classification}`
- 决策结果：`allow` / `deny` / `require_approval` / `require_dual_approval`（四眼）/ `allow_with_audit_only`
- 求值时机：任务准入一次 + **每次高危工具调用前一次**（不把准入当永久通行证）
- 漂移防护：策略快照写入任务上下文，运行中策略变更"不溯及已批准、对新动作用新策略"

#### 2.3.5 配额与计费
- **四维计量**：token（prompt/cached/reasoning/output）、工具调用次数、沙箱运行时长、存储与出站流量
- **归因维度**：`tenant → org_unit → user → thread → turn → model`，能回答"某部门上周在模型上花了多少钱"
- **预算控制**：软阈值告警 → 降档经济模型 → 硬阈值熔断（任务优雅暂停保存 rollout，而非直接杀）

#### 2.3.6 连接器治理
- 分级：官方认证 / 企业私有 / 社区（默认禁用，需管理员开启）
- 元数据：owner、权限粒度、幂等性、副作用等级、限流、版本、评测覆盖度、最后健康检查
- 质量分：可用性、P95 延迟、错误率、权限最小化 → 低于阈值自动降级/下线
- 安全：MCP stdio 型是重大风险面（配置即代码执行），强制 `.codex/config.toml` 只在受信任工作区生效、`enabled_tools` 白名单优先、破坏性注解工具恒定需审批

#### 2.3.7 知识库 / RAG
- **ACL 随索引写入**：每 chunk 携带 `tenant_id + acl_tags + permission_version`；检索先过滤后召回
- 流程：metadata/ACL 过滤 → 稠密+稀疏混合召回 → rerank → 只回填支撑片段（附 chunk_id 与权限版本）
- 衔接：知识检索做成 MCP 工具或自定义 Tool，权限在 Gateway 侧强制，不依赖模型自觉遵守

---

## 3. 执行平面（L4）详细架构

### 3.1 Runtime 池调度

| 项 | 设计 |
|---|---|
| 任务粒度 | 一个 Turn = 一个 Pod（长任务可复用，有最大时长，超期强制结算并 resume 到新 Pod） |
| 镜像 | 预装 Codex 二进制 + 语言工具链基础镜像；按语言/场景分多个镜像 |
| 冷启动 | 预热池（warm pool）+ Workspace 快照（PVC snapshot）；目标 < 5s |
| 并发 | 租户级并发上限 + 全局上限 + 队列等待位；超限排队并实时告知位置 |
| 销毁 | 任务结束或空闲超时（15–30 min）销毁；销毁前必做：上传 rollout、结算、审计 |

### 3.2 三层沙箱（缺一不可）

Codex 自带**第二层**（命令级 OS 沙箱），Nexus 补第一层和第三层：

| 层 | 技术 | 防什么 |
|---|---|---|
| ① 容器/微虚拟机层 | K8s Pod + 受限 Seccomp/AppArmor；高敏租户用 Kata/Firecracker | 租户间横向逃逸、内核攻击面 |
| ② 命令级 OS 沙箱（Codex 自带） | macOS Seatbelt；Linux Landlock+seccomp+bubblewrap；Windows restricted token | Agent 在本工作区乱跑命令、读 SSH key、外传数据 |
| ③ 网络层 | NetworkPolicy 默认拒绝全部出站，仅放行 Model Gateway 与 MCP Gateway | 数据外泄、C2 回连、依赖投毒外联 |

> Linux 下 Codex 沙箱在容器中可能因宿主不支持 Landlock/seccomp 而失效，因此**容器层不能省**，且需启动自检验证沙箱可用性（自检失败禁止调度生产任务）。

**sandbox 与 approval 正交配置**（沿用 Codex 哲学）：

| 场景 | `sandbox.mode` | `approval_policy` | 说明 |
|---|---|---|---|
| 新租户默认 | `read-only` | `untrusted` | 只读+逐条确认，最保守 |
| 成熟场景 | `workspace-write` | `on-failure` | 工作区内可写，失败才问 |
| 高危场景 | `read-only` | `always` | 强管控，任何动作都要人批 |
| 禁止场景 | — | — | `danger-full-access` 不向终端用户开放 |

### 3.3 MCP Gateway（凭据注入关键）

```
Codex (MCP Client) ─stdio/http→ MCP Gateway Sidecar（同 Pod，独立凭据域）
   ├─ 从控制面换取短期凭据（TTL ≤ 任务时长）
   ├─ 强制 enabled_tools 白名单
   ├─ 出站请求审计 + 敏感字段脱敏
   └─ 转发到真实 MCP Server / 企业 API
```
要点：`config.toml` 里**不出现任何真实密钥**，只出现指向 Gateway 的本地地址与任务令牌。

### 3.4 凭据代理
- 签发短期令牌（JWT，audience 限定 Model Gateway / MCP Gateway，TTL = 任务超时 + 缓冲）
- 令牌绑定 `tenant_id + thread_id + turn_id + 权限快照哈希`，任一不符即拒绝
- 支持即时吊销（用户点"停止任务" → 控制面吊销 → Gateway 拒绝后续调用）

---

## 4. 六个硬骨头专题

### 4.1 会话云端持久化：三写一致与 resume

**方案：以控制面写入为准，Harness 本地状态视为"可丢弃缓存"。**

```
app-server 事件流
   ├─→ 控制面消费者（at-least-once）
   │      ├─ 幂等写入（event_id 唯一键：thread_id+turn_id+item_seq）
   │      ├─ 顺序补齐（seq 缺口主动拉 rollout 补齐）
   │      └─ 写 Postgres：thread/turn/item
   ├─→ 对象存储：rollout 文件（每 N 个 Item 或每 T 秒上传，结束必传）
   └─→ WebSocket：推前端（仅展示，不作为真相）
```
| 点 | 做法 |
|---|---|
| 幂等 | `thread_id + turn_id + item_seq` 唯一键，重复事件直接丢弃 |
| 顺序 | 维护期望 seq；缺口 → 暂停推送、拉 rollout 补齐、再继续 |
| 不阻塞 Harness | 写库失败不反压 app-server；降级本地队列 + 告警 |
| 大字段 | shell 输出/diff 超 64KB 只存对象存储引用 + 摘要 |
| resume | 新 Pod → 下载 rollout → `thread/resume`；新事件 seq 从云端最大值继续 |
| fork | `thread/fork` 产生新 thread_id，复制 item 元数据（不复制大字段实体） |

### 4.2 权限模型：三层授权 + 政策下发

```
第一层 平台身份层  用户/服务账号/Agent 身份，SSO+SCIM+RBAC/ABAC
           ↓（决定"能不能进这个工作区"）
第二层 工作区层    仓库、数据集、连接器、知识库范围；环境标签 prod/staging
           ↓（决定"这个环境里能碰什么"）
第三层 工具与命令层 execpolicy 规则 + approval_policy + MCP 白名单 + sandbox.mode
           ↓（决定"具体能执行什么动作"）
       执行，并全量记录
```
政策下发（复用 Codex 最巧妙处）：
```
控制面按 (tenant, role, workspace, risk_level) 生成：
  ├── config.toml         # sandbox.mode / approval_policy / mcp_servers / model_provider
  ├── execpolicy.rules    # Starlark allow/deny 规则集
  ├── enabled_tools       # 每个 MCP server 的工具白名单
  └── AGENTS.md           # 项目级自然语言规范（兜底软约束）
        ↓ 运行时注入 Pod，任务结束即焚
```

### 4.3 审批流：跨进程 HITL 桥接（最复杂）

```
① app-server 发出审批请求事件
   ↓
② 适配层解析 → 控制面创建 ApprovalTicket（pending）
   ├─ 内容：thread_id/turn_id/item_seq/工具名/参数(脱敏)/diff预览/风险/影响
   ├─ 策略：谁可批（单人/双人/角色）、超时动作（默认拒绝）
   └─ 快照：审批时上下文（事后可看"批的是什么"）
   ↓
③ 推送：Web 抽屉 + IM 卡片 + 邮件（按风险选渠道）
   ↓
④ 用户决策（批准/拒绝/修改参数后批准/转交）
   ↓
⑤ 决策先落库（decided）再回写 app-server
   ↓
⑥ app-server 继续/中止；结果回事件流闭环
```
六个边界情况：Pod 等待时崩溃（DB 存审批状态，重建后用 item_seq 去重，已决策直接重放）、审批超时（默认拒绝）、审批期间权限撤销（决策时重校验）、修改参数后批准（重新走策略求值）、批量相似请求（作用域有限定：仅该目录/仅该工具/≤1h）、审计（请求快照+决策人+时间+理由不可篡改）。

### 4.4 多租户隔离与运行时池

三档部署矩阵：

| 档位 | 隔离方式 | 适用 | 成本 |
|---|---|---|---|
| 共享池 | 逻辑隔离（namespace + 行级 tenant_id + 网络策略） | 中小客户、非敏感 | 低 |
| 专属池 | 独立节点池 + 独立命名空间 + 独立密钥 | 大客户、合规要求 | 中 |
| 私有化 | 独立 VPC / 独立集群 / 数据不出域 | 金融、政务、国企 | 高 |

**四重取证**（缺一不可）：逻辑（Postgres RLS 兜底）、运行时（namespace+节点亲和+NetworkPolicy）、密钥（按租户 CMK，禁用后不可解密）、存储（对象存储按租户前缀+独立桶策略）。季度跨租户越权红队演练。

### 4.5 沙箱内零长期密钥

| 密钥类型 | 处理 |
|---|---|
| 模型 API Key | 绝不进沙箱，只到 Model Gateway，任务令牌换调用 |
| MCP/企业 API 凭据 | MCP Gateway 侧车持有，短期委托令牌向控制面换 |
| Git 凭据 | 凭据代理 + 只读/限定分支，禁推保护分支 |
| 云厂商 AK/SK | IRSA/Workload Identity 换短期角色凭证，不落盘 |
| 用户 OAuth 令牌 | 加密存控制面密钥库，按需换短期 access token，结束吊销 |

沙箱启动自检清单：Landlock/seccomp/Seatbelt 可用性、出站仅两白名单地址、镜像无长期密钥、只读 rootfs+非 root、资源限额生效。

### 4.6 成本控制
模型分档路由（简单子任务用经济模型）、Prompt Caching（系统提示/工具描述版本化）、复用 Harness 压缩（保留推理轨迹+自动压缩）、步数与预算上限、沙箱空闲超时回收、缓存任务结果、归因看板（异常消耗实时告警）。

---

## 5. L5 Harness 适配层（唯一贴着 Codex 写的代码）

薄适配，职责严格限定四件事，**明确不做**：不改 `run_turn`、不改工具路由、不改压缩算法、不改 execpolicy 求值器。

1. **协议桥接**：app-server JSON-RPC 事件流 → 内部事件总线（Kafka/NATS/Postgres LISTEN/NOTIFY）；用 `codex app-server generate-json-schema`/`generate-ts` 生成类型纳入 CI，协议变更自动检出
2. **配置生成**：按 `tenant+role+workspace+risk_level` 生成 `config.toml`、execpolicy 规则集、MCP 声明、Skills 清单，运行时注入 Pod
3. **命令包装**：对外暴露 `start`/`resume`/`interrupt`/`approve`/`fork`/`archive` 六动作，映射底层协议
4. **健康检查与回收**：探测 app-server 存活、处理僵尸进程、Pod 退出前上传 rollout

---

## 6. L6 模型层 / L7 存储与治理

### 6.1 L6 模型层
- 统一入口 Model Gateway（LiteLLM/自建；Codex 侧复用 `responses-api-proxy` 与 `model-provider` 抽象指向自有端点）
- 路由策略：分类/抽取走经济模型，规划/复杂工具编排走强模型，长任务中段中档
- 缓存：相同前缀 prompt caching；系统提示与工具描述版本化提升命中率
- 故障转移：主超时/限流 → 备模型 → 仍失败任务进入"待重试"
- 私有化：数据不出域租户指向自建 vLLM/Ollama（Codex 内置 ollama/lmstudio provider）

### 6.2 L7 存储分层

| 数据类型 | 存储 | 理由 |
|---|---|---|
| 结构化元数据 | Postgres | 强一致、事务、RLS |
| 会话事件（Item 流） | Postgres 分区表 + 冷归档对象存储 | 实时查询与回放；按月分区 |
| rollout/快照 | 对象存储（按租户前缀+CMK） | 大对象、低频 |
| 产物 | 对象存储 + 病毒/敏感扫描 | 分享与版本 |
| 知识库向量 | pgvector / Milvus | 视既有栈 |
| 审计日志 | WORM 追加写 + SIEM 投递 | 合规不可篡改 |
| 追踪与指标 | OTel → ClickHouse/ES + Grafana | 可观测 |

---

## 7. 一次任务的完整生命周期（13 步）

| 步 | 动作 | 关键设计点 |
|---|---|---|
| ① | 客户端提交任务 | 幂等键 `Idempotency-Key`，避免重复计费 |
| ② | 鉴权+策略求值+配额预扣 | 先扣后跑防超卖；策略快照入任务上下文防漂移 |
| ③ | 调度沙箱 Pod | 注入 config.toml、execpolicy、短期令牌；Workspace 由快照/克隆生成 |
| ④ | 启动 app-server，下发 rollout 恢复 | 首次 `thread/start`；恢复先下载 rollout 再 `thread/resume` |
| ⑤ | 模型采样 | 沙箱出站只到 Model Gateway，令牌按任务绑定 |
| ⑥ | app-server 回吐事件流 | `turn/started`、`item/*`、text delta、工具进度、审批请求 |
| ⑦ | 控制面消费事件→写云端 Postgres+WS 推前端 | 会话持久化主通道；先落库后推，可回放 |
| ⑧ | 审批请求→落 ApprovalTicket→推用户 | 最复杂桥接 |
| ⑨ | 用户决策回写→app-server 继续 | 决策先落库再回写，宕机可重放 |
| ⑩ | 工具执行 | shell 走 execpolicy+OS 沙箱；MCP 走 Gateway 注入凭据；都记 Item |
| ⑪ | 上下文将满→auto compact | 复用 Harness 压缩；压缩前完整上下文已落云端可回溯 |
| ⑫ | `turn/completed`→产物与 rollout 上传对象存储 | 产物扫描（敏感/恶意）后才对用户可见 |
| ⑬ | 用量结算+审计→Pod 销毁 | 归还配额、写 usage、审计留痕；会话留云端可随时 resume |

---

## 8. 技术选型

| 领域 | 方案 | 选型理由 |
|---|---|---|
| Agent 内核 | Codex Harness（app-server） | Apache-2.0、生产验证、沙箱与策略引擎现成 |
| 集成方式 | app-server JSON-RPC（stdio/unix socket） | 唯一支持长会话+事件流+中断+审批回写 |
| 编排引擎 | Temporal/Cadence | 审批跨小时，需可持久化可恢复 Workflow |
| 事件总线 | Kafka/NATS（MVP 用 Postgres LISTEN/NOTIFY） | 事件流吞吐与重放 |
| 主库 | PostgreSQL（RLS+分区） | RLS 是租户隔离最后兜底 |
| 对象存储 | S3/MinIO（按租户前缀+CMK） | rollout/产物按租户加密隔离 |
| 沙箱（容器层） | K8s Pod+Seccomp/AppArmor；高敏 Kata/Firecracker | 与 Codex 命令级沙箱互补 |
| 命令级沙箱 | Codex 自带（Seatbelt/Landlock+bwrap/Windows token） | 直接用，不重写 |
| 命令策略 | Codex execpolicy（Starlark） | 规则语言与求值器已与会话层解耦 |
| 工具协议 | MCP（经 Gateway 代理） | 生态标准，Codex 原生支持 |
| 模型网关 | LiteLLM/自建 + Codex responses-api-proxy | 统一计量、路由、降级 |
| 认证 | Keycloak/企业 IdP（OIDC+SAML+SCIM） | 企业 SSO 刚需 |
| 密钥管理 | HashiCorp Vault/云 KMS | 按租户 CMK 是隔离取证关键 |
| 可观测 | OpenTelemetry + ClickHouse + Grafana | trace 串"用户→任务→工具→模型" |
| 评测 | 自建评测中心 + LLM-as-judge | 每次模型/提示/工具变更回归 |
| 前端 | React + WS | 实时事件流渲染 |

---

## 9. 实施路线图（四阶段）

| 阶段 | 周期 | 目标 | 验收 |
|---|---:|---|---|
| 0. PoC | 4 周 | 验证"Codex 远端 Pod 跑、事件落库、审批桥接" | 一条任务端到端跑通，会话可新 Pod 恢复 |
| 1. 单租户 MVP | M1–M4 | 单租户可用：权限、会话云端化、审批、产物 | 10 种子用户日常用，P0 完成率 ≥ 70% |
| 2. 多租户与隔离 | M5–M7 | 租户体系、配额计费、隔离取证、连接器治理 | 3+ 租户共享，跨租户越权演练 0 通过 |
| 3. 可靠性与治理 | M8–M10 | 评测体系、可观测、成本优化、合规 | 任务成功率 ≥ 85%，成本可归因 |
| 4. 规模化与生态 | M11–M12+ | 高并发、连接器市场、知识库、私有化 | 100+ 并发稳定，私有化交付跑通 |

---

## 10. 风险与反模式（八条高危）

| 反模式 | 后果 | 正解 |
|---|---|---|
| 前端直连 Harness | 绕过审计配额 | 所有流量经网关 |
| API Key 打进沙箱镜像 | 逃逸=全量泄漏 | 短期令牌+Gateway |
| 改 Harness 内核实现企业特性 | 数周无法跟随上游 | 配置驱动+事件桥接 |
| 用 LLM 判权限 | 提示注入即提权 | 权限判定用代码策略 |
| 只做逻辑隔离 | 一个 SQL 注入跨租户 | 四重隔离 |
| 审批状态存 Harness | Pod 崩审批丢 | 控制面一等资源先落库 |
| 会话只存本地 rollout | 机器故障全丢 | 事件流落云端为真相 |
| 先做通用助手再找场景 | 无切换价值 | 绑定高价值场景切入 |

---

## 11. 配套产物索引

| 产物 | 文件 |
|---|---|
| 架构图（SVG 源） | `system-architecture.svg` |
| 架构图（PNG 位图） | `system-architecture.png` |
| 交互报告（HTML） | `system-architecture-report.html` |
| 生成脚本 | `_gen_svg.py` |

本方案与 `../Nexus 基于CodexHarness的企业级Agent平台_系统设计与实施路线图.md` 对齐，为其工程化落地版。
