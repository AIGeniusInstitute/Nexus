# Nexus Agent Studio — 技术方案

> 对应 PRD：docs/prd/nexus-agent-studio/PRD.md
> 分支：feat/nexus-agent-studio

## 1. 架构定位

不改 codex 内核（`codex-rs/` 其他 crate 零改动），所有改造在 `codex-rs/nexus-control/` crate 内。通过 app-server JSON-RPC 集成（既有约束）。

```
用户 Studio CUI
   │ WS GET /v1/ws/threads/{id}/events
   ▼
ws.rs run() ── DB 回放 items + broadcast 实时推送
   ▲
   │ broadcast::channel (per-thread)
   │
http_server.rs turn_start drain
   │ 对每个 TurnEvent: 落 app_server_events + (item/* 落 items) + bcast.send(frame)
   ▲
   │ TurnEvent (含 delta)
runtime.rs driver_loop
   │ ServerNotification::try_from → map_notification → TurnEvent
   │ next_event() 读 codex app-server stdio
   ▲
stdio_client.rs AppServerProcess
   │ JSON-RPC over stdio
   ▼
codex app-server ── model_gateway.rs (Responses↔Chat SSE 转换) ── dashscope
```

## 2. 后端改造

### 2.1 migration `20260907000001_agent_studio.sql`

```sql
CREATE TABLE agent_definitions (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    name          TEXT NOT NULL,
    description   TEXT,
    system_prompt TEXT,
    model         TEXT,
    created_by    BIGINT REFERENCES users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_agent_defs_tenant ON agent_definitions(tenant_id);

ALTER TABLE threads ADD COLUMN agent_def_id BIGINT REFERENCES agent_definitions(id);
```

db.rs 注册 raw_sql。

### 2.2 `map_notification` 扩展（runtime.rs:833）

当前签名返回 `(Option<String>, Option<String>, Option<Usage>, bool)` = (codex_item_id, content_ref, usage, is_completed)。delta 事件复用此签名：codex_item_id = notification.item_id，content_ref = delta，其余 None/false。

新增 6 个 match 臂：

```rust
ServerNotification::AgentMessageDelta(p) =>
    (Some(p.item_id.clone()), Some(p.delta.clone()), None, false),
ServerNotification::ReasoningTextDelta(p) =>
    (Some(p.item_id.clone()), Some(p.delta.clone()), None, false),
ServerNotification::ReasoningSummaryTextDelta(p) =>
    (Some(p.item_id.clone()), Some(p.delta.clone()), None, false),
ServerNotification::CommandExecutionOutputDelta(p) =>
    (Some(p.item_id.clone()), Some(p.delta.clone()), None, false),
ServerNotification::PlanDelta(p) =>
    (Some(p.item_id.clone()), Some(p.delta.clone()), None, false),
ServerNotification::FileChangeOutputDelta(p) =>
    (Some(p.item_id.clone()), Some(p.delta.clone()), None, false),
```

但 `map_notification` 当前只返回 4 元组，driver_loop 用它构造 TurnEvent 时设 `item_type`。问题：delta 的 item_type 需要区分（`item/agentMessage/delta` 等），但当前 map_notification 不返回 method 字符串。

**方案**：扩展 map_notification 返回 5 元组 `(Option<String> /*item_id*/, Option<String> /*content*/, Option<Usage>, bool, String /*item_type*/)`. 当前 4 个 match 臂显式给 item_type（`item/started`、`item/completed`、`turn/completed`、`thread/tokenUsage/updated`），delta 臂给对应 method。`_` 分支给空 String。

driver_loop 构造 TurnEvent 时用返回的 item_type（取代当前硬编码）。

**driver_loop 真实 drain 段**（runtime.rs ~594-694）：当前 `map_notification(&n)` 返回 4 元组构造 TurnEvent。改为 5 元组，TurnEvent.item_type = 返回的第 5 项。

TurnEvent 结构不变（已有 item_type: String）。delta 事件 codex_item_id=Some(item_id)，content_ref=Some(delta)。

### 2.3 turn_start drain broadcast（http_server.rs:694）

当前常规 frame：`{thread_id, seq, type, content, item_id}`。delta 事件复用此 frame（type=item_type 如 `item/agentMessage/delta`，content=delta，item_id=item_id）。**无需 enrich**——frame 已够前端聚合。

