# Nexus M2 — 技术方案：执行闭环

> 分支：`feat/nexus-m2` · 前置 M1（merge 034fad0）
> 日期：2026-09-06 · 状态：自主审查通过（§7）

## 1. 现状与问题

M1 之后：
- `http_server.rs::turn_start`（L139-158）建 turn 行后**写一条 mock system item**，从不驱动 app-server → 任务不能跑。
- `ws.rs::run`（L37-67）每 1s 轮询 `items` 表推 frame——无新 item 时空转、有新 item 时最多 1s 延迟。
- M0 的 `stdio_client.rs`（AppServerProcess）和 `model_gateway.rs` 是 PoC 级独立 CLI（`nexus-control poc`），**未接入 M1 的 async serve 路径**。
- app-server 是**行阻塞 stdio**：`next_notification()` 阻塞在 `BufReader::read_line`；单进程一次只能服务一个 turn。

核心矛盾：M1 是 tokio async，AppServerProcess 是 sync blocking——直接 `block_on` 会卡死 runtime。

## 2. 架构：专用驱动线程 + channel 桥

```
 axum handler (async)                      驱动线程 (std::thread, 阻塞 stdio)
 ┌─────────────────────┐                  ┌───────────────────────────┐
 │ POST /turns         │  cmd: RunTurn    │  loop {                    │
 │  create turn row    │ ───────────────▶ │   recv cmd                 │
 │  runtime.dispatch() │                  │   match cmd:               │
 │  while let Some(ev) │ ◀─────────────── │     Initialize → init      │
 │   = event_rx.recv() │  event: Notif    │     StartThread → t/start  │
 │   persist → PG items │                  │     RunTurn → turn/start   │
 │   broadcast → WS     │                  │       drain next_notif     │
 │  update turn done   │                  │       emit each → tx       │
 └─────────────────────┘                  │     until turn/completed    │
        │ broadcast                         │     Interrupt → kill+respawn│
        ▼                                  │  }                         │
 ┌─────────────────────┐                  └───────────────────────────┘
 │ ws.rs run()         │  ◀── broadcast channel (per-thread, 多订阅者)
 │  subscribe + poll   │
 │  push item frames    │
 └─────────────────────┘
```

设计要点：
1. **驱动线程独占 AppServerProcess**：阻塞 I/O 全在该线程，async runtime 永不阻塞。
2. **命令串行**：cmd_rx 是 mpsc，天然单进单出——无需 Mutex，无并发 stdio 问题。全局单 turn 串行（M2 接受）。
3. **事件回流**：event_tx 把 `ServerNotification` 发回 async 侧；turn_start handler 循环 `recv()`，每条落库 + broadcast，遇 `TurnCompleted` 结束。
4. **interrupt = kill+respawn+resume**：async 侧发 `Interrupt` 命令；驱动线程 `proc.kill()`，下次命令前 respawn + `thread/resume`（codex_thread_id 已落库）。M0 验证过的最简路径。

## 3. 模块设计

### 3.1 `src/runtime.rs`（新增）— RuntimeDriver

```rust
pub enum DriverCommand {
    Initialize,
    StartThread { thread_id: Uuid },          // thread/start 或 resume（按 codex_thread_id）
    RunTurn { turn_db_id: i64, thread_id: Uuid, input: String },
    Interrupt { thread_id: Uuid },
    Shutdown,
}

pub struct TurnEvent {
    pub thread_id: Uuid,
    pub turn_id: i64,           // app-server turn id (str) — 存 items.turn_id 用 DB id
    pub seq: i64,
    pub item_type: String,      // "item/started" | "item/completed" | "turn/completed" ...
    pub content_ref: Option<String>,
    pub raw_json: serde_json::Value,   // 落 app_server_events.event_json
    pub usage: Option<Usage>,          // tokenUsage/updated 时填
}

pub struct Usage { input_tokens: i64, output_tokens: i64, cost_micros: i64, model: Option<String> }

pub struct RuntimeDriver {
    cmd_tx: std::sync::mpsc::Sender<DriverCommand>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<TurnEvent>,  // async 侧消费
    // 驱动线程 JoinHandle（drop 时发 Shutdown）
}
```

