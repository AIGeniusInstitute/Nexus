# Nexus Agent Studio — PRD

> 里程碑：Agent Studio（CUI 对话框 + 流式输出）
> 分支：`feat/nexus-agent-studio`
> 参考：~/deepthink Agent Studio + ChatView 模块

## 1. 背景与目标

### 1.1 背景

Nexus 已交付 M0–M22 全部里程碑：控制面（threads/turns/items/审批/计量/策略/审计/timeline/评测/KB/fork/pool/连接器/skills/编排/eval-center/judge/事件溯源）、Web 控制台（13 页企业控制台 + Docker 一键部署）、真实模型联调（M8 gateway Responses↔Chat SSE 转换 + M9 function calling 双向）。

但当前 Nexus 的「对话」体验停留在 **Threads 页面的简陋事件流**（纯文本罗列 items 的 content_ref，无结构化渲染、无流式增量、无思考过程展示）。真实模型流式能力（gateway 已转 reasoning_text.delta / output_text.delta）被 codex 内部消费后产出 ThreadItem，但 codex 额外发出的 **delta notification（AgentMessageDelta / ReasoningTextDelta / CommandExecutionOutputDelta 等）在 `runtime.rs:833 map_notification` 的 `_` 分支被全部丢弃**——不入 items、不 broadcast。这是 CUI 流式展示的根本障碍。

deepthink 的 Agent Studio 模块证明了一套完整的 Agent 对话体验：Agent 定义管理（system_prompt/model/挂载）+ CUI 对话框（WS 流式 + 区块化渲染：思考折叠 / 工具卡片 / 答案流式 markdown / 产物渲染）。

### 1.2 目标

在 Nexus 实现 **Agent Studio 模块**，提供企业级 CUI 对话框界面，用户可以看到 Agent 执行的**完整流式输出**：

- **思考过程**（reasoning）— 流式增量，折叠面板
- **工具调用**（CommandExecution / McpToolCall / FunctionCallOutput）— 卡片化，参数 + 输出
- **Skill 调用** — 复用工具调用卡片
- **答案输出**（AgentMessage）— 流式 markdown 打字效果
- **任务产物**（FileChange / 代码块）— 结构化渲染

### 1.3 非目标（P2，明确不在本次）

- Agent 版本历史 / diff 对比 / 分享链接 / 协作者（deepthink Agent Studio PaaS 全套）
- AI 生成 / AI 优化 Agent 配置
- 编排者-Worker Agent 协作（Nexus 已有 M18 Orchestration，复用不重建）
- 产物渲染器全套（14 种 artifact renderer：Pdf/Docx/Xlsx/Pptx 等）—— P1 只做代码块 + FileChange diff
- Skills 挂载关联到 Agent 定义（Nexus 已有 M17 Skills 市场，P1 不做挂载关联表）
- trace 双层 DAG 回放（Nexus 已有 M11 timeline，P1 不做 Trace 回放器）
- 终端交互 / xterm 集成

## 2. 范围决策

### 2.1 为什么不照搬 deepthink 全套

deepthink 的 `chat.ts` store 3429 行 + `StreamingDisplay.tsx` 970 行 + `stream-event.types.ts` 40+ 事件类型，依赖 Zustand + markdown-it + mermaid + monaco + xterm + react-virtual。Nexus 前端是**手写零依赖**风格（ui.tsx 自写组件库，hash 路由，无状态管理库）。照搬会破坏 Nexus 的简洁性、引入大量重依赖，违反 **Simplicity First**。

Nexus 的 codex 引擎与 deepthink 的 agent-runner 是不同的执行模型：Nexus 通过 app-server JSON-RPC（`stdio_client.rs`），delta notification 已有标准协议（`item/agentMessage/delta` 等，带 `item_id` + `delta` 字段），不需要 deepthink 的 `stream-processor.ts` 自定义转换层。Nexus 只需在 `map_notification` 转发 delta + 前端按 `item_id` 聚合即可，比 deepthink 的方案更直接。

### 2.2 P1 范围（本次交付）