但当前 drain 的 `if let Some(cid) = &ev.codex_item_id` 分支会把 delta 落 items 表（INSERT ON CONFLICT codex_item_id）——这会污染 items（delta 不该入 items）。

**方案**：drain 中区分 delta 事件：item_type 以 `/delta` 结尾的，**跳过 items INSERT**，只落 app_server_events + broadcast。

```rust
let is_delta = ev.item_type.ends_with("/delta");
if !is_delta {
    if let Some(cid) = &ev.codex_item_id {
        // 既有 items INSERT
    }
}
```

app_server_events 仍落（raw_json 全量）。broadcast 仍发。usage 累积不变（delta 无 usage）。

### 2.4 Agent 定义模块 `agent_defs.rs`（新）

```rust
pub struct AgentDef { id, tenant_id, name, description, system_prompt, model, created_by, created_at, updated_at }
pub struct CreateReq { name, description?, system_prompt?, model? }
pub struct UpdateReq { name?, description?, system_prompt?, model? }

create(pool, tid, uid, req) -> INSERT RETURNING
list(pool, tid) -> SELECT
get(pool, tid, id) -> SELECT WHERE tenant_id
update(pool, tid, id, req) -> COALESCE UPDATE
delete(pool, tid, id) -> DELETE
get_system_prompt(pool, tid, agent_def_id) -> Option<String>  // turn_start 用
```

http_server.rs 加 5 路由 + handler。thread create handler 扩展读 body.agent_def_id 写 threads.agent_def_id。

### 2.5 system_prompt 注入 driver_loop

`DriverCommand::RunTurn` 加字段 `system_prompt: Option<String>`。

turn_start handler：读 `SELECT agent_def_id FROM threads WHERE id=?` → 若有，查 `agent_definitions.system_prompt` → 传 RunTurn.system_prompt。

driver_loop 新建 thread 路径（codex_thread_id None）：
```rust
let mut params = ThreadStartParams::default();
if let Some(sp) = &cmd.system_prompt {
    params.base_instructions = Some(sp.clone());
}
match p.thread_start(params) { ... }
```

resume 路径不变（system_prompt 已固化在 thread 创建时）。

### 2.6 main.rs / lib.rs

- lib.rs 注册 `agent_defs` 模块。
- main.rs run_serve 注册路由、标题 "Nexus Agent Studio: serve"。
- db.rs 接线 migration。

## 3. 前端改造

### 3.1 依赖

`web/package.json` 加 `marked`（仅此一个，~30KB）。

### 3.2 `api.ts` 扩展

```ts
export interface AgentDef { id: number; name: string; description: string | null; system_prompt: string | null; model: string | null; created_at: string; }
export interface StreamFrame { thread_id: string; seq: number; type: string; content: string | null; item_id?: string; approval_id?: number; command?: string; }
export const api = {
  ...,
  listAgents: () => req<AgentDef[]>('/agents'),
  createAgent: (b) => req<AgentDef>('/agents', {method:'POST', body:JSON.stringify(b)}),
  getAgent: (id) => req<AgentDef>(`/agents/${id}`),
  updateAgent: (id, b) => req<AgentDef>(`/agents/${id}`, {method:'PUT', body:JSON.stringify(b)}),
  deleteAgent: (id) => req<void>(`/agents/${id}`, {method:'DELETE'}),
  createThreadWithAgent: (title?, agentDefId?) => req<Thread>('/threads', {method:'POST', body:JSON.stringify({title, agent_def_id: agentDefId})}),
};
```

`openThreadStream` 改为直接传 frame（已如此，Threads.tsx 的 `f.event==="item"` bug 不动——Studio 用新 handler）。

### 3.3 `pages/Studio.tsx`（新）

结构：
```
<Shell page="studio">
  <StudioPage>
    <div className="studio-layout">  // flex 两栏
      <StudioSidebar>  // 会话列表 + 新建（选 Agent）
      <StudioChat thread={active}>  // CUI 对话框
    </div>
```

