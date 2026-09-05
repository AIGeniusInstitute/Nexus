# Nexus M3 · 审批与策略 — 技术方案

> 前置：M2 执行闭环（merge 4b5b7b6）。M2 driver 线程独占 AppServerProcess，async 侧经 `std::sync::mpsc::Sender`（cmd）+ `tokio::sync::mpsc::UnboundedReceiver`（event）通信。`handle_server_request` 对 `CommandExecutionRequestApproval`/`FileChangeRequestApproval` 直接 auto-accept。

## 1. 核心问题：HITL 审批的同步阻塞 + 死锁

app-server 审批是 **JSON-RPC 请求-响应**：server 发 `Request{id, method, params}`，client 必须回写 `Response{id, result:{decision}}`。agent 在等待期间不发新通知（gating）。

**死锁风险**（若 ResolveApproval 经 runtime mutex 发送）：
- `turn_start` 持 runtime mutex → 阻塞 `event_rx.recv().await` 等 turn/completed
- driver 挂起在 `cmd_rx.recv()` 等 ResolveApproval
- resolve handler 要发 ResolveApproval → 锁 runtime mutex → turn_start 持锁不放 → **死锁**

## 2. 解决方案：拆分 RuntimeHandle

```
M2:  AppState { runtime: Arc<Mutex<RuntimeHandle{cmd_tx, event_rx}>> }
M3:  AppState { runtime_cmd: Sender<DriverCommand>,            // Clone, 无锁
                runtime_events: Arc<Mutex<UnboundedReceiver<TurnEvent>>>, } // 仅 turn_start 读
```

- `cmd_tx`（`std::sync::mpsc::Sender`）是 `Clone`，放 AppState 顶层，**无锁直接 `send`**。
- `event_rx`（`UnboundedReceiver`）不 Clone，放 `Arc<Mutex<>>`，仅 `turn_start` 持锁 drain。
- resolve / interrupt handler：`st.runtime_cmd.send(...)` — **不经锁**，不死锁。
- driver 单线程串行：turn_start 持 event_rx 锁整 turn → 第二个 turn_start 阻塞在锁 = 串行（同 M2 语义）。

## 3. Driver 改造（runtime.rs + stdio_client.rs）

### 3.1 stdio_client：surface 审批请求

新增 `StreamEvent` 枚举，`next_event()` 替代 `next_notification()` 在 driver 中的角色：

```rust
pub enum StreamEvent {
    Notification(JSONRPCNotification),
    ApprovalRequest(ApprovalRequest),
}
pub struct ApprovalRequest {
    pub jsonrpc_id: serde_json::Value,   // 原始 request id（回写要用）
    pub kind: ApprovalKind,              // Command | FileChange
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub reason: Option<String>,
    pub raw_params: serde_json::Value,   // 完整 params（脱敏展示用）
}
pub enum ApprovalKind { CommandExecution, FileChange }
```

`next_event(&mut self) -> Result<StreamEvent>`：复用 `read_jsonrpc_message` 循环；遇 `Notification` → 返回；遇 `Request` → `ServerRequest::try_from` 分类：审批请求 → 返回 `ApprovalRequest`，其他 → `handle_server_request`（auto，保 M2 行为）+ 继续。

新增 `respond_approval(&mut self, id: Value, kind: ApprovalKind, decision: DecisionInput) -> Result<()>`：构造 `JSONRPCResponse{id, result: to_value(response)}` 回写。DecisionInput 是 Nexus 侧的简化枚举 `{Approve, Deny, Cancel}` → 映射到协议枚举（Approve→Accept，Deny→Decline，Cancel→Cancel）。

### 3.2 DriverCommand 扩展

```rust
pub enum DriverCommand {
    RunTurn { ... },            // M2
    Interrupt,                  // M2
    Shutdown,                   // M2
    ResolveApproval {           // M3 新增
        approval_id: Uuid,      // Nexus 侧 ticket id（driver 用它匹配 parked 上下文）
        decision: DecisionInput,
    },
}
```

### 3.3 driver_loop：挂起-回写