| 模块 | 内容 |
|---|---|
| 后端·delta 转发 | `map_notification` 扩展，6 种 delta notification 产出 TurnEvent + broadcast |
| 后端·Agent 定义 | `agent_definitions` 表 + CRUD + thread 绑定 + `base_instructions` 注入 |
| 后端·broadcast frame | frame enrich（delta 带 item_id + delta 文本，前端按 item_id 聚合） |
| 前端·Studio CUI | 新 `Studio.tsx` 页面：消息气泡 + 流式区块 + WS item_id 聚合 |
| 前端·markdown | 引入 `marked` 轻量库渲染 AgentMessage |
| 前端·工具卡片 | CommandExecution / McpToolCall / FunctionCallOutput / FileChange 结构化渲染 |

## 3. 功能需求与验收标准

### FR-1：delta notification 流式转发

**需求**：`runtime.rs map_notification` 扩展，处理以下 6 种 delta notification，每种产出 TurnEvent（携带 `codex_item_id` = notification 的 `item_id`，`content_ref` = `delta` 文本，`item_type` = notification method），经 driver → http_server drain → broadcast 到 WS。

| codex notification | item_type | content_ref | 用途 |
|---|---|---|---|
| `AgentMessageDeltaNotification` | `item/agentMessage/delta` | delta | 答案文本增量 |
| `ReasoningTextDeltaNotification` | `item/reasoning/textDelta` | delta | 思考全文增量 |
| `ReasoningSummaryTextDeltaNotification` | `item/reasoning/summaryTextDelta` | delta | 思考摘要增量 |
| `CommandExecutionOutputDeltaNotification` | `item/commandExecution/outputDelta` | delta | 命令输出增量 |
| `PlanDeltaNotification` | `item/plan/delta` | delta | 计划增量 |
| `FileChangeOutputDeltaNotification` | `item/fileChange/outputDelta` | delta | 文件变更输出增量 |

**约束**：
- delta 事件 **不入 `items` 表**（只有 `item/started` + `item/completed` 入 items，保留既有幂等键语义），但落 `app_server_events`（已有 raw_json 落库，零改动）。
- delta 事件 **broadcast**（frame type=item_type, content=delta, item_id=item_id）。
- delta 事件 `is_turn_completed=false`、`usage=None`、`codex_thread_id=None`。

**AC-1.1**：真实模型 turn 执行期间，WS 订阅者收到 `item/reasoning/textDelta` frame（content=非空 delta 文本，item_id=非空）。
**AC-1.2**：真实模型 turn 执行期间，WS 订阅者收到 `item/agentMessage/delta` frame。
**AC-1.3**：`item/started` + `item/completed` 仍正常落 items 表 + broadcast（零回归）。
**AC-1.4**：delta 事件不入 items 表（`SELECT count(*) FROM items WHERE item_type LIKE '%/delta'` = 0）。
**AC-1.5**：`cargo test` 零回归（既有 32 测试全过 + 新增 map_notification delta 单测）。

### FR-2：Agent 定义 CRUD + system_prompt 注入

**需求**：
- 新表 `agent_definitions(id, tenant_id, name, description, system_prompt, model, created_by, created_at, updated_at)`。
- `threads` 表 ALTER ADD `agent_def_id BIGINT REFERENCES agent_definitions(id)`。
- API：`POST /v1/agents`（创建）、`GET /v1/agents`（列表）、`GET /v1/agents/{id}`、`PUT /v1/agents/{id}`、`DELETE /v1/agents/{id}`。
- `POST /v1/threads` 支持 body `agent_def_id`（可选），写入 threads.agent_def_id。
- `turn_start` handler：读 `threads.agent_def_id` → 查 `agent_definitions.system_prompt` → 通过 `DriverCommand::RunTurn` 新增字段 `system_prompt: Option<String>` 传给 driver。
- `driver_loop` 新建 thread 时（codex_thread_id 为 None）：`ThreadStartParams { base_instructions: system_prompt, ..default() }`；resume 时忽略（system_prompt 是 thread 级，已在创建时固化）。

**约束**：
- Agent 定义 tenant-scoped（本租户 CRUD，admin `*:*` 跨租户只读不在此需求）。
- `system_prompt` 为空时退化为 `ThreadStartParams::default()`（零回归，既有 thread 行为不变）。
- `model` 字段暂存但不覆盖 `NEXUS_MODEL` env（model 路由是 P2，P1 仍用 env 模型）。

