# Nexus 源码深度调研报告：Codex Harness 架构解构与平台化启示

> 基于 `~/Nexus/codex-rs` 仓库 111+ Rust crate 的全量源码调研，面向 Nexus 企业级 Agent 平台落地。

---

## §0 执行摘要

Codex Harness（`codex-rs/`）是一个由 111+ Rust crate 组成的 Cargo workspace（Rust 2024 edition + Tokio 异步运行时），叠加 Bazel 构建覆盖层。其架构本质是「单用户、本地、可打断的 Agent 引擎」：核心引擎层 `codex-core` 驱动 Turn 七阶段循环（准入→快照→采样→工具分发→结果回写→压缩→完成/中断），通过 `app-server` JSON-RPC 协议层对外暴露 Thread/Turn/Item 三原语，底层以 SQLite + JSONL rollout 实现本地持久化，以 execpolicy（Starlark 规则引擎）+ 三平台 OS 沙箱实现命令级隔离。核心结论：**Harness 提供了完备的 Agent 执行内核与协议接口，但缺失多租户、计量、审计、云端会话、跨进程审批等企业能力**。Nexus 的路径是控制/执行分离——以 app-server 为唯一集成面，消费事件流写云端 Postgres，下发 config.toml + execpolicy 规则集表达租户差异，不改内核，四重隔离保障安全。

---

## §1 仓库总览

### 1.1 仓库结构

```
~/Nexus/
├── codex-rs/          # ← 实际 Harness 内核，编辑在此
│   ├── Cargo.toml     # workspace root, edition = "2024"
│   ├── core/          # codex-core 引擎主体
│   ├── app-server/    # JSON-RPC 协议层
│   ├── state/         # SQLite 元数据镜像
│   ├── execpolicy/    # Starlark 规则引擎
│   ├── sandboxing/    # 跨平台沙箱编排
│   ├── ...            # 105 个 workspace member crate
│   └── ext/           # 14 个扩展 crate（extension-api/skills/mcp/goal/...）
├── codex-main/        # ← 上游快照参考，禁止编辑
├── docs/architecture/ # Nexus 架构文档
└── CLAUDE.md          # 项目说明 + crate 清单
```

- **Cargo workspace**：105 个 workspace member crate + 14 个 `ext/` 子目录扩展 crate = **111+ crate 总量**。
- **Rust 2024 edition**：workspace 级统一追踪（`Cargo.toml` 注释 `Track the edition for all workspace crates in one place`）。
- **Tokio 异步运行时**：全仓库统一使用。
- **Bazel 覆盖层**：Bazel 9.0.0（`.bazelversion`），每个 crate 有 `BUILD.bazel`，CI 校验 `MODULE.bazel.lock` 漂移。Bazel 不自动暴露源文件给 `include_str!`/`include_bytes!`，需手动更新 `compile_data`。

### 1.2 crate 拓扑概览