`StudioChat` 状态机：
- `items: Map<item_id, StudioItem>` —— StudioItem = { itemId, kind, content(ThreadItem JSON parsed), deltas: Record<deltaType, string>, completed: bool }
- `order: string[]` —— item_id 渲染顺序
- `busy: bool` —— turn 进行中
- WS 连接：`openThreadStream(thread.id, onFrame)`，onFrame 按 type 分发：
  - `item/started` / `item/completed`：parse content JSON → 取 `type`(kind) → upsert items[item_id]
  - `item/*/delta`：items[item_id].deltas[type] += content
  - `approval/requested`：set pendingApproval
  - `turn/completed`：busy=false
- 提交：`api.startTurn(thread.id, input)` fire-and-forget（不 await，靠 WS）；setBusy(true)

渲染 assistant 气泡：遍历 items，按 kind 分发：
- `reasoning`：折叠面板（琥珀），text = deltas['item/reasoning/textDelta'] 或 content.content[]
- `agentMessage`：markdown 渲染 deltas['item/agentMessage/delta'] 或 content.text
- `commandExecution`：卡片（command + cwd + exit_code + output = deltas['item/commandExecution/outputDelta'] + content.aggregated_output）
- `mcpToolCall`：卡片（tool + arguments + result）
- `functionCallOutput`：卡片（name + output）
- `fileChange`：diff 视图（content.changes）
- `plan`：折叠面板（deltas['item/plan/delta'] 或 content.text）

user 消息：turn_start 提交时本地追加 user 气泡（input 文本）。

### 3.4 `ui.tsx` 扩展

加 `Markdown` 组件（marked 渲染 + sanitize）、`ReasoningBlock`（折叠琥珀）、`ToolCard`（工具卡片）、`DiffView`（文件变更）。

### 3.5 App.tsx 路由

加 `studio` case + sidebar 导航项「Agent Studio」。

## 4. 关键决策记录

| 决策 | 理由 |
|---|---|
| delta 不入 items 表 | items 幂等键是 codex_item_id（item 级），delta 同 item_id 多条会冲突；delta 是瞬态流式数据，落 app_server_events 已够审计 |
| frame 不 enrich | 既有 frame {seq,type,content,item_id} 够前端按 item_id 聚合 delta；approval frame 已有 approval_id/command |
| map_notification 加第 5 返回值（item_type） | 当前 driver_loop 硬编码 item_type，delta 需区分 method；5 元组比新增 TurnEvent 字段更 Surgical |
| fire-and-forget startTurn | turn_start HTTP 同步阻塞到 completed，await 会卡 UI；WS 是流式通道，deepthink 同模式 |
| 引入 marked | markdown 是 CUI 答案核心体验，手写 markdown 解析器易错且违反 Simplicity（marked 成熟稳定 30KB） |
| system_prompt 走 base_instructions | ThreadStartParams 既有字段，thread 级固化；resume 不重传（codex thread 状态保留 instructions） |
| Agent 定义不做版本/分享/编排 | Simplicity First：P1 聚焦 CUI+流式；Nexus 已有 M17 Skills + M18 编排，不重建 |

## 5. 改动文件清单

### 后端（nexus-control/src/）
- `migrations/20260907000001_agent_studio.sql`（新）
- `runtime.rs`：map_notification 扩展 + DriverCommand::RunTurn 加 system_prompt + driver_loop thread_start 注入
- `http_server.rs`：turn_start drain delta 跳过 items + 读 agent_def_id 传 system_prompt + 5 Agent 路由
- `agent_defs.rs`（新）
- `db.rs`：注册 migration
- `lib.rs`：注册 agent_defs 模块
- `main.rs`：标题 + 路由

### 前端（web/src/）
- `package.json`：加 marked
- `api.ts`：Agent 接口 + Studio frame 类型
- `ui.tsx`：Markdown / ReasoningBlock / ToolCard / DiffView
- `pages/Studio.tsx`（新）
- `App.tsx`：studio 路由 + sidebar

## 6. 验证计划

1. `cargo check` 0 error 0 warning
2. `cargo test` 既有 32 + 新增 map_notification delta 单测全过
3. e2e SIMULATE：零回归（approval + 计量 + pool）
4. e2e 真实模型（dashscope）：Studio 提交 turn → WS 收到 reasoning delta + agentMessage delta + item/completed + 工具卡片
5. Web tsc + vite build
6. 测试报告 HTML