**AC-2.1**：`POST /v1/agents {name, system_prompt}` 返回 id，`GET /v1/agents` 列表含该项。
**AC-2.2**：`POST /v1/threads {agent_def_id}` 创建 thread，`threads.agent_def_id` 落库。
**AC-2.3**：绑定 Agent 定义的 thread 提交 turn，codex app-server 收到 `thread/start` 的 `baseInstructions` = system_prompt（通过 app_server_events 或 codex 行为验证）。
**AC-2.4**：未绑定 Agent 定义的 thread（agent_def_id NULL）行为零回归（SIMULATE turn completed）。

### FR-3：Studio CUI 对话框页面

**需求**：新前端页面 `Studio.tsx`，hash 路由 `#/studio`。布局：
- 左侧：会话列表（复用 thread CRUD）+ 「新建会话」按钮（可选 Agent 定义）。
- 右侧：选中会话的 CUI 对话框。

**CUI 对话框结构**：
- 顶部：Agent 定义选择器（下拉，可选；切换不影响已有 thread）。
- 消息区：滚动列表，user 消息气泡（右对齐）+ assistant 流式气泡（左对齐）。
- assistant 气泡内部按 `item_id` 聚合，区块化渲染：
  - **思考过程**：折叠面板（默认折叠，琥珀色边框），`item/reasoning/textDelta` + `item/reasoning/summaryTextDelta` 增量累积；`item/started` Reasoning 给出完整结构。
  - **工具调用卡片**：`item/started`/`item/completed` 的 ThreadItem kind=`commandExecution` → 卡片（命令 + cwd + exit_code + aggregated_output）；`item/commandExecution/outputDelta` 增量累积到输出区。kind=`mcpToolCall` → 卡片（tool + arguments + result）。kind=`functionCallOutput` → 卡片（name + output）。
  - **计划**：kind=`plan` 或 `item/plan/delta` 增量，折叠面板。
  - **答案**：kind=`agentMessage` 的 text + `item/agentMessage/delta` 增量累积 → markdown 流式渲染。
  - **文件变更**：kind=`fileChange` → diff 视图（changes 列表）。
- 输入区：文本框 + 发送按钮（回车提交），提交时 `startTurn` fire-and-forget（不 await，靠 WS 渲染增量）。

**WS 帧处理状态机**（前端）：
- 连接 `GET /v1/ws/threads/{id}/events`。
- frame `{thread_id, seq, type, content, item_id, approval_id?, command?}`：
  - `type=item/started` 或 `item/completed`：parse content（ThreadItem JSON），按 `item_id` upsert 到 items Map，kind 分发渲染。
  - `type=item/*/delta`：按 `item_id` 累积 delta 到对应 item 的 delta buffer。
  - `type=approval/requested`：渲染审批卡片（批准/拒绝按钮）。
  - `type=turn/completed`：标记当前 turn 结束。
- 连接时 DB 回放（既有 ws.rs 行为）补齐历史 items。

**AC-3.1**：`#/studio` 页面加载，左侧会话列表显示，右侧空状态。
**AC-3.2**：新建会话（选 Agent 定义）→ 右侧出现 CUI 对话框。
**AC-3.3**：提交消息 → user 气泡出现 → assistant 流式气泡开始渲染（思考折叠 + 答案流式打字）。
**AC-3.4**：真实模型 turn 期间，思考过程区显示流式文本，答案区流式 markdown。
**AC-3.5**：工具调用卡片显示命令/参数/输出（CommandExecution/McpToolCall）。
**AC-3.6**：审批触发时显示审批卡片，批准后 turn 继续。
**AC-3.7**：turn 结束后流式气泡转为持久化消息（item/completed 结构替代 delta 累积）。
**AC-3.8**：刷新页面，历史 items 从 DB 回放正确渲染。

### FR-4：markdown 渲染

**需求**：AgentMessage 答案用 markdown 渲染。引入 `marked`（~30KB，零依赖，成熟）。

**约束**：仅渲染 markdown 到 sanitized HTML（`marked` + `DOMPurify` 或 marked 内置 sanitize）。代码块带语法高亮可选（P1 不做高亮，纯 `<pre><code>`）。