| 层 | crate 群 | 数量 | 定位 |
|---|---|---|---|
| 核心引擎 | core, core-api, core-plugins, protocol, context-fragments, prompts | 6 | Turn 循环 + 上下文管理 + 协议定义 |
| app-server 协议 | app-server, app-server-protocol, app-server-transport, app-server-daemon, app-server-client, app-server-test-client, app-server-protocol-noop-macros | 7 | JSON-RPC 双向通信 + 传输 + 托管 |
| 持久化与状态 | state, thread-store, rollout, rollout-trace, history, message-history, attachment-store | 7 | SQLite 镜像 + JSONL rollout + 附件 |
| 沙箱安全 | execpolicy, sandboxing, linux-sandbox, bwrap, windows-sandbox-rs, windows-sandbox-service, process-hardening, mxc-sandbox, secrets, keyring-store, workload-identity, guardian-context | 12 | 策略引擎 + OS 沙箱 + 凭证 + 守护 |
| 模型与云控 | model-provider, model-provider-info, responses-api-proxy, ollama, lmstudio, chatgpt, login, aws-auth, backend-client, cloud-config, cloud-tasks, cloud-tasks-client, cloud-tasks-mock-client, codex-backend-openapi-models, codex-api, codex-home, models-manager | 17 | 模型抽象 + 登录认证 + 云任务/配置 |
| MCP/技能/工具 | codex-mcp, rmcp-client, skills, hooks, tools, file-search, file-system, file-watcher, apply-patch, git-utils, collaboration-mode-templates, agent-roles, agent-identity, agent-graph-store, connectors, ext/extension-api + 14 ext/* | 30+ | MCP + 技能 + 钩子 + 文件工具 + 协作 |
| exec/TUI/可观测 | exec, exec-server, exec-server-protocol, shell-command, shell-escalation, cli, tui, codex-client, otel, otel-trace-websocket, analytics, diagnostics, feedback, realtime-webrtc, voice-host, code-mode*（4）, websocket-client, uds, stdio-to-uds, terminal-detection, thread-manager-sample | 25+ | 非交互执行 + 终端 UI + 可观测 + 语音 |

---

## §2 核心引擎层 codex-core

### 2.1 crate 职责与关键源文件

| Crate | 职责 | 关键源文件 |
|---|---|---|
| **codex-core** (`core`) | 引擎主体：会话循环、上下文管理、工具分发、压缩、MCP/插件编排、rollout 持久化 | `core/src/lib.rs:1`，子模块 `agent/` `context/` `context_manager/` `session/` `state/` `tools/` `tasks/` `guardian/` `unified_exec/` `mcp_tool_call/` `exec_policy/` `plugins/` |
| **codex-core-api** (`core-api`) | 对外门面 crate，`pub use` 再导出 `ThreadManager`/`CodexThread`/`TurnInput`/`Config` 等 | `core-api/src/lib.rs:23-50`，`79-80` |
| **codex-core-plugins** (`core-plugins`) | 插件市场与加载：marketplace、安装/升级、清单解析、远程 bundle | `core-plugins/src/manager.rs` `loader.rs` `marketplace.rs` `remote.rs` `store.rs` |
| **codex-protocol** (`protocol`) | 纯数据协议层：`ThreadId`/`SessionId`/`ResponseItemId`、`TurnItem`、`ResponseItem`、`EventMsg`、`TurnInput` | `protocol/src/lib.rs:1`，无业务逻辑，被几乎所有 crate 依赖 |
| **codex-context-fragments** | 上下文片段抽象：`RenderedFragment`、`AnnotatedContent`、`ContextualUserFragment` trait | `context-fragments/src/lib.rs:1`，`fragment.rs:6` |
| **codex-prompts** | prompt 模板与审查/压缩/实时指令常量 | `prompts/src/lib.rs:1`，`SUMMARIZATION_PROMPT`、`REVIEW_PROMPT`、`PermissionsInstructions`，通过 `include_str!` 引用 `templates/` |

### 2.2 Turn 七阶段实现位置

Turn 循环主入口为 `run_turn`（`core/src/session/turn.rs:162`），由 `RegularTask::run`（`core/src/tasks/regular.rs:40-87`）调用，外层由 `SessionTask` trait（`core/src/tasks/mod.rs:179`）与 `spawn_task`（`tasks/mod.rs:271`）驱动。

| 阶段 | 实现函数 | 文件:行号 |
|---|---|---|
| 1. admission（输入准入） | `CodexThread::start_or_steer_turn` / `start_turn_if_idle` / `recover_turn_if_idle` → `submit_turn_input_with_mode` → `AgentControl::ensure_execution_capacity_for_turn_start` → `Session::maybe_start_turn_for_pending_work` | `codex_thread.rs:313,325,349`，`tasks/mod.rs:427` |
| 2. snapshot（步骤快照） | `Session::capture_step_context_with_required_mcp_servers` → 产出 `StepContext` | `turn.rs:230,370,395`，`step_context.rs:15` |
| 3. sampling（采样请求） | `run_sampling_request` → `try_run_sampling_request` → `ModelClientSession::stream`，重试 `handle_retryable_response_stream_error` | `turn.rs:1411,2261,2300+`，`responses_retry.rs` |
| 4. tool dispatch（工具分发） | `ToolCallRuntime::handle_tool_call` → `ToolRouter::dispatch_tool_call_with_terminal_outcome`；MCP 工具走 `handle_mcp_tool_call` | `tools/parallel.rs:74`，`tools/router.rs:325`，`mcp_tool_call.rs:121` |
| 5. result writeback（结果回写） | `handle_output_item_done` → `finalize_non_tool_response_item` → `record_completed_response_item_with_finalized_facts` → `Session::record_conversation_items` 写入 `ContextManager` | `stream_events_utils.rs:290,245,92` |
| 6. compaction（压缩） | `run_pre_sampling_compact`（前置）+ `run_auto_compact`（中段）→ `compact::run_inline_auto_compact_task` / `compact_remote_v2::run_inline_remote_auto_compact_task` | `turn.rs:1082,1248`，`compact.rs:119,151`，`compact_remote_v2.rs:82` |
| 7. complete/interrupt | 完成：`run_turn_stop_hooks` + `emit_thread_idle_lifecycle_if_idle`；中断：`run_turn_interrupt_hooks` / `abort_all_tasks` / `abort_turn_if_active` | `hook_runtime.rs`，`tasks/lifecycle.rs:43`，`tasks/mod.rs:30,807,965,509,540` |

### 2.3 五原语严格区分

> **CLAUDE.md 原文**："Key primitives, which you must keep distinct or you'll design the data model wrong"

| 原语 | 定义位置 | 层级 | 说明 |
|---|---|---|---|
| **Thread** | `CodexThread`（`codex_thread.rs:177`），`ThreadManager`（`thread_manager.rs:226`） | 协议层 | app-server 长生命周期会话对象，支持 `fork`/`resume`/`archive`/`rollback`。持有 `Arc<Session>`、`SessionIo`、`session_source`、`rollout_path` |
| **Session** | `session/session.rs:42` | 内部运行时 | Core 的内部运行态持有者：`state: Mutex<SessionState>`、`conversation`、`active_turn`、`input_queue`、`services: SessionServices`。**不对外暴露** |
| **Turn** | `TurnContext`（`session/turn_context.rs:194`），`ActiveTurn`/`TurnState`（`state/turn.rs:33,90`） | 运行时 | 一次任务往返（用户消息→模型推理→工具调用→结果），可能含多个 Step。冻结单轮模型/权限/技能/插件视图 |
| **Step** | `StepContext`（`session/step_context.rs:15`），`ResolvedStepSettings`/`StepSettings`（`step_settings.rs`） | 采样 | 一次采样请求的不可变视图，含 `tool_router: Arc<ToolRouter>`、`mcp: Arc<McpBinding>`、`environments`、`loaded_agents_md` |
| **Item** | `TurnItem`（`protocol/src/items.rs:45`），`ResponseItem`（`protocol/src/models.rs`），`RolloutItem`/`ThreadItem`（`rollout.rs`） | 持久化 | Turn 中可持久化的原子事实：UserMessage/FunctionCallOutput/AgentMessage/Plan/McpToolCall/ContextCompaction 等 |

### 2.4 上下文管理、压缩、工具分发机制

**上下文管理**：`ContextManager`（`context_manager/history.rs:62`）维护 `ResponseItemEnvelope` 列表 + token 信息 + retained context + world state baseline。关键方法：`record_items`（:313）、`for_prompt`（:421，按 input_modalities 过滤生成 `Vec<ResponseItem>`）、`replace_compacted`（:530，压缩后整体替换历史）、`estimate_token_count`（:468）。

**压缩**：分本地与远程两路。本地 `compact.rs` 用 `SUMMARIZATION_PROMPT` 生成摘要并 `replace_compacted`；远程 `compact_remote_v2.rs:82` 走模型端 `RemoteCompactionSupport`（`turn.rs:1271` 按 provider capabilities 分流）。触发判定由 `context_window::context_window_token_status`（`session/context_window.rs:25`）提供 token_status。

**工具分发**：`ToolRouter`（`tools/router.rs:74`）由 `build_tool_router`（`tools/spec_plan.rs`，经 `built_tools`/`prepare_tool_recommendations` 在 `turn.rs:1570,1523` 构建）。`ToolCallRuntime`（`tools/parallel.rs:42`）驱动并行执行：`handle_tool_call`（:74）→ `dispatch_tool_call_with_terminal_outcome`（router:325），含 terminal-outcome 抢占（:167-191）。MCP 工具经 `handle_mcp_tool_call`（`mcp_tool_call.rs:121`）→ `handle_approved_mcp_tool_call`（:430），审批由 `request_mcp_tool_user_approval`（:1506）与 `guardian` 子模块协作。

### 2.5 上下文管理机制

**ContextManager**（`context_manager/history.rs:62`）维护以下核心状态：
- `ResponseItemEnvelope` 列表：每条含 `item` + `metadata: CodexHarnessMetadata`（`client_authored`、`fallback_token_limit_override`、`compaction_model_hash`、`user_input_order`、`inherited_user_message`）
- token 信息：`estimate_token_count`（:468）做预算记录
- retained context：主机侧有界事实（`VerifiedAnswer`、`RetainedUserMessage`，上限 8 记录/65KB），独立于模型 compaction 契约
- world state baseline：`update_world_state`（:280）维护基础世界状态

关键方法链路：
1. `record_items`（:313）——追加新 ResponseItem 到历史
2. `for_prompt`（:421）——按 input_modalities 过滤生成 `Vec<ResponseItem>` 供模型采样
3. `replace_compacted`（:530）——压缩后整体替换历史（旧条目被 CompactedItem 替代）
4. `update_world_state`（:280）——更新世界状态基线

`context_manager/updates.rs` 处理增量注入，`normalize.rs` 做归一化。

### 2.6 压缩机制详解

压缩分**本地**与**远程**两路：

**本地压缩**（`compact.rs`）：
- `run_inline_auto_compact_task`（:119）——用 `SUMMARIZATION_PROMPT`（`prompts` crate）生成摘要
- `run_compact_task`（:151）——完整压缩任务
- `InitialContextInjection`（:74）——控制压缩后是否在末条 user message 前注入初始上下文

**远程压缩**（`compact_remote_v2.rs`）：
- `run_inline_remote_auto_compact_task`（:82）——走模型端 `RemoteCompactionSupport`
- `turn.rs:1271` 根据 provider capabilities 分流：OpenAI/Azure Responses provider 启用 `RemoteCompactionSupport::V2`（`provider.rs:353-366`）
- 辅助模块：`compact_remote.rs`、`compact_remote_history.rs`（历史保留策略）、`compact_token_budget.rs`（预算管理）、`compact_model_fallback.rs`（模型回退）

**触发判定**：
- `context_window::context_window_token_status`（`session/context_window.rs:25`）提供 token_status
- `token_budget.rs` 做预算记录
- 前置压缩 `run_pre_sampling_compact`（`turn.rs:1082`），中段压缩 `run_auto_compact`（`turn.rs:1248`）

### 2.7 工具分发机制

**ToolRouter**（`tools/router.rs:74`）由 `build_tool_router`（`tools/spec_plan.rs`）构建：
- `built_tools` / `prepare_tool_recommendations` 在 `turn.rs:1570,1523` 构建工具路由表
- `dispatch_tool_call_with_terminal_outcome`（router:325）——分发工具调用，含 terminal-outcome 抢占（:167-191）
- `dispatch_tool_call_with_code_mode_result`（router:302）——Code Mode 结果分发
- `build_tool_call`（router:95）——构建工具调用
- `tool_supports_parallel`（router:95）——判断工具是否支持并行

**ToolCallRuntime**（`tools/parallel.rs:42`）驱动并行执行：
- `handle_tool_call`（:74）——入口，分发到具体 handler
- 终端结果抢占（:167-191）——某工具返回 terminal outcome 时抢占其他并行调用

**MCP 工具链路**：
- `handle_mcp_tool_call`（`mcp_tool_call.rs:121`）——MCP 工具入口
- `handle_approved_mcp_tool_call`（:430）——审批后执行
- `request_mcp_tool_user_approval`（:1506）——请求用户审批
- `guardian` 子模块协作——自动审批与严格复核

### 2.8 对外暴露的关键 trait 与接口

| 接口 | 关键方法 | 文件:行号 |
|---|---|---|
| `ThreadManager` | `start_thread`、`get_thread`、`fork_thread`、`spawn_subagent`、`resume_thread_from_rollout`、`shutdown_all_threads_bounded` | `thread_manager.rs:226` |
| `CodexThread` | `start_or_steer_turn`、`steer_turn`（:434）、`suspend_turn_and_shutdown`（:403）、`inject_response_items`（:609）、`next_event`（:569）、`update_thread_metadata`（:695） | `codex_thread.rs:177` |
| `SessionTask`/`AnySessionTask` | `kind`、`run`，被 `spawn_task` 泛型调用 | `tasks/mod.rs:179,221` |
| `ToolRouter` | `dispatch_tool_call_with_code_mode_result`、`build_tool_call`、`tool_supports_parallel` | `tools/router.rs:95` |
| `ContextManager` | crate-private，通过 `Session::clone_history`/`record_conversation_items` 间接对外 | `context_manager/history.rs:62` |

`core-api` 再导出：`Config`/`Constrained`/`ExtraConfig`（`core-api/src/lib.rs:79-80`）、`StateDbHandle`、`build_models_manager`。`core-plugins` 对外：`PluginsManager`（`manager.rs`）、`loader`、`marketplace`、`store`、`startup_sync`。`protocol` 全量 `pub mod`：`items`、`models`、`protocol`、`turn_input`、`permissions`、`mcp_policy`、`openai_models`。

### 2.9 crate 依赖关系

`protocol` 是叶子数据层，被 `core`、`core-plugins`、`context-fragments`、`prompts` 全部依赖。依赖链：

```
protocol (叶子)
  ↑
  ├── context-fragments (依赖 protocol)
  ├── prompts (依赖 protocol + context-fragments + execpolicy + git-utils)
  ├── core-plugins (依赖 protocol + mcp + plugin + skills + tools + connectors + http-client)
  └── core (依赖 protocol + context-fragments + core-plugins + prompts + mcp + tools +
            extension-api + model-provider + network-proxy + execpolicy + hooks +
            guardian-context + agent-graph-store + agent-roles + models-manager +
            history + state + ... 约 40 个 crate)
       ↑
       └── core-api (仅依赖 core + config + 少量外围，作为稳定门面)
            ↑
            ├── app-server (仅依赖 core-api，不直接触达 core 内部)
            ├── cli
            └── codex-client
```

### 2.10 为何不膨胀 core

CLAUDE.md 明确规定："**Resist adding code to `codex-core`. It's already bloated.**" 理由：

1. **编译时间**：core 已依赖约 40 个 crate，再膨胀将导致增量编译时间恶化
2. **审查困难**：大文件 diff 噪声大，reviewer 难以快速判断变更安全性
3. **上游冲突**：core 是上游高频变动区，日均 10+ commit，本地膨胀加剧 merge 冲突
4. **门面隔离**：`core-api` 作为稳定门面，使 `app-server`/`cli` 等上游仅依赖门面而非 core 内部——新概念应放入新 crate 或已有小 crate

`core-plugins` 的 marketplace 机制（`marketplace_policy.rs`、`installed_marketplaces.rs`）允许企业私有 marketplace，是工具/插件扩展的正确位置。

### 2.11 在 Nexus 企业平台化中的定位与可扩展点

核心引擎层是 Nexus 平台的"运行时内核"：`ThreadManager` 提供多线程/子智能体编排（`spawn_subagent`、`fork_thread`、`agent/registry.rs` 的 spawn 深度限制 `exceeds_thread_spawn_depth_limit`），`Session` 提供单线程内的轮次/任务调度，`protocol` 提供 API 协议契约，`core-api` 作为对外稳定门面——这种"门面+协议+内核"三明治结构使上层（app-server、cloud-tasks、collaboration-mode）可独立演进。

可扩展点：
1. **工具扩展**——`ToolRouter`/`CoreToolRuntime`（`tools/router.rs:241`）+ `codex-tools::DiscoverableTool` 支持动态注册；`core-plugins` marketplace 允许企业私有 marketplace
2. **模型 provider**——`model-provider` + `model-provider-info` + `models-manager` 抽象使自托管/合规模型可插拔，`compact_remote_v2` 已支持 `RemoteCompactionSupport` 能力协商
3. **上下文片段**——`ContextualUserFragment` trait（`context-fragments/src/fragment.rs`）允许注入企业知识库/合规上下文
4. **agent 角色**——`agent/role.rs` 的 `resolve_role_config`/`build` 支持自定义角色配置（`agent-roles` crate）
5. **执行隔离**——`sandboxing`、`exec_policy`、`unified_exec`、`network_policy_decision` 提供沙箱与策略钩子
6. **hooks**——`hook_runtime.rs` 的 turn start/stop/interrupt hook 为平台化注入审批/审计提供统一扩展面

---

## §3 app-server 协议层

### 3.1 crate 职责与关键源文件

| Crate | 职责 | 关键源文件 |
|---|---|---|
| **app-server-protocol** | 纯协议定义层：JSON-RPC 消息封装、四向枚举（ClientRequest/ServerRequest/ServerNotification/ClientNotification）、generate-ts/json-schema 导出器 | `rpc.rs`（JSON-RPC 封装）、`protocol/common.rs`（四个枚举与宏）、`protocol/v1.rs`（旧稳定 API）、`protocol/v2/`（新 API）、`precomputed_exports.rs` |
| **app-server-protocol-noop-macros** | proc-macro，生产构建提供 no-op `JsonSchema`/`TS` derive，测试构建切换为真实实现 | `app-server-protocol-noop-macros/src/lib.rs:11-19` |
| **app-server-transport** | 传输层：`AppServerTransport` 枚举（Stdio/UnixSocket/WebSocket/Off）+ `TransportEvent` | `transport/mod.rs`（`from_listen_url:116`）、`stdio.rs`、`unix_socket.rs`、`websocket.rs`、`auth.rs` |
| **app-server** | 核心服务进程：消息处理、请求分发、线程生命周期 | `lib.rs`、`message_processor.rs`（请求总入口）、`outgoing_message.rs`（事件 fan-out）、`thread_state.rs`、`request_processors/` |
| **app-server-daemon** | 托管 app-server 进程生命周期：start/stop/restart/version、PID/lock、更新循环 | `lib.rs`、`backend/`、`update_loop.rs` |
| **app-server-client** | in-process 客户端门面：initialize 握手、typed 请求/通知派发、优雅关闭 | `app-server-client/src/lib.rs:1-17` |
| **app-server-test-client** | CLI 测试客户端：loopback 响应服务器、插件 analytics 捕获 | 端到端协议验证 |

### 3.2 JSON-RPC 2.0 协议设计

协议是「类 JSON-RPC 2.0」但**不发送/不要求 `"jsonrpc":"2.0"` 字段**（`rpc.rs:1-2` 注释："We do not do true JSON-RPC 2.0"）。`JSONRPCMessage` 为 `untagged` 枚举（`rpc.rs:37`），区分 `Request`（带 id 期望响应）、`Notification`（无 id）、`Response`、`Error`。`RequestId` 支持 String/Integer（`rpc.rs:17-21`），`JSONRPCRequest` 可选携带 W3C `trace` 分布式追踪上下文（`rpc.rs:52-55`）。

**v1 vs v2** 是协议 API 版本划分而非传输版本：
- **v1**（`protocol/v1.rs`）：保留旧稳定类型如 `InitializeParams`、`ApplyPatchApprovalParams` 等
- **v2**（`protocol/v2/`）：新 API 命名空间，所有类型 `#[ts(export_to = "v2/")]`，用 `#[experimental("...")]` 标注未稳定方法

### 3.3 传输层

| 传输 | URL 格式 | 实现 | 备注 |
|---|---|---|---|
| stdio | `stdio://` | `stdio.rs:24`，按行 JSONL | 默认传输 |
| Unix socket | `unix://[PATH]` | `unix_socket.rs`，WebSocket-over-UDS + HTTP Upgrade | 默认 `$CODEX_HOME/app-server-control/app-server-control.sock` |
| WebSocket | `ws://IP:PORT` | `websocket.rs` | 实验性/不支持 |
| off | `off` | 不暴露本地传输 | — |

所有传输统一产出 `TransportEvent::IncomingMessage{JSONRPCMessage}`（`mod.rs:182`）并消费 `QueuedOutgoingMessage`，背压通过 128 容量 mpsc + `OVERLOADED_ERROR_CODE=-32001`（`mod.rs:52,229-258`）处理。

### 3.4 ServerNotification 事件流

`ServerNotification`（`common.rs:1851-1973`）由 `server_notification_definitions!` 宏生成，`#[serde(tag="method", content="params")]`，覆盖：

- **thread/\***：`started`、`status/changed`、`archived`、`deleted`、`unarchived`、`closed`、`reverted`、`name/updated`、`goal/{updated,cleared}`、`queue/changed`、`project/updated`、`tokenUsage/updated`、`settings/updated`、`compacted`
- **turn/\***：`started`、`completed`、`diff/updated`、`plan/updated`、`moderationMetadata`
- **hook/\***：`started`、`completed`
- **item/\***：`started`、`completed`、`agentMessage/delta`、`plan/delta`、`commandExecution/outputDelta`、`commandExecution/terminalInteraction`、`fileChange/outputDelta`、`fileChange/patchUpdated`、`mcpToolCall/progress`、`reasoning/*`、`autoApprovalReview/*`
- 其他：`rawResponse{Item,}/completed`、`command/exec/outputDelta`、`process/{outputDelta,exited}`、`mcpServer/*`、`account/*`、`model/*`、`fs/changed` 等

事件推送：`OutgoingMessageSender::send_server_notification`（`outgoing_message.rs:585`）封装为 `ServerNotificationEnvelope`（带 `emittedAtMs`），通过 `Broadcast` 或 `ToConnection` 路由（`outgoing_message.rs:607-631`）。订阅模型由 `ThreadStateManager`（`thread_state.rs`）维护 per-thread `connection_ids` 集合。

### 3.5 Server→Client 审批请求

审批通过 **Server→Client 请求**实现，客户端响应后才继续 turn：

| 请求方法 | 参数结构 | 文件:行号 |
|---|---|---|
| `item/commandExecution/requestApproval` | `CommandExecutionRequestApprovalParams`（含 `kind`/`approval_id`/`network_approval_context`/`additional_permissions`/`proposed_execpolicy_amendment`/`available_decisions`） | `item.rs:1533` |
| `item/fileChange/requestApproval` | `FileChangeRequestApprovalParams` | `item.rs:1616` |
| `item/permissions/requestApproval` | 请求额外文件/网络权限，响应 `GrantedPermissionProfile` + `scope`(Turn/Session) | `permissions.rs:773` |
| `item/tool/requestUserInput` | 用户输入请求 | — |
| `mcpServer/elicitation/request` | MCP elicitation | — |
| `item/tool/call` | DynamicToolCall（动态工具） | — |

审批流由 `thread/start` 的 `approval_policy` 与 `approvals_reviewer`（`thread.rs:88-92`）控制路由。`thread/approveGuardianDeniedAction`（`common.rs:677`）允许手动推翻 guardian 拒绝。

### 3.6 thread/resume/fork/revert/rollback

| 操作 | 实现 | 文件:行号 |
|---|---|---|
| **resume** | `thread_resume` → `thread_resume_inner`（循环直到 `ControlFlow::Continue` 消失），处理 running-thread 重入与 rollout path 校验 | `thread_processor.rs:544`，参数 `ThreadResumeParams`（`thread.rs:335`） |
| **fork** | `thread_fork` → `thread_fork_inner`，支持 `last_turn_id`(inclusive)/`before_turn_id`(exclusive)/`ephemeral`/`defer_goal_continuation` | `thread_processor.rs:568`，`thread.rs:518`，goal 继承 `thread_fork_goal.rs` |
| **archive/unarchive** | 仅修改元数据标记，不卸载运行态；发 `ThreadArchived`/`ThreadUnarchived` 通知 | `thread_processor.rs:614,746,1660,2047` |
| **revert** | 替换分页 thread 的持久历史前缀：`wait_for_thread_shutdown` + drain + `remove_thread` 重建 runtime，返回 `turns_backwards_cursor`/`items_backwards_cursor` 供分页回填，发 `ThreadReverted` 通知 | `thread_processor.rs:587,2093-2304`，`ThreadRevertParams.before_turn_id`（`thread.rs:1259`） |
| **rollback** | **已 deprecated**，`num_turns` 从尾部丢弃 N 个 turn，仅改历史不回滚文件 | `thread_processor.rs:803,2088,2314` |

### 3.7 核心 RPC 方法清单

方法清单在 `app-server-protocol/src/protocol/common.rs` 的 `client_request_definitions!` 宏（行 506-1426）与 `server_request_definitions!`（行 1696-1767）中集中声明。

**thread/\***（`common.rs:524-818`）：
- 生命周期：`thread/start`、`thread/resume`、`thread/fork`、`thread/archive`、`thread/unarchive`、`thread/delete`、`thread/unsubscribe`
- 元数据：`thread/name/set`、`thread/metadata/update`、`thread/settings/update`、`thread/memoryMode/set`、`memory/reset`
- 目标：`thread/goal/{set,get,clear}`
- 队列：`thread/queue/{add,list,update,delete,reorder,start}`
- 分区：`threadSection/{list,create,update,delete}`、`thread/section/move`
- 读取：`thread/list`、`thread/search`、`thread/searchOccurrences`、`thread/loaded/list`、`thread/read`、`thread/turns/list`、`thread/items/list`、`thread/inject_items`、`thread/timeline/list`
- 历史：`thread/rollback`（deprecated）、`thread/revert`
- 压缩：`thread/compact/start`
- 审批：`thread/approveGuardianDeniedAction`
- 终端：`thread/backgroundTerminals/{clean,list,terminate}`、`thread/shellCommand`
- 实时：`thread/realtime/{start,appendAudio,appendText,appendSpeech,stop,listVoices}`（实验性）
- 增减：`thread/increment_elicitation`、`thread/decrement_elicitation`

**turn/\***（`common.rs:982-999`）：`turn/start`、`turn/settings/update`、`turn/steer`、`turn/interrupt`

**app/\***（`common.rs:905-919`）：`app/read`、`app/list`、`app/installed`

**其他大块**：
- `initialize`、`server/diagnostics`
- `config/{read,value/write,batchWrite}`、`configRequirements/read`
- `fs/{readFile,writeFile,createDirectory,getMetadata,readDirectory,remove,copy,watch,unwatch}`
- `plugin/{list,search,installed,reconcile,read,install,uninstall,skill/read,share/*}`
- `skills/{list,extraRoots/set,config/write}`、`hooks/list`
- `marketplace/{add,remove,upgrade}`
- `command/exec{,/write,/terminate,/resize}`、`process/{spawn,writeStdin,kill,resizePty}`
- `model/{list}`、`modelProvider/capabilities/read`
- `experimentalFeature/{list,enablement/set}`、`permissionProfile/list`
- `remoteControl/{enable,disable,status/read,pairing/start,pairing/status,client/list,client/revoke}`
- `collaborationMode/list`、`environment/{add,info,status}`
- `project/{list,read,create,import,update,move,delete}`
- `mcpServer/{oauth/login,resource/read,event/stream/{start,stop},tool/call}`、`mcpServerStatus/list`、`config/mcpServer/reload`
- `account/{login/start,login/cancel,logout,read,rateLimits/read,rateLimitResetCredit/consume,usage/read,...}`
- `feedback/upload`、`windowsSandbox/{setupStart,readiness}`
- `externalAgentConfig/{detect,import,import/recordHistory,import/readHistories}`
- `fuzzyFileSearch{,/sessionStart,/sessionUpdate,/sessionStop}`、`review/start`

### 3.8 类型导出 generate-ts / generate-json-schema 机制

导出器在 `app-server-protocol/src/precomputed_exports.rs`，核心是**预计算 + 压缩内嵌**策略：稳定与实验两套导出以 zstd 压缩 JSON 内嵌于二进制（`precomputed_exports.rs:15-18`），运行时 `load_exports`（行 115）解压反序列化为 `PrecomputedExports{typescript, json_schema, internal_json_schema}` 三个 `BTreeMap<相对路径, 内容>`。

公开 API：`generate_types`（行 53）、`generate_ts`/`generate_ts_with_options`（支持 `generate_indices`/`ensure_headers`/`run_prettier`/`experimental_api`）、`generate_json`/`generate_json_with_experimental`、`generate_internal_json_schema`。

导出内容由 `#[cfg(test)]` 下用真实 `ts_rs::TS`/`schemars::JsonSchema` 的宏生成，产物落在 `schema/typescript/` 与 `schema/json/`。`app-server-protocol-noop-macros` 让非 test 构建零成本保留注解。

### 3.9 为何是 Nexus 主集成面

CLAUDE.md 明确："**Integrate via `app-server` (JSON-RPC), never `codex exec` or the in-process SDK**"：

| 集成方式 | 长生命周期 Thread | 双向事件流 | turn/interrupt | 协议级审批 | thread/resume/fork/revert | 崩溃恢复 |
|---|---|---|---|---|---|---|
| `codex exec` | ✗（一次性） | ✗ | ✗ | ✗ | ✗ | ✗ |
| in-process SDK | 部分 | ✗（进程内） | ✗ | ✗ | ✗ | ✗ |
| **app-server** | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

app-server 是唯一提供全量企业级能力的集成面。其传输可插拔（stdio/unix/ws/off），方法可演进（`#[experimental]` + `InitializeCapabilities.experimental_api` opt-in），扩展面广（MCP/插件/skills/hooks/远控/realtime/协作模式/Windows sandbox 均为协议级一等公民）。

`app-server-daemon` 提供托管 start/stop/restart/version 与更新循环；`app-server-test-client` 提供端到端协议验证与 loopback 响应服务器。客户端通过 `initialize` 握手协商能力（`opt_out_notification_methods`、`extensions`、`request_attestation`），后续按 thread 订阅模型收事件，构成完整的多客户端、多 thread、双向 RPC 集成基座。

---

## §4 持久化与状态层

### 4.1 crate 职责与关键源文件

| Crate | 职责 | 关键源文件 |
|---|---|---|
| **state** (codex-state) | SQLite 元数据镜像：从 rollout JSONL 抽取 thread/turn/item 元数据落库 | `state/src/lib.rs`、`runtime.rs`、`sqlite.rs`、`extract.rs`、`model/*`、`migrations/` |
| **thread-store** | 存储中立 trait `ThreadStore` + `LocalThreadStore`/`InMemoryThreadStore` | `thread-store/src/lib.rs`、`store.rs:52`、`local/mod.rs:428`、`live_thread.rs` |
| **rollout** | JSONL rollout 文件写入/读取/压缩/索引/搜索 | `rollout/src/lib.rs`、`recorder.rs`、`compression.rs`、`list.rs`、`state_db.rs`、`session_index.rs` |
| **rollout-trace** | 可选诊断 trace bundle：原始事件→离线 reducer→语义图 | `rollout-trace/src/lib.rs`、`bundle.rs`、`writer.rs`、`thread.rs` |
| **history** | rollout 领域类型：`RolloutItem`、`ResponseItemEnvelope`、`CompactedItem`、`RetainedContext` | `history/src/lib.rs`、`rollout_payload.rs`、`retained_context.rs` |
| **message-history** | 全局追加式 `~/.codex/history.jsonl` 用户消息历史 | `message-history/src/lib.rs`、`batch.rs` |
| **attachment-store** | 附件存储中立 trait：返回 `AttachmentRef`（含 `sediment://` URL） | `attachment-store/src/lib.rs` |

### 4.2 SQLite Schema（state crate）

state 运行时管理 **6 个独立 SQLite 文件**以减少锁竞争（`state/src/sqlite.rs:27-34`）：

| 文件 | 用途 |
|---|---|
| `state_5.sqlite` | 主元数据 |
| `logs_2.sqlite` | 日志 |
| `goals_1.sqlite` | 目标 |
| `memories_1.sqlite` | 记忆 |
| `queue_1.sqlite` | 队列 |
| `thread_history_1.sqlite` | 线程历史 |

关键表：`threads`（`migrations/0001_threads.sql`）、`thread_turns`、`thread_items`、`thread_goals`、`logs`、`queued_items`、`projects`/`project_roots`、`thread_artifacts`（`UNIQUE(thread_id,artifact_type,identity_key)`）、`thread_history_projection_state`（增量投影游标）。

### 4.3 rollout 文件格式

- **目录**：`~/.codex/sessions/`（活跃）与 `~/.codex/archived_sessions/`（`rollout/src/lib.rs:74-75`）
- **文件名**：`rollout-<YYYY-MM-DDTHH-MM-SS>-<thread_id>.jsonl`，revert 场景加 `_<rollout_id>` 后缀（`rollout_file_name.rs:39-58`）
- **格式**：JSONL，每行 `RolloutLine { timestamp, ordinal?, item }`，由 `decode_rollout_line`（`lib.rs:40-72`）解析
- **写入**：`RolloutRecorder` 通过 mpsc 通道发 `RolloutCmd::{AddItems, Persist, Flush, Shutdown}` 到后台 writer（`recorder.rs:90-130`）
- **压缩**：冷文件后台 `.jsonl.zst` 压缩（`compression.rs:18`），读取透明处理 plain/zst

### 4.4 thread-store vs state vs rollout 三者边界

| 层 | 职责 | 说明 |
|---|---|---|
| **rollout** | 本地 JSONL 文件原语 | 字节级落地层：写、读、压缩、列出、反向扫描、seekable reader |
| **state** | SQLite 元数据投影 | 从 rollout 抽取元数据（`extract.rs::apply_rollout_item`）并镜像到关系表，提供分页查询/搜索/logs/goals/queue |
| **thread-store** | 存储中立抽象 | 定义 `ThreadStore` trait，仅暴露 `create_thread`/`resume_thread`/`append_items`/`persist_thread` 等，`ThreadId` 为唯一持久句柄。`LocalThreadStore` 组合 `RolloutRecorder` + `StateDbHandle` |

README 明确："Stores persist explicit metadata fields, raw history appends remain history-only"——元数据推断应在 store 之上。这为远程/云端实现预留了空间（"Other storage implementations may live outside this repository"）。

### 4.5 关键数据结构与对外接口

- `RolloutItem` 枚举（`history/src/lib.rs`）——落盘单位，变体含 `SessionMeta`/`ResponseItem`/`Compacted`/`InterAgentCommunication`/`WorldState`/`RetainedContext`/`RealtimeItem`/`TokenUsageRecord`
- `RolloutLine{timestamp, ordinal, item}`（`rollout/src/lib.rs`）——JSONL 每行结构
- `StateRuntime`（`state/src/runtime.rs:101`）——持有 6 个连接池
- `SqliteConfig`/`RuntimeDbPath`（`state/src/sqlite.rs`）——SQLite 配置
- `StateDbHandle = Arc<StateRuntime>`（`rollout/src/state_db.rs:20`）——state 句柄
- `ThreadStore` trait + `LiveThread`/`LiveThreadInitGuard`（`thread-store/src/store.rs:52`，`live_thread.rs:34`）
- `LocalThreadStore`/`InMemoryThreadStore`——两种实现
- `LocalQueueStore`/`QueueStore` trait（`queue_store.rs:13`）——队列存储
- 元数据类型：`ThreadMetadata`/`ThreadMetadataBuilder`/`ThreadMetadataPatch`、`Anchor`/`SortKey`/`ThreadsPage`、`ThreadArtifact`/`ThreadArtifactPage`、`QueuedUserSubmissionRecord`、`ThreadGoal`、`Project`/`ProjectRoot`、`LogEntry`/`LogRow`、`BackfillState`/`RolloutMigrationState`
- rollout：`RolloutRecorder`/`RolloutRecorderParams::{Create,Resume}`/`RolloutCmd`、`RolloutLineReader`/`open_rollout_line_reader`/`open_rollout_seekable_reader`、`SessionIndexEntry`、`RolloutFileName`
- rollout-trace：`TraceWriter`/`TraceBundleManifest`/`RawTraceEvent`/`replay_bundle`，bundle = `manifest.json`+`trace.jsonl`+`payloads/*.json`+可选 `state.json`，受 `CODEX_ROLLOUT_TRACE_ROOT` 控制
- message-history：`HistoryEntry`/`HistoryConfig`/`HistoryBatch`
- attachment-store：`AttachmentStore` trait、`AttachmentMetadata`、`AttachmentRef`、`InlineAttachmentStore`

### 4.6 历史记录（history / message-history）

**history** crate：rollout 领域类型。`RolloutItem` 枚举是落盘单位；`ResponseItemEnvelope{item, metadata: CodexHarnessMetadata}` 保留 harness 元数据（`client_authored`、`fallback_token_limit_override`、`compaction_model_hash`、`user_input_order`、`inherited_user_message`）。`retained_context.rs` 维护主机侧有界事实（`VerifiedAnswer`、`RetainedUserMessage`，上限 8 记录/65KB），独立于模型 compaction 契约。`rollout_payload.rs` 定义 wire 格式 `RolloutItemWire`（`type` tag、snake_case）。

**message-history** crate：全局 `~/.codex/history.jsonl`，每行 `{"session_id","ts","text"}`。POSIX `O_APPEND` 单次 write 原子写，advisory lock + retry，超 `max_bytes` 软裁剪到 0.8。`batch.rs` 提供分页 `HistoryBatch`/`HistoryBatchCursor`（128 行/64KB 一批）。

### 4.7 与 Nexus 云端化的关系

- `thread-store` 的 `ThreadStore` trait 是存储中立边界，`LocalThreadStore` 即本地镜像实现范本——云端可用 Postgres 替换 SQLite 作为权威元数据，rollout JSONL 镜像到对象存储
- `rollout/src/model_context.rs:22` 注释："Local JSONL readers and future reverse-paged cloud readers can both feed their items through this scan"——云端分页 reader 已被设计进 model-context 重建路径
- `attachment-store` 的 `sediment://` URL 与 `codex-api/files.rs` 文件服务构成附件到对象存储的桥
- `rollout_migration_state`（迁移 0047）追踪迁移进度，启动时做 rollout→SQLite backfill 并等待 gate

---

## §5 沙箱安全与策略层

### 5.1 crate 职责与关键源文件

整个安全/策略层可划分为四组职责：

| 组 | Crate | 关键文件 |
|---|---|---|
| **策略规则引擎** | execpolicy | `parser.rs`（Starlark 解析器）、`policy.rs`（评估器）、`rule.rs`、`decision.rs`、`amend.rs`（运行时追加规则）、`execpolicycheck.rs`（CLI）、`sandbox_migration.rs` |
| **OS 级沙箱执行** | sandboxing, linux-sandbox, bwrap, windows-sandbox-rs, windows-sandbox-service | `sandboxing/src/manager.rs`（`SandboxManager::transform`）、`seatbelt.rs`（macOS）、`landlock.rs`（Linux argv）、`bwrap.rs`、`windows.rs`；`linux-sandbox/src/linux_run_main.rs`、`launcher.rs`、`bwrap.rs` |
| **进程加固与证明** | process-hardening, mxc-sandbox | `process-hardening/src/lib.rs`（pre-main `#[ctor::ctor]`）；`core/src/attestation.rs`（`AttestationProvider` trait + `x-oai-attestation` header） |
| **凭证与守护** | secrets, keyring-store, workload-identity, guardian-context | `secrets/src/lib.rs`、`local.rs`、`sanitizer.rs`；`keyring-store/src/lib.rs`；`workload-identity/src/exchange.rs`；`guardian-context/src/lib.rs` |

### 5.2 execpolicy 规则引擎

**规则文件格式**：Starlark 脚本（`parser.rs:59-66`，`Dialect::Extended` + f-string）。三个内置函数：
- `prefix_rule(pattern=[...], decision="allow"|"prompt"|"forbidden", match=[...], not_match=[...], justification="...")`
- `network_rule(host="...", protocol="http"|"https"|"socks5_tcp"|"socks5_udp", decision=..., justification=...)`
- `host_executable(name="...", paths=[...])`

**评估器**：`Policy::check`（`policy.rs:225-249`）→ `matches_for_command_with_options`（:305-332）：先按 cmd[0] 精确匹配 `rules_by_program`，若无规则命中且提供了 `heuristics_fallback`，返回单条 `HeuristicsRuleMatch`。`Evaluation::from_matches` 对所有命中规则的 `Decision` 取 `max()`（`policy.rs:402-411`），即 **Forbidden > Prompt > Allow**（`decision.rs:7-16` 的 Ord 派生）。

**per-tenant 叠加**：`Policy::merge_overlay`（`policy.rs:178-202`）把 overlay 策略叠加到基础策略——这正是 `core/src/exec_policy/model_policy.rs` 的 `ExecPolicyManager::current_for_environment` 所做：取基础 policy，与 `RequirementsExecPolicy` 做 `merge_overlay`（`model_policy.rs:14-25`）。`RequirementsExecPolicy` 持有 order-independent 的 `Policy` 且可序列化，使其可作为租户/环境级配置在控制平面与执行器间传输。

### 5.3 三层沙箱

| 层 | 平台 | 实现 | 关键文件:行号 |
|---|---|---|---|
| **容器层** | K8s Pod | NetworkPolicy + 资源限制 + RWX PVC 隔离 | Nexus 自建 |
| **Codex 命令级 OS 沙箱** | macOS | Seatbelt：`/usr/bin/sandbox-exec` + `.sbpl` 模板拼接 | `seatbelt.rs:63`，`manager.rs:395-432` |
| | Linux | Landlock + seccomp + bubblewrap（独立 mount/net namespace） | `landlock.rs:23-60`，`linux-sandbox/src/launcher.rs:38-57` |
| | Windows | Restricted token / AppContainer / WFP / ConPTY / deny-read ACL | `windows-sandbox-rs/src/token.rs`，`windows.rs:23-100` |
| **网络层** | 全平台 | NetworkPolicy 决策（per-process 网络策略） | `exec-server-protocol/src/network_policy.rs` |

### 5.5 进程加固与 attestation 机制

**process-hardening**（`process-hardening/src/lib.rs`）：`pre_main_hardening()` 设计为 `#[ctor::ctor]` 在 main 前调用。
- **Linux**：`prctl(PR_SET_DUMPABLE, 0)` 禁 ptrace + `setrlimit(RLIMIT_CORE, 0)` 禁 core dump + 清除 `LD_*` 环境变量
- **macOS**：`ptrace(PT_DENY_ATTACH)` + `RLIMIT_CORE=0` + 清除 `DYLD_*`
- **Windows**：留 TODO

**attestation**（`core/src/attestation.rs`）：定义 `AttestationProvider` trait，其 `header_for_request(context)` 返回 `x-oai-attestation` header 值。`AttestationContext` 携带 `thread_id`。这是 host 集成边界，具体证明生成策略由宿主实现。

**cyber_access_program**（`core/src/cyber_access_program.rs`）：`for_auth` 在 ChatGPT 认证下把 `CyberAccessProgram` 映射为 `AccessPrograms`，将安全访问计划接入 API 请求授权。

### 5.6 secrets / keyring-store / workload-identity 凭证管理

**keyring-store**（`keyring-store/src/lib.rs`）：定义 `KeyringStore` trait（`load`/`save`/`delete`），`DefaultKeyringStore` 封装 `keyring` crate 的 OS keychain。

**secrets**（`secrets/src/lib.rs`）：`SecretName` 约束为 `[A-Z0-9_]`；`SecretScope` 分 `Global` 与 `Environment(id)`，canonical key 为 `global/<name>` 或 `env/<id>/<name>`。`SecretsManager` 内部 `LocalSecretsBackend` 用 keyring 存主加密密钥、本地磁盘存加密密文。`compute_keyring_account` 对 codex_home 做 SHA-256 取 16 hex 作为 keyring account。`sanitizer::redact_secrets` 做输出脱敏。

**workload-identity**（`workload-identity/src/exchange.rs`）：`WorkloadIdentityExchange` 实现 RFC 7522 JWT-Bearer 令牌交换。`resolve()` 带单飞 `Mutex` 缓存短令牌，`refresh(observed_version)` 在下游拒绝时强制刷新，`invalidate_if_current` 按版本号失效。token URL 仅允许 HTTPS 或 loopback HTTP。

### 5.7 guardian-context 守护机制

**guardian-context**（`guardian-context/src/lib.rs`）为同步 Guardian 审查与异步 Guardian 评分组装共享上下文。核心抽象是 `SectionContributor` trait，每个 contributor 声明 `scope()` 并在 `contribute(input)` 中产出 `ContextSection`。`SectionRegistry` 按注册顺序收集，遇错即止。默认注册四个内建 section：`RootConversationSection`、`RetainedUserInstructionsSection`、`TrustedUserAnswersSection`、`ConversationTranscriptSection`。实现零拷贝、可并发复用。这是 Guardian 安全审查的数据供给层，不直接做隔离决策，而是把经证明的可信证据喂给审查/评分模型。

### 5.8 单租户命令级 vs 多租户隔离

上述所有 crate 实现的是**单进程、单租户、命令级**隔离：每条 shell 命令在 OS 沙箱内执行、受 execpolicy 规则约束。这些机制对"哪个租户发起的命令"无感知——`PermissionProfile`、`Policy` 均为进程级配置，不携带 tenant 身份。

在 Nexus 多租户架构中，这些 crate 属于**数据平面执行器**：控制平面（Nexus）负责租户认证、配额、租户→执行器调度、跨租户资源隔离与网络分段；沙箱/execpolicy 在执行器内部保证**单条命令不越权访问文件系统/网络/凭证**。`core/src/sandboxing/mod.rs` 的 `ExecRequest`（`mod.rs:50-74`）就是控制平面下发的执行单元边界，携带 `permission_profile`、`network`、`network_environment_id` 等字段——这些正是控制平面按租户策略填充后注入执行器的载体。

---

## §6 模型抽象与云控制层

### 6.1 crate 职责与关键源文件

| Crate | 职责 | 关键源文件 |
|---|---|---|
| **model-provider** | 运行时模型提供商抽象 trait `ModelProvider` | `provider.rs:148-329`，`create_model_provider`（:320-329） |
| **model-provider-info** | 可序列化 provider 元数据 `ModelProviderInfo` + 内置注册表 | `lib.rs:97-633`，`merge_configured_model_providers`（:551） |
| **responses-api-proxy** | 极简 OpenAI Responses API HTTP 代理 | `lib.rs:73-275`，README |
| **ollama** | 本地 Ollama 接入：模型拉取、版本校验（≥0.13.4） | `lib.rs:1-70`、`client.rs`、`pull.rs` |
| **lmstudio** | 本地 LM Studio 接入 | `lib.rs:1-46`、`client.rs` |
| **chatgpt** | ChatGPT backend-api 客户端 | `chatgpt_client.rs:55-176` |
| **login** | OAuth/Device Code/PKCE 登录流程、`AuthManager`、`CodexAuth` | `auth/mod.rs`、`device_code_auth.rs` |
| **aws-auth** | AWS SigV4 凭证加载与请求签名 | `lib.rs:19-195`、`signing.rs` |
| **backend-client** | 统一后端 HTTP 客户端 | `client.rs:135-756` |
| **cloud-config** | 云端配置 bundle 生命周期 | `service.rs:35-534`、`backend.rs:39-137` |
| **cloud-tasks** | `codex cloud` 子命令 TUI + CLI | `lib.rs:37-2024` |
| **cloud-tasks-client** | 云任务抽象 trait `CloudBackend` + HTTP 实现 | `api.rs:136-176`、`http.rs:25-66` |
| **codex-api** | Codex 与 OpenAI Responses/WebSocket 协议客户端 | `lib.rs:1-80`、`provider.rs` |
| **models-manager** | 模型目录管理：在线/离线刷新、文件缓存 | `manager.rs:37-120` |

### 6.2 模型提供商抽象

**元数据层 `ModelProviderInfo`**（`lib.rs:97-152`）：字段包括 `name`、`base_url`、`env_key`、`auth`、`aws`(SigV4)、`wire_api`(只支持 `Responses`)、`requires_openai_auth`、`supports_websockets`。`to_api_provider()`（:293-335）把它转成 `codex_api::Provider`——ChatGPT 系列 token 走 `https://chatgpt.com/backend-api/codex`，否则 `https://api.openai.com/v1`。

**内置 provider 注册表**（`lib.rs:512-544`）：`openai`、`amazon-bedrock`、`amazon-bedrock-runtime`、`ollama`(端口 11434)、`lmstudio`(端口 1234)。

**运行时 trait `ModelProvider`**（`provider.rs:148-304`）：核心方法 `info()`、`capabilities()`（返回 `ProviderCapabilities` 含 `remote_compaction`/`namespace_tools`/`web_search`）、`auth()`、`api_provider()`。`ConfiguredModelProvider` 对 OpenAI/Azure Responses provider 启用 `RemoteCompactionSupport::V2`（`provider.rs:353-366`）。

### 6.3 responses-api-proxy

极简 HTTP 代理（`lib.rs:73-275`）：`run_main()` 从 stdin 读 auth header，用 `tiny_http::Server` 在 `127.0.0.1` 监听。**只允许 `POST /v1/responses`**，转发时剥离 incoming `Authorization`/`Host`，注入 `Authorization: Bearer <key>`。定位是"特权用户代非特权用户调用 OpenAI"：root 持 key，非特权用户通过 `--server-info` JSON 拿到 port。硬核安全：栈缓冲 + `zeroize` + `mlock(2)` 防止 key 落盘交换。这是 Nexus "自建 Model Gateway" 的本地单租户极简形态。

### 6.4 云控制平面

**backend-client** 是统一后端 HTTP 客户端（`client.rs:136-160`），`PathStyle`（:117-133）区分 `CodexApi`（`/api/codex/...`）与 `ChatGptApi`（`/wham/...`）。方法覆盖：`list_tasks`、`get_task_details`、`create_task`、`get_config_bundle`（下发 enterprise-managed TOML）、`get_rate_limits`/`get_token_usage_profile`（计量与配额）。

**cloud-config** 负责配置 bundle 传输、缓存、刷新：启动 `load_startup_bundle_with_timeout`（15s 超时），miss 时 `fetch_remote_bundle_and_update_cache_with_retries`（最多 5 次），后台每 15 分钟刷新，401 触发 auth recovery。`cloud_config_eligible_auth` 只对 business/education/Enterprise plan 启用。

**cloud-tasks** 是 `codex cloud` 子命令 TUI/CLI 前端：`exec`/`status`/`list`/`diff`/`apply` 子命令，ratatui TUI 模式。

### 6.5 登录与认证流程

**login crate** 统一管理凭证。`CodexAuth` 是枚举，变体包括：`ApiKey`、`Chatgpt`、`ChatgptAuthTokens`、`Headers`、`AgentIdentity`、`PersonalAccessToken`、`BedrockApiKey`、`BedrockAccessKeys`。`AuthManager` 持有 shared 状态，`auth()` async 返回当前凭证，`unauthorized_recovery()` 用于 401 后 token 刷新。子模块：`access_token.rs`（OAuth access token）、`agent_identity.rs`、`bedrock_access_keys.rs`/`bedrock_api_key.rs`、`personal_access_token.rs`、`workload_identity.rs`（GCP/AWS Workload Identity）、`external_bearer.rs`（外部命令 bearer）。`device_code_auth.rs` 实现 OAuth Device Code Flow + PKCE。

**chatgpt crate** 是 ChatGPT backend-api 客户端层。`chatgpt_client.rs` 提供 `chatgpt_get_request`/`chatgpt_post_request_with_timeout`，强制 `auth.uses_codex_backend()` 且 `auth.get_account_id().is_some()`，注入 `OAI-Product-Sku: codex` header。

**数据流**：
1. 模型选择：`config.toml` `model_providers` → `merge_configured_model_providers`（`lib.rs:551`）→ `ModelProviderInfo` → `create_model_provider`（`provider.rs:320`）→ `ModelProvider` trait → `api_provider()` → `codex_api::Provider` → `ResponsesClient` 发请求
2. Auth：`codex login`（Device Code + PKCE）→ `AuthManager` → `CodexAuth` → `ModelProvider::api_auth_for_scope` → `resolve_provider_auth` → `SharedAuthProvider` → `codex-api` 注入 header。Bedrock：`AwsAuthContext::sign` 做 SigV4 签名
3. 云端任务：`codex cloud` TUI → `CloudBackend` trait → `HttpClient` → `backend-client::Client` → `/wham/tasks*` 端点
4. 云端配置：`AuthManager` → `cloud_config_eligible_auth` 门控 → `BackendBundleClient::get_bundle` → `ConfigBundleResponse` → `CloudConfigBundle`（enterprise_managed TOML）→ 缓存 → `codex-config` 解析合并

### 6.6 codex-backend-openapi-models

`src/lib.rs:1-7` 声明"intentionally contains no hand-written types"，全部由 regen 脚本从后端 OpenAPI 生成到 `src/models/*.rs`。20 个生成文件覆盖：`rate_limit_status_payload`、`rate_limit_status_details`、`rate_limit_window_snapshot`、`additional_rate_limit_details`、`credit_status_details`、`spend_control_status_details`/`spend_control_limit_details`、`config_bundle_response`/`delivered_config_toml`/`delivered_requirements_toml`/`delivered_toml_fragment`/`delivered_managed_layers`、`task_list_item`/`paginated_list_task_list_item_`/`task_response`/`code_task_details_response`、`config_file_response`、`git_pull_request`/`external_pull_request_response`。这是云控制平面的"契约层"。

### 6.7 为何指向自建 Model Gateway & GPT-5.x 不开源的影响

CLAUDE.md 明确："**Codex did not open-source the model.**" Harness + 集成接口是开源的，但 GPT-5.x 是 API/ChatGPT-only。本地/私有模型通过 `ollama`/`lmstudio` provider 路由（能力受限）。

Nexus 自建 Model Gateway 体现为两种形态：
1. **本地单租户极简网关** = `responses-api-proxy`：计量+路由雏形（`--dump-dir` 采集 request/response pair）
2. **云端多租户控制平面** = `backend-client` + `cloud-config` + `cloud-tasks`：`ChatGPT-Account-Id`/FedRAMP header 多租户路由，`get_rate_limits`/`get_token_usage_profile` 计量配额，`get_config_bundle` 下发 enterprise-managed TOML 策略

Nexus 需自建 Model Gateway 来实现：多模型路由（OpenAI/Bedrock/OSS）、Token 计量与配额、租户级路由隔离、审计日志——这些都是 Harness 不提供的企业能力。

---

## §7 MCP/技能/工具/协作层

### 7.1 codex-mcp：MCP 客户端与连接管理

`codex-mcp` 是 MCP 聚合层。`McpConnectionSet`（`connection_manager.rs:1-8`）是 `McpRuntime`/`McpBinding` 背后的私有连接集合，协调启动状态、维护服务器元数据，跨运行中的 RMCP 客户端聚合工具与资源。子模块：`required.rs`（必需服务器）、`resources.rs`、`startup.rs`（含 ChatGPT 认证提供器）、`status.rs`、`tool_catalog.rs`。

`McpServerConnection`（`connection_manager.rs:60-118`）持有 `AsyncManagedClient`、启动超时、`watch::Sender<bool>` 触发器。连接复用校验连接身份、启动完成、客户端未关闭、OAuth 凭据匹配。`rmcp-client` crate 提供底层传输：stdio、HTTP、SSE、OAuth、EMA 身份交换。

### 7.2 skills：Markdown 技能系统

技能以 `SKILL.md` 文件定义（YAML frontmatter + Markdown 正文）。`SkillMetadata`（`model.rs:16-30`）含 name、description、interface、dependencies、policy、scope、plugin_id。`SkillPolicy` 控制隐式调用与产品门控。

加载机制：`SkillRootLoader` / `LoadedSkills` / `SkillRootSnapshots`。`parser.rs` 解析 frontmatter，`selection.rs` 收集显式技能提及，`invocation.rs` 检测隐式技能调用。

`ext/skills` 是运行时扩展层：多 Provider（`HostSkillProvider`/`ExecutorSkillProvider`/`OrchestratorSkillProvider`）支持本地与远程技能来源，`extension.rs:20-50` 注册为 `ConfigContributor`/`ContextContributor`/`SkillInvocationContributor`/`ToolContributor`/`ThreadLifecycleContributor`/`TurnInputContributor`。

### 7.3 hooks：生命周期钩子

`hooks` crate 实现 **12 个事件钩子**（`lib.rs:18-30`）：

| 钩子 | 支持 matcher | 说明 |
|---|---|---|
| `PreToolUse` | ✓ | 工具调用前 |
| `PermissionRequest` | ✓ | 权限请求 |
| `PostToolUse` | ✓ | 工具调用后 |
| `PreCompact` | ✓ | 压缩前 |
| `PostCompact` | ✓ | 压缩后 |
| `SessionStart` | ✓ | 会话开始 |
| `SessionEnd` | ✓ | 会话结束 |
| `UserPromptSubmit` | ✓ | 用户提交 |
| `SubagentStart` | ✓ | 子代理启动 |
| `SubagentStop` | ✓ | 子代理停止 |
| `Stop` | ✗ | 停止 |
| `Interrupt` | ✗ | 中断 |

`HookFn`（异步闭包）与 `HookResult`（`Success`/`FailedContinue`/`FailedAbort`）。引擎含 `command_runner.rs`（命令钩子）、`dispatcher.rs`（分发）、`mcp_runner.rs`（MCP 钩子）、`discovery.rs`（发现）、`schema_loader.rs`（JSON schema 加载）。

### 7.4 collaboration-mode-templates 与多 Agent 协作

- **collaboration-mode-templates**（`lib.rs`）：`include_str!` 内嵌 `templates/plan.md` 与 `templates/default.md`。`default.md` 定义 Default 模式（优先执行而非提问）；`plan.md` 定义 3 阶段对话式规划模式。模式通过 `<collaboration_mode>` 标签切换。
- **agent-roles**（`lib.rs`）：`AgentRoleConfig` 持有 description、config_file、nickname_candidates。`discovery.rs` 递归扫描 `.toml` 角色文件。
- **agent-identity**（`lib.rs`）：基于 ed25519 密钥对、`crypto_box`、JWT 的 Agent 密码学身份。JWT issuer `https://chatgpt.com/codex-backend/agent-identity`，audience `codex-app-server`。
- **agent-graph-store**（`store.rs:10-70`）：`AgentGraphStore` trait 存储 thread-spawn 父子拓扑（`upsert_thread_spawn_edge`、`list_thread_spawn_descendants` 广度优先）。`LocalAgentGraphStore` 为 SQLite 实现。
- **ext/agent**（`lib.rs`）：`AgentRunner` 通过 `ThreadManager` fork 线程启动子 Agent。
- **ext/guardian-v2**（`lib.rs:20-40`）：`GuardianExtension` 持有 `agent_spawner`，可委派子 Agent spawn 进行安全审查。

### 7.5 ext/extension-api 扩展机制

`ext/extension-api` 是核心扩展契约。贡献者 trait 矩阵（`contributors.rs:16-50`）：

| Contributor | 职责 |
|---|---|
| `ThreadLifecycleContributor` | 线程生命周期 |
| `TurnLifecycleContributor` | 轮次生命周期 |
| `TurnInputContributor` | 轮次输入注入 |
| `ToolContributor`/`ToolLifecycleContributor` | 工具注册与生命周期 |
| `ContextContributor` | 上下文注入 |
| `ConfigContributor` | 配置贡献 |
| `ApprovalReviewContributor` | 审批审查 |
| `SkillInvocationContributor` | 技能调用 |
| `McpServerContributor` | MCP 服务器动态注入/移除 |
| `TokenUsageContributor` | Token 用量 |
| `TurnItemContributor` | Turn Item 贡献 |

新能力通过实现对应 Contributor trait 并在 `install` 函数注册即可接入。`AgentSpawner` trait 允许扩展（如 guardian）请求 host 派生子 Agent。

### 7.6 文件操作工具

| Crate | 职责 | 关键特性 |
|---|---|---|
| **file-search** | 基于 `ignore`（WalkBuilder）遍历 + `nucleo` 模糊匹配引擎 | `crossbeam-channel` 并行，提供 CLI |
| **file-system** | `ExecutorFileSystem` 抽象 | 封装沙箱策略（`FileSystemSandboxPolicy`）、权限配置（`PermissionProfile`）、流式读取（`FILE_READ_CHUNK_SIZE=1MB`）、目录遍历限制（`MAX_WALK_DEPTH=64`, `MAX_WALK_DIRECTORIES=10000`） |
| **file-watcher** | 基于 `notify` crate 的 `RecommendedWatcher` | 订阅式文件/目录变更通知，`FileWatcherEvent` 合并去重路径 |
| **apply-patch** | 补丁解析与应用 | `parser.rs`（`Hunk`/`UpdateFileChunk`/`parse_patch`）、`streaming_parser.rs`（流式解析）、`file_update.rs`（`AppliedPatch`、`unified_diff_from_chunks`），依赖 `similar` diff 引擎 |
| **git-utils** | Git 操作全集 | `apply.rs`（补丁应用/暂存）、`baseline.rs`（基线 diff/重置）、`branch.rs`（merge-base）、`info.rs`（remote URL/分支/commit/`GitInfo`）、`status.rs`、`operations.rs`、`trust.rs`（信任根解析）、`fsmonitor.rs`、`git_process.rs`，`SAFE_BARE_REPOSITORY_CONFIG` 安全配置 |

### 7.7 与 Nexus 连接器治理/Skills 市场的关系

- **技能市场**：`connectors` crate 提供 App/Connector 目录、元数据存储、工具策略评估，`tools/tool_discovery.rs` 的 `DiscoverableTool` 支持安装/启用，`ext/skills` 多 Provider 支持本地与远程技能来源
- **MCP 作为工具供给通道**：`codex-mcp` 聚合本地与 Codex Apps 远程 MCP 服务器工具，`McpServerContributor` 允许扩展动态注入
- **多 Agent 协作**：`agent-identity` 密码学身份 + `agent-graph-store` 父子拓扑 + `ext/agent` 的 `AgentRunner` + `collaboration-mode-templates` 模式提示模板 + `ext/guardian-v2` 子 Agent 审查
- **扩展点**：贡献者 trait 矩阵是平台扩展的核心契约——构成插件式企业 Agent 平台架构

---

## §8 exec CLI / TUI / 可观测层

### 8.1 crate 职责与关键源文件

| Crate | 职责 | 关键源文件 |
|---|---|---|
| **exec** | 非交互式 Codex 执行器二进制（`codex-exec`） | `exec/src/main.rs:28-40`、`cli.rs:15-81`、`event_processor_with_jsonl_output.rs`、`event_processor_with_human_output.rs` |
| **exec-server** | 远程命令/进程/文件系统执行服务端 | `server.rs:46-61`、`local_process.rs`、`local_file_system.rs`、`relay.rs`（Noise 中继）、`rpc.rs`、`noise_channel.rs` |
| **exec-server-protocol** | JSON-RPC 协议定义 | `protocol.rs:20-57`、`rpc.rs`、`network_policy.rs` |
| **cli** | `codex` 二进制入口，统一命令分发 | `cli/src/main.rs`（183KB）、`login.rs`、`doctor.rs`（148KB）、`debug_sandbox.rs` |
| **tui** | ratatui 终端界面 | `main.rs:21-57`、`app.rs`（42KB）、`chatwidget.rs`（84KB）、`tui.rs`、`keymap.rs`（155KB）、`markdown_render.rs`（101KB） |
| **otel** | OpenTelemetry 全栈 provider | `provider.rs:62-69`、`otlp.rs`、`trace_context.rs`（W3C traceparent 传播） |
| **otel-trace-websocket** | OTLP trace 批次转发到 WebSocket | `lib.rs:69,151-179` |
| **analytics** | 分析事件采集、reducer 聚合、Statsig 上报 | `client.rs`、`reducer.rs`（148KB）、`facts.rs` |
| **feedback** | 用户反馈收集、Sentry 上报 | `upload.rs:33-47`，DSN `o33249.ingest.us.sentry.io` |
| **rollout-trace** | rollout trace bundle 格式、append-only writer | `writer.rs:36-47`、`reducer/` |
| **realtime-webrtc / voice-host** | 实时语音 WebRTC 信令 + GStreamer 运行时 | `protocol.rs:60-71`、`client.rs:24-28`；`voice-host/src/main.rs`、`transport.rs` |

### 8.2 exec / exec-server 远程执行服务

**exec** crate（`exec/src/main.rs:28-40`）：`codex-exec` 二进制入口，支持 `Resume`/`Fork`/`Review` 子命令（`cli.rs:149-159`）。输出分两路：`EventProcessorWithJsonOutput`（JSONL）和 `EventProcessorWithHumanOutput`（人类可读）。`lib.rs:5` 以 `#![deny(clippy::print_stdout)]` 强约束 stdout 纯净性。

**exec-server**（`server.rs:46-61`）：以 `#[tracing::instrument(name="codex.exec_server")]` 根 span 启动。47 个模块，核心含 `local_process.rs`（77KB）、`local_file_system.rs`（46KB）、`relay.rs`（55KB，Noise IK 握手加密通道）、`rpc.rs`（47KB JSON-RPC 派发）、`capability_discovery.rs`。`ExecServerClient`（`client.rs`，123KB）是客户端门面。

**exec-server-protocol**（`protocol.rs:20-57`）：定义全部方法常量——`process/{start,read,write,signal,terminate}`、`fs/{readFile,open,writeFile,walk}`、`http/request`、`capabilityRoots/discoverV1`、`initialize` 等。`rpc.rs:21-26` 定义 `JSONRPC_VERSION="2.0"` 与 `MAX_JSONRPC_VALUE_NODES=256*1024` 防爆裂。

### 8.3 tui：ratatui 终端界面

`tui/src/main.rs:21-57` 是 `codex`（无子命令时）默认入口。核心 `App`（`app.rs`，42KB）协调 `AppServerSession`、`ChatWidget`、`BacktrackState`。`chatwidget.rs`（84KB）是聊天主控件。`app_server_session.rs`（150KB）是与 app-server 的会话桥。

关键模块：`keymap.rs`（155KB，键映射）、`markdown_render.rs`（101KB）、`diff_render.rs`（97KB）、`resume_picker.rs`（237KB）。

### 8.4 可观测性四层金字塔

| 层 | crate | 覆盖谱系 |
|---|---|---|
| **进程内 hot-path trace** | rollout-trace | 每次推理、工具派发、线程启动、compaction、code-mode cell 的原始事件，`trace.jsonl` bundle，可离线 `replay_bundle` 重建语义视图 |
| **分布式 trace** | otel | 全局 `OtelProvider`（tracer/logger/metrics），OTLP/gRPC 或 OTLP/HTTP 导出到远端；W3C traceparent 跨进程注入/提取，使 exec→exec-server→code-mode-host 调用链可串联 |
| **实时 trace 调试** | otel-trace-websocket | loopback OTLP trace 批次桥接到 WebSocket，开发者实时订阅 trace 流 |
| **业务级分析** | analytics + diagnostics + feedback | app-server JSON-RPC 事件流经 reducer 聚合为 turn/token/tool/guardian 维度 facts → Statsig；进程 gauge（内存/并发）；Sentry envelope 上报 |

### 8.5 code-mode 系列

四个 crate 构成代码模式（code-mode）分层：

| Crate | 职责 | 关键文件 |
|---|---|---|
| `code-mode-protocol` | 工具协议定义：`PUBLIC_TOOL_NAME="exec"`、`WAIT_TOOL_NAME="wait"`、`CodeModeSession` trait、`ExecuteRequest`/`WaitRequest` | `lib.rs:51-52`、`description.rs`（37KB）、`session.rs`、`runtime.rs`、`json_schema_types.rs`、`host/`、`grpc/` |
| `code-mode` | re-export protocol + 两种会话提供者：`GrpcCodeModeSessionProvider` 与 `ProcessOwnedCodeModeSessionProvider` | `lib.rs`、`remote_session.rs`（19KB） |
| `code-mode-host` | 宿主端，管理最多 256 并发请求、128 活跃 cell | `lib.rs:54-59`、`peer.rs`（18KB）、`grpc_transport.rs`、`delegate.rs` |
| `code-mode-runtime` | 进程内 V8 运行时 | `service.rs:33-50`（`InProcessCodeModeSession`）、`v8_init.rs`、`cell_actor/`、`session_runtime/` |

### 8.6 可观测性数据流

**执行数据流（TUI 路径）**：`cli/src/main.rs` → `codex_tui::run_main` → `App` → `AppServerSession`（150KB）→ `InProcessAppServerClient` 或 `RemoteAppServerClient` → app-server → `ExecServerClient`（`exec-server/src/client.rs`，123KB）→ JSON-RPC over Noise/WebSocket → `exec-server` `transport::run_transport` → `local_process.rs`/`local_file_system.rs` 或远程 `relay.rs`/`remote.rs`。

**执行数据流（exec 路径）**：`exec/src/main.rs:28-40` → `codex_exec::run_main` → `InProcessAppServerClient` → 事件经 `EventProcessorWithJsonOutput` 或 `EventProcessorWithHumanOutput` 输出。

**协议层**：`exec-server-protocol/src/rpc.rs` 的 JSON-RPC 信封（无 `"jsonrpc":"2.0"` 字段，Codex 方言）承载 `protocol.rs` 定义的方法；`RequestId` 可为 String 或 Integer。`W3cTraceContext` 在信封中传播 traceparent，实现端到端 trace 串联。

**语音数据流**：`voice-host/src/main.rs` 通过 stdin/stdout 帧协议与父进程通信；`StartTransport` → `Transport::new()` 创建 WebRTC PeerConnection → SDP offer/answer 交换 → `Runtime::initialize` 启动 GStreamer。音频数据不跨 stdio 管道（仅信令与 SDP），由 WebRTC data channel 独立承载。

**可观测数据流**：hot-path 代码调 `TraceWriter` 写本地 bundle → `otel` provider 批量导出 OTLP →（可选）`otel-trace-websocket` 桥接到 WS → `analytics` reducer 聚合 → Statsig/Sentry 上报。`exec-server` 的 `telemetry.rs`/`trace_context.rs` 在执行服务端各环节注入 span 与指标。

**传输辅助**：`uds` 提供跨平台 async UDS listener/stream；`stdio-to-uds` 是 stdio↔UDS 中继二进制；`websocket-client` 的 `WebSocketConnector` 基于代理策略建立 WebSocket 连接。`terminal-detection` 检测 `TerminalInfo` 喂入 OTel user-agent 语义属性。

### 8.7 为何 TUI 审批不能直接搬到 Web

CLAUDE.md 明确："**Codex's approval mechanism cannot be lifted to the Web directly.** It's a local TUI popup blocking in-process. Enterprise 'approve hours later on IM, Pod rebuilt in between' requires a self-built bridge that persists approvals to the DB and replays them on resume."

TUI 审批是进程内阻塞式弹窗——当 app-server 发出 `item/commandExecution/requestApproval` 等 Server→Client 请求时，TUI 在终端阻塞等待用户输入。企业场景需要：
1. 审批请求持久化到云端 DB（不依赖进程存活）
2. 通过 IM/Web 推送给审批人
3. 审批人可能在数小时后响应
4. 响应时原 Pod 可能已重建——需在 `thread/resume` 时回放审批决策

这需要 Nexus 自建审批桥接层，而非复用 TUI 审批机制。

---

## §9 架构特征总结表

| crate 群 | 核心 crate | Nexus 复用策略 | 说明 |
|---|---|---|---|
| **核心引擎** | codex-core, core-api, protocol, context-fragments, prompts | **黑盒不改** | 通过 core-api 门面 + protocol 协议交互，不修改内核。新概念放新 crate |
| **app-server 协议** | app-server, app-server-protocol, app-server-transport | **可直接复用** | 作为 Nexus 唯一集成面，JSON-RPC 双向通信，传输可插拔（stdio/unix/ws） |
| **持久化** | state, thread-store, rollout, history | **需自建外壳** | `ThreadStore` trait 是存储中立边界，`LocalThreadStore` 是范本——云端用 Postgres 替换 SQLite，rollout 镜像对象存储 |
| **沙箱安全** | execpolicy, sandboxing, linux-sandbox, windows-sandbox-rs | **可直接复用 + 承接策略** | 沙箱/execpolicy 是数据平面执行器内部机制，`merge_overlay` 承接 per-tenant 规则注入 |
| **进程加固** | process-hardening, attestation | **可直接复用** | pre-main 加固 + attestation header |
| **凭证管理** | secrets, keyring-store, workload-identity | **需自建外壳** | 单租户本地凭证→需多租户 KMS + 短期令牌 + MCP Gateway 侧车注入 |
| **模型抽象** | model-provider, model-provider-info, responses-api-proxy | **需自建外壳** | 指向自建 Model Gateway，`responses-api-proxy` 是极简形态范本 |
| **本地模型** | ollama, lmstudio | **可直接复用** | OSS provider 路由，能力受限但可用 |
| **云控制** | backend-client, cloud-config, cloud-tasks | **黑盒不改/参考** | 这些指向 ChatGPT backend-api，Nexus 自建控制平面替代 |
| **登录认证** | login, aws-auth | **需自建外壳** | OAuth/Device Code 流程可参考，企业需 OIDC/SAML/SCIM |
| **MCP** | codex-mcp, rmcp-client | **可直接复用** | 连接管理器可复用，MCP Gateway 侧车由 Nexus 自建 |
| **技能** | skills, ext/skills | **可直接复用** | Markdown 技能系统，多 Provider 支持本地与远程 |
| **钩子** | hooks | **可直接复用** | 12 生命周期钩子，企业可注入审计/合规 |
| **工具** | tools, file-search, file-system, apply-patch, git-utils | **可直接复用** | 内置工具集完整 |
| **协作** | collaboration-mode-templates, agent-roles, agent-graph-store | **参考实现** | 子代理/多代理编排参考，Nexus 需自建编排器 |
| **扩展 API** | ext/extension-api + 14 ext/* | **可直接复用** | 贡献者 trait 矩阵是平台扩展核心契约 |
| **exec** | exec, exec-server, exec-server-protocol | **可直接复用** | 非交互执行 + 远程执行服务 |
| **TUI** | tui | **不直接搬 Web** | 终端 UI，Web 前端需自建 |
| **可观测** | otel, analytics, feedback, rollout-trace | **可直接复用 + 自建汇聚** | OTel provider 可复用，汇聚层需对接 Nexus 存储与告警 |
| **语音** | realtime-webrtc, voice-host | **可直接复用** | 实时语音 WebRTC |
| **code-mode** | code-mode*（4 crate） | **可直接复用** | 代码模式协议与 V8 运行时 |
| **CLI** | cli | **可直接复用** | `codex` 二进制入口 |

---

## §10 Nexus 落地启示

### 10.1 控制/执行分离

这是整个架构最关键的设计判断（路线图 §0.1）：

> Codex Harness 给的是「单用户、本地、可打断的 Agent 引擎」；Nexus 要建的是「让这台引擎变成多租户、可计量、可审计、崩溃不丢会话的企业服务」。前者是执行内核（L5），后者是控制平面（L1–L4 + L6–L7），两者必须严格分离，且不改内核。

控制面长期有状态、多租户、强一致；执行面无状态、一次性、可随时销毁。仅经 app-server 协议与对象存储通信。沙箱一定跑不受信代码（依赖安装、用户脚本、MCP 子进程 stdio），放进控制面等于把整个平台暴露。

### 10.2 app-server 集成

- **唯一集成面**：通过 `app-server` JSON-RPC 集成，不通过 `codex exec` 或 in-process SDK
- **Thread 生命周期**：消费 `thread/started`/`thread/status/changed`/`turn/started`/`turn/completed`/`item/*` 事件流
- **审批桥接**：消费 `item/commandExecution/requestApproval` 等 Server→Client 请求，持久化到 DB，推送给审批人，在 `thread/resume` 时回放
- **Thread 操作**：`thread/start`/`resume`/`fork`/`revert` 支持会话恢复、分支、回退

### 10.3 会话云端化

- 控制面消费 app-server 事件流 → Postgres（权威元数据）
- rollout JSONL 同步到对象存储（可重建的执行态）
- `ThreadStore` trait 屏蔽本地/云端差异，`LocalThreadStore` 是本地镜像实现范本
- Pod 死亡时：从 Postgres 恢复 Thread 元数据 → 从对象存储拉取 rollout → `thread/resume` 重建运行态

### 10.4 execpolicy 承接策略

- 控制平面为租户生成 overlay 规则文件（Starlark `prefix_rule`/`network_rule`）
- 执行器加载租户 overlay → `Policy::merge_overlay` 到基础 policy
- `Policy::check` 对每条命令评估 allow/prompt/deny
- `RequirementsExecPolicy` 可序列化，作为租户/环境级配置在控制平面与执行器间传输
- `amend.rs` 的 `blocking_append_*` 支持运行时增量追加规则（学习机制）

### 10.5 不改内核

- 上游日均 10+ commit、111+ crate——修改内核导致 merge 冲突无法收口
- 租户/角色差异表达为生成的 `config.toml` + `execpolicy` 规则集
- 事件桥接消费 app-server 事件流
- 外壳包装在 L4 执行面
- 不可避免的内核 patch 放 `patches/` + 上游追踪看板

### 10.6 四重隔离

| 隔离层 | 机制 | 对应 crate |
|---|---|---|
| **容器层** | K8s Pod + NetworkPolicy + 资源限制 | Nexus 自建 |
| **OS 沙箱层** | Seatbelt / Landlock+seccomp+bubblewrap / Windows token | sandboxing, linux-sandbox, windows-sandbox-rs |
| **策略层** | execpolicy Starlark 规则引擎 + approval_policy | execpolicy |
| **网络层** | NetworkPolicy + 进程级网络策略决策 | exec-server-protocol/network_policy.rs |

这四重隔离确保：单条命令不越权访问文件系统/网络/凭证（OS 沙箱 + execpolicy）；跨租户资源隔离（容器层 + 网络层）；沙箱内零长期密钥（短期令牌 + MCP Gateway 侧车注入）。

---

> **报告总结**：Codex Harness（`codex-rs/`）提供了完备的 Agent 执行内核（Turn 七阶段 + 五原语）、成熟的 JSON-RPC 协议层（app-server 双向事件流 + 协议级审批）、可靠的本地持久化（SQLite 镜像 + JSONL rollout）、强力的命令级安全（execpolicy Starlark + 三平台 OS 沙箱）和灵活的扩展机制（ext/extension-api 贡献者矩阵）。Nexus 的落地路径清晰：以 app-server 为唯一集成面，消费事件流写云端 Postgres，下发 config.toml + execpolicy 表达租户差异，不改内核，四重隔离保障安全。核心自建项为：多租户控制平面（L1-L3）、执行面托管外壳（L4）、Model Gateway（L6）、存储与治理（L7）。