驱动线程主体（blocking）：
```rust
fn driver_loop(codex_bin, codex_home, gateway_cfg, cmd_rx, event_tx) {
    let mut proc: Option<AppServerProcess> = None;
    let mut codex_thread_id: Option<String> = None;  // 本进程当前 thread
    for cmd in cmd_rx.iter() {
        match cmd {
            Initialize => { spawn+initialize; proc=Some(...) }
            StartThread { thread_id } => {
                // 若 proc 有该 thread 的 codex_thread_id → resume；否则 start
                // codex_thread_id 由 async 侧从 PG 读出传入（见下）
            }
            RunTurn { turn_db_id, thread_id, input } => {
                ensure proc + thread; turn_start(...);
                loop {
                    let n = proc.next_notification()?;   // 阻塞读
                    let ev = map_notification(n, thread_id, turn_db_id);
                    let is_completed = matches!(ev.item_type.as_str(), "turn/completed");
                    event_tx.send(ev);                    // 回 async
                    if is_completed { break; }
                }
            }
            Interrupt {..} => { proc.kill(); proc=None; }  // 下次 RunTurn respawn+resume
            Shutdown => break,
        }
    }
}
```

> **注意**：app-server `turn/start` 立即返回 `TurnStartResponse{turn_id}`，随后**异步流式**发通知。所以 `next_notification` 的 drain 循环在 `turn/completed` 之前会持续返回 item/* 通知。`item/started` 与 `item/completed` 用同一 seq（同一 item 的开始与完成），落库时 `ON CONFLICT (thread_id,turn_id,seq) DO UPDATE` 更新 content_ref/digest——天然幂等（resume 重放不重复）。

### 3.2 `http_server.rs` 改动（T2-3）

`AppState` 新增 `runtime: Arc<RuntimeDriver>` 与 `broadcast: Arc<DashMap<Uuid, broadcast::Sender<Value>>>`（每 thread 一通道）。

`turn_start` handler 改写：
```rust
async fn turn_start(AuthUser(c), State(st), Path(id), Json(req)) {
    // 1. 校验 thread 租户 + 取 codex_thread_id（可能 NULL=新 thread）
    let row = SELECT codex_thread_id, owner_user_id FROM threads WHERE id=$1 AND tenant_id=$2;
    // 2. 建 turn 行 status=running
    INSERT INTO turns(thread_id,status,started_at) VALUES($1,'running',NOW()) RETURNING id;
    // 3. dispatch: 若 codex_thread_id NULL → StartThread（驱动器 thread/start，回写 codex_thread_id）；
    //    否则 → 驱动器 resume
    st.runtime.dispatch(StartThread{id}) / ensure resume
    // 4. RunTurn
    st.runtime.send(RunTurn{turn_db_id, id, input});
    // 5. drain event_rx，每条：persist items+app_server_events + broadcast + 累计 usage
    while let Some(ev) = st.runtime.next_event().await {
        persist_item(&pool, &ev);          // INSERT ON CONFLICT DO UPDATE
        persist_raw_event(&pool, &ev);     // app_server_events ON CONFLICT DO NOTHING
        broadcast(&st.broadcast, id, &ev); // ws 订阅者即时收到
        if ev.is_turn_completed { break; }
    }
    // 6. update turns: status=completed, tokens, cost, model, completed_at
    Ok(Json({"turn_id": turn_db_id}))
}
```

新增端点 `POST /v1/threads/{id}/turns/{turn_id}/interrupt`（AC2.5）：发 Interrupt 命令，UPDATE turns status='interrupted'。

### 3.3 `ws.rs` 改动（T2-4）

`run` 改为：先 poll 历史到 last_seq（回放），然后 `subscribe` broadcast channel，`select!` broadcast + 定期 perm check：
```rust
let rx = st.broadcast.get(&thread_id).map(|t| t.subscribe());
loop {
    tokio::select! {
        Ok(frame) = rx.recv() => { socket.send(Text(frame)); }   // 即时推送
        _ = sleep(5s) => { check_membership; revoked→close; }
        // 兜底：每 10s 再 poll items（防 broadcast 丢失，回填缺口）
    }
}
```
无 broadcast 通道时退化为纯 poll（兼容 M1 行为）。

### 3.4 `model_gateway.rs` 扩展（T2-6）

`ModelGateway::start` 增加 upstream passthrough：
- 读 env `NEXUS_UPSTREAM_MODEL_URL` + `NEXUS_UPSTREAM_MODEL_KEY`。
- 有 upstream：`handle_request` 用 `std::net` 连上游，转发 body + 替换 Authorization，回传响应 + 计数。M0 的单线程同步够用（PoC 级，M4 换 reqwest/tokio）。
- 无 upstream：维持 M0 mock（返回 `nexus-gateway-mock`）。

`config.toml` 仍由 `execpolicy_rules::write_config_toml` 写，`model_provider.base_url = gateway_url/v1`，`wire_api = "responses"`。

### 3.5 `main.rs` serve 改动

`run_serve` 新增参数 `--codex-bin` `--codex-home`（默认 `/home/me/.local/bin/codex` 与 `~/.nexus-control/codex-home`），启动时：
- 初始化 model_gateway（spawn 线程）
- 写 config.toml 指向 gateway
- spawn RuntimeDriver 驱动线程
- AppState 注入 `runtime` + `broadcast`

## 4. 事件→表映射

| app-server 通知 | Nexus 表 | 字段映射 |
|---|---|---|
| `item/started` `item/completed` | `items` | seq=item.seq, item_type=method, content_ref=item JSON, ON CONFLICT(thread,turn,seq) DO UPDATE |
| 全部通知 | `app_server_events` | thread_id, turn_id, seq, event_json=raw, ON CONFLICT(thread,seq) DO NOTHING |
| `turn/completed` | `turns` | status=completed, completed_at, cost/token（从 turn/completed payload） |
| `thread/tokenUsage/updated` | `turns` | input_tokens/output_tokens/cost_micros（累加 ON CONFLICT DO UPDATE） |
| `thread/started` | `threads` | codex_thread_id=resp.thread.id（首次） |

seq 来源：app-server item 通知带 `item.id`（int seq）或 `turn_id`+序号。驱动器维护 per-turn 自增 seq 计数器（若通知无显式 seq），保证单调。

## 5. 关键决策

1. **单驱动线程串行**：app-server stdio 天然串行；M2 不做多进程池（M5+ K8s）。简单且正确。
2. **kill+respawn+resume 实现 interrupt/resume**：复用 M0 验证路径；不引入 stdin/stdout 并发 select（Simplicity First）。codex_thread_id 落库保证 resume 不丢状态。
3. **broadcast per-thread**：tokio::sync::broadcast（多 WS 订阅者）；容量 256，丢旧不阻塞生产者。poll 兜底回填缺口。
4. **gateway passthrough 用 std::net**：与 M0 gateway 同构（单线程同步），不引入 reqwest（M4 换）。真实模型可选，验收闭环不依赖。
5. **items ON CONFLICT DO UPDATE**（非 DO NOTHING）：item/started 与 item/completed 同 seq，需更新 content_ref/digest——幂等且语义正确。

## 6. 有意简化（Simplicity First）

| 项 | M2 | 后续 |
|---|---|---|
| 进程池 | 单进程串行 | M5+ K8s 预热池 |
| interrupt | kill+respawn | M3+ turn/interrupt RPC（需并发 stdio select） |
| broadcast 容量丢包 | poll 兜底回填 | M4 持久订阅 |
| gateway 同步 | std::net 单线程 | M4 reqwest/tokio |
| usage 计量 | 落 turns 表 | M4 计量看板/成本归因 |
| tracing | per-turn span | M4 全链路 OTel |

## 7. 自主审查结论

| # | 审查项 | 结论 |
|---|---|---|
| 1 | async/blocking 隔离是否彻底 | ✅ 阻塞 stdio 全在驱动线程，async 仅 recv channel |
| 2 | 串行是否阻塞 WS 推送 | ✅ broadcast 独立于驱动线程，多订阅者并发收 |
| 3 | resume 历史完整性 | ✅ codex_thread_id 落库 + app_server app-server 内置 thread 状态 + items ON CONFLICT 幂等 |
| 4 | 不改 codex-rs 内核 | ✅ 仅 nexus-control 扩展 |
| 5 | gateway 真实/mock 双模 | ✅ env 切换，闭环不依赖 key |
| 6 | seq 单调性 | ✅ 驱动器 per-turn 自增；resume 时从 PG max(seq) 起续 |
| 7 | interrupt 资源回收 | ✅ kill 释放，respawn 在下条命令前惰性 |

**结论：方案 OK，可全面开工。** 3 处简化（单进程/interrupt=kill/gateway sync）均符合 MVP 边界，不偏离架构产物。