```rust
// drain 循环内
match p.next_event()? {
    StreamEvent::Notification(n) => { /* M2 map_notification + send event */ }
    StreamEvent::ApprovalRequest(ar) => {
        let approval_id = Uuid::new_v4();
        // emit approval/requested TurnEvent
        event_tx.send(TurnEvent {
            item_type: "approval/requested", codex_item_id: Some(ar.item_id),
            content_ref: Some(serde_json::to_string(&ar)...),
            approval: Some(ApprovalInfo { approval_id, jsonrpc_id: ar.jsonrpc_id.clone(),
                                           kind: ar.kind.clone(), ... }),
            ..default
        });
        // 记 parked 上下文
        let parked = ParkedApproval { approval_id, jsonrpc_id: ar.jsonrpc_id, kind: ar.kind };
        // 挂起：阻塞 cmd_rx 等决策或中断
        loop {
            match cmd_rx.recv() {
                Ok(ResolveApproval{ approval_id, decision }) if approval_id == parked.approval_id => {
                    p.respond_approval(parked.jsonrpc_id, parked.kind, decision)?;
                    break; // 回到 notification drain
                }
                Ok(Interrupt) => {
                    // 回写 Cancel，kill，emit turn-aborted，退出 turn
                    let _ = p.respond_approval(parked.jsonrpc_id, parked.kind, DecisionInput::Cancel);
                    emit turn-aborted(is_turn_completed=true);
                    break 'turn_drain;
                }
                Ok(Shutdown) => { p.kill(); return; }
                Ok(other) => { /* RunTurn while parked = 协议错误，emit error */ }
                Err(_) => break 'turn_drain,
            }
        }
    }
}
```

**单 approval 串行**：driver 同一时间最多一个 `parked`（turn 串行 + app-server gating 保证）。

## 4. ApprovalTicket 数据模型（T3-1）