**AC-4.1**：AgentMessage 含代码块的 markdown 正确渲染为 `<pre><code>` 块。
**AC-4.2**：流式 delta 期间 markdown 实时更新（每 delta 重渲染或节流）。

## 4. 数据模型

### 4.1 新表 `agent_definitions`

```sql
CREATE TABLE agent_definitions (
    id           BIGSERIAL PRIMARY KEY,
    tenant_id    BIGINT NOT NULL REFERENCES tenants(id),
    name         TEXT NOT NULL,
    description  TEXT,
    system_prompt TEXT,
    model        TEXT,
    created_by   BIGINT REFERENCES users(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_agent_defs_tenant ON agent_definitions(tenant_id);
```

### 4.2 threads 扩展

```sql
ALTER TABLE threads ADD COLUMN agent_def_id BIGINT REFERENCES agent_definitions(id);
```

### 4.3 既有表复用（零改动）

- `items`：item/started + item/completed 落库（content_ref = ThreadItem JSON）。
- `app_server_events`：全量 raw notification JSON 落库（含 delta）。
- `turns` / `approval_tickets` / `usage_records`：既有。

## 5. API 一览

| Method | Path | 用途 |
|---|---|---|
| POST | `/v1/agents` | 创建 Agent 定义 |
| GET | `/v1/agents` | 列表（本租户） |
| GET | `/v1/agents/{id}` | 详情 |
| PUT | `/v1/agents/{id}` | 更新 |
| DELETE | `/v1/agents/{id}` | 删除 |
| POST | `/v1/threads` | 创建会话（body 可选 `agent_def_id`，既有端点扩展） |
| GET | `/v1/ws/threads/{id}/events` | WS 流式订阅（既有，frame enrich） |

## 6. 测试用例

| ID | 用例 | 验证 |
|---|---|---|
| TC-1 | map_notification delta 单测 | 6 种 delta notification → TurnEvent item_type + content_ref=delta + codex_item_id=item_id |
| TC-2 | Agent 定义 CRUD | POST 创建 + GET 列表 + PUT 更新 + DELETE |
| TC-3 | thread 绑定 Agent | POST thread {agent_def_id} → threads.agent_def_id 落库 |
| TC-4 | system_prompt 注入 | 绑定 Agent 的 thread turn_start → codex thread/start baseInstructions = system_prompt |
| TC-5 | 零回归 SIMULATE | 既有 SIMULATE turn + approval + 计量不退化 |
| TC-6 | 真实模型流式 | dashscope 真实 turn → WS 收到 reasoning delta + agentMessage delta + item/completed |
| TC-7 | Studio CUI 渲染 | 真实 turn → 思考折叠 + 答案流式 + 工具卡片 |
| TC-8 | 历史回放 | 刷新页面 → DB items 回放渲染 |
| TC-9 | 审批卡片 | approval/requested → 卡片 → 批准 → turn 继续 |
| TC-10 | cargo test 零回归 | 既有 32 测试 + 新增全过 |

## 7. 风险与依赖

| 风险 | 缓解 |
|---|---|
| map_notification 改动影响 turn_start drain | Surgical：只加 match 臂，不动 drain 框架；delta 不入 items 表避免幂等键冲突 |
| 真实模型联调需 dashscope key | 复用 M8-M9 已验证的 NEXUS_UPSTREAM_MODEL_URL + NEXUS_MODEL_KEY |
| 前端 marked 引入破坏零依赖 | marked 是单文件零依赖库，build 体积可控（~30KB）；可接受 |
| driver_loop thread_start 改动 | 只在新建 thread 路径加 base_instructions，resume 路径不动 |
| turn_start 同步阻塞 vs 流式 | 不改 turn_start HTTP 语义（仍同步到 completed），前端 fire-and-forget + WS 流式（deepthink 模式） |

## 8. 里程碑进度

- [ ] PRD（本文档）
- [ ] 技术方案
- [ ] 编码：后端 delta 转发 + Agent 定义 + broadcast enrich
- [ ] 编码：前端 Studio CUI 页面
- [ ] 测试：cargo test + e2e（SIMULATE + 真实模型）
- [ ] 测试报告 HTML
- [ ] 合并 main + push