```sql
-- migrations/20260906000003_m3_approval.sql
CREATE TABLE IF NOT EXISTS approval_tickets (
    id            UUID PRIMARY KEY,
    thread_id     UUID NOT NULL REFERENCES threads(id),
    turn_id       BIGINT NOT NULL REFERENCES turns(id),
    tenant_id     BIGINT NOT NULL,
    kind          TEXT NOT NULL,            -- command_execution | file_change
    status        TEXT NOT NULL,            -- pending | approved | denied | cancelled | interrupted
    item_id       TEXT,                     -- codex ThreadItem.id
    jsonrpc_id    JSONB NOT NULL,           -- 原始 request id（回写用，存 JSON 值）
    command       TEXT,
    cwd           TEXT,
    reason        TEXT,
    raw_params    JSONB,
    decided_by    BIGINT REFERENCES users(id),
    decided_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_approval_thread ON approval_tickets(thread_id);
CREATE INDEX IF NOT EXISTS idx_approval_status ON approval_tickets(status) WHERE status='pending';

CREATE TABLE IF NOT EXISTS approval_audit (
    id            BIGSERIAL PRIMARY KEY,
    approval_id   UUID NOT NULL REFERENCES approval_tickets(id),
    actor_user_id BIGINT NOT NULL,
    action        TEXT NOT NULL,            -- created | resolved | revoked_deny | interrupted
    decision      TEXT,
    params_digest TEXT,                     -- 命令摘要（脱敏）
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

生命周期：`pending` →（resolve）→ `approved`/`denied`；`pending` →（interrupt）→ `interrupted`；`pending` →（权限撤销）→ `denied`。每次状态变更加 audit 行。

## 5. HTTP API（http_server.rs）

| Method | Path | 说明 |
|---|---|---|
| POST | `/v1/threads/{id}/turns` | turn_start（M2 改造：审批事件处理） |
| POST | `/v1/threads/{id}/turns/{tid}/interrupt` | interrupt（M2 改造：无锁 cmd_tx） |
| POST | `/v1/approvals/{aid}/resolve` | **M3 新增**：body `{decision: approve|deny|cancel}` → UPDATE ticket + `runtime_cmd.send(ResolveApproval)` |
| GET  | `/v1/approvals` | **M3 新增**：列 pending（按 tenant） |
| GET  | `/v1/threads/{id}/approvals` | **M3 新增**：按 thread 列 |

### turn_start 改造

drain 循环内增加 approval/requested 分支：
```rust
while let Some(ev) = rx.recv().await {
    if ev.item_type == "approval/requested" {
        // 落库 pending ticket（先落库再回写——R3 风险）
        INSERT approval_tickets(...);
        INSERT approval_audit(action='created');
        // 广播 WS
        bcast.send(approval/requested frame);
        // 继续 rx.recv().await 等 driver 回写后的后续事件
        // （driver 此刻 parked 在 cmd_rx，无新事件；turn_start 阻塞在此 = 等 human）
        continue;
    }
    // 常规 item 事件处理（M2）
    ...
    if ev.is_turn_completed { break; }
}
```

resolve handler：
```rust
async fn approval_resolve(c, st, Path(aid), Json{decision}) {
    // 校验：ticket 属本 tenant + status=pending
    // UPDATE ticket decided + audit
    // 无锁发 ResolveApproval
    st.runtime_cmd.send(ResolveApproval{ approval_id: aid, decision })?;
    Ok(json)
}
```

## 6. 策略中心（T3-3）

### 6.1 数据模型

```sql
CREATE TABLE IF NOT EXISTS policies (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   BIGINT NOT NULL,
    role        TEXT NOT NULL,         -- admin | developer | viewer | *
    action_kind TEXT NOT NULL,         -- command_execution | file_change | *
    pattern     TEXT NOT NULL,         -- 命令/路径 glob，如 rm -rf* / *
    risk_level  TEXT NOT NULL,         -- low | medium | high
    decision    TEXT NOT NULL,         -- allow | prompt | deny
    priority    INT NOT NULL DEFAULT 0,
    enabled     BOOLEAN NOT NULL DEFAULT TRUE
);
-- seed 默认策略（最小集）：
-- * / rm*            high   deny
-- * / sudo*          high   deny
-- * / *:read         low    allow
-- developer / file_change / *  medium prompt
```

### 6.2 求值器 `policy.rs`

`evaluate(tenant, role, kind, command) -> PolicyDecision { Allow, Prompt, Deny }`：按 priority desc 匹配第一条 pattern；default Prompt（fail-open 到需审批）。

### 6.3 生成器：准入求值 + 下发

turn_start 前：
1. 查 tenant 的 policies → 求值该 turn 的"准入"（角色权限是否够开 turn）。
2. 生成 `.rules`（Starlark execpolicy）：deny 列表 → `forbid`，allow 列表 → `allow`，prompt → 不写 rules（交给 HITL）。复用 M0 `execpolicy_rules.rs` 的写入路径 `<CODEX_HOME>/rules/`。
3. 生成 `config.toml`：model 指向 gateway、sandbox 设 ReadOnly、approval_policy=OnRequest（让 agent 真正请求审批）。

MVP 不做 per-turn 动态 rules 重写（开销大）；做 per-tenant 启动时生成一次，turn 用 OnRequest 让具体命令走 HITL。策略表的 deny 规则同时写进 .rules（双保险：协议级 + 运行时）。

## 7. Web 审批抽屉（T3-5）

React 页 `ApprovalsPage.tsx`（M1 Web 门户已存在）：
- `GET /v1/approvals` 列 pending → 表格（命令/cwd/风险/时间）。
- 行内 批准/拒绝 按钮 → `POST /v1/approvals/{id}/resolve`。
- WS 订阅 thread events → 新 approval/requested 实时插入列表。
- 参数脱敏：command 全显（审计需要），env 变量值打码。

## 8. 测试策略

**核心难点**：mock gateway 不触发真实审批（agent 不执行命令）。两种验证路径：

1. **合成注入测试模式**（`NEXUS_SIMULATE_APPROVAL=1`）：driver 在 turn_start 后、drain 前，合成一个 `ApprovalRequest`（假 jsonrpc_id + 假 command）→ 走完整 HITL 桥（落库/广播/resolve/回写）。验证桥本身正确，无真实模型依赖。
2. **真实 upstream 联调**（可选，需 `NEXUS_UPSTREAM_MODEL_URL`）：配 `AskForApproval::OnRequest`，真实 agent 执行命令 → 真实审批请求 → 验证端到端。

MVP e2e 用路径 1（确定性、无外部依赖）。

### 测试用例
- AC3.1：SIMULATE 模式 turn → ticket pending 落库 + WS 广播 approval/requested
- AC3.2：resolve(approve) → ticket approved + driver 回写 + turn completed
- AC3.3：interrupt → ticket interrupted + turn interrupted
- AC3.4：权限撤销 → pending ticket auto-deny + WS close
- AC3.5：策略 rm* → deny；ls → allow；.rules 生成正确
- AC3.6：Web 抽屉 GET/POST 流程
- AC3.7：audit 行齐全
- 回归：M2 的 14 单测 + 10 e2e 不破

## 9. §7 自审

| 维度 | 自审结论 |
|---|---|
| 死锁 | ✅ 拆分 cmd_tx/event_rx，resolve 无锁；driver parked 在 cmd_rx，turn_start 阻塞 event_rx.recv，两者无环形等待 |
| 串行 | ✅ 单 driver + event_rx 锁 = 单 turn 单 approval 串行，与 M2 语义一致 |
| 回写正确性 | ✅ jsonrpc_id 存原始 JSON Value，回写 `JSONRPCResponse{id, result}` id 精确匹配 |
| 协议不改 | ✅ 全在 nexus-control，不动 codex-rs 内核 |
| Simplicity | ✅ 不做崩溃恢复/批量/改参/amendment；SIMULATE 测试模式隔离真实模型依赖 |
| 回归 | ✅ event 通道/落库/WS 复用 M2；next_event 是 next_notification 的超集 |
| 风险 R3 | ✅ "先落库再回写"——approval/requested 到达先 INSERT pending 再广播，Pod 崩溃后 pending ticket 可查（回放留后续，但状态不丢） |

**结论：方案 OK，开工。**
