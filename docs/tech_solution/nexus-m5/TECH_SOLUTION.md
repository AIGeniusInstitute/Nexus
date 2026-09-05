# Nexus M5 — 并发 Turn 池 · 技术方案

> 里程碑 M5（Driver Pool）· 2026-09-06 · 分支 `feat/nexus-m5`

## 1. 现状瓶颈（实证）

M4 `http_server.rs:220` `let mut rx = st.runtime_events.lock().await;` 在整个 turn 期间持全局 mutex drain event_rx → **所有租户 turn 全局串行**。单 driver 线程持 1 AppServerProcess。max_concurrent_turns 门控只能 429 拒绝，不能真并发。

## 2. 设计：DriverPool

### 数据结构
```rust
pub struct DriverPool {
    slots: Vec<std::sync::Mutex<DriverSlot>>,   // N slot
    cmd_txs: Vec<std::sync::mpsc::Sender<DriverCommand>>,  // 按 idx 取（Clone Sender 路由用）
    free_tx: tokio::sync::mpsc::UnboundedSender<usize>,    // 空闲 slot 队列
    free_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<usize>>,
}
struct DriverSlot {
    event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<TurnEvent>>,  // Some=空闲 None=占用
}
```

### spawn_pool
```rust
pub fn spawn_pool(codex_bin, codex_home, pool_size, start_approval_id) -> Arc<DriverPool> {
    let (free_tx, free_rx) = unbounded_channel();
    let mut slots = Vec::with_capacity(pool_size);
    let mut cmd_txs = Vec::with_capacity(pool_size);
    for i in 0..pool_size {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = unbounded_channel();
        thread::spawn(move || driver_loop(codex_bin.clone(), codex_home.clone(), cmd_rx, event_tx, start_approval_id));
        slots.push(Mutex::new(DriverSlot { event_rx: Some(event_rx) }));
        cmd_txs.push(cmd_tx);
        let _ = free_tx.send(i);  // 初始全空闲
    }
    Arc::new(DriverPool { slots, cmd_txs, free_tx, free_rx: Mutex::new(free_rx) })
}
```
**注意**：N 个 driver 共享同一 codex_home（config.toml/rules 只读 OK），各持独立 AppServerProcess（独立 codex_thread_id uuid 不冲突）。next_approval_id 各 driver 独立从 start_approval_id 起递增——**会碰撞**！需全局唯一 approval_id。

### approval_id 全局唯一
各 driver 独立递增 next_approval_id 会产生重复 id（driver 0 产 id=10，driver 1 也产 id=10）。**修复**：用 `AtomicI64` 全局计数器，`start_approval_id + fetch_add`。spawn_pool 持 `Arc<AtomicI64>`，各 driver clone 引用，`next_id = counter.fetch_add(1, ...)`。

### DriverGuard（acquire/release）
```rust
pub struct DriverGuard {
    idx: usize,
    event_rx: Option<UnboundedReceiver<TurnEvent>>,
    pool: Arc<DriverPool>,
}
impl DriverPool {
    async fn acquire(&self) -> Option<DriverGuard> {
        let idx = self.free_rx.lock().await.recv().await?;
        let rx = self.slots[idx].lock().unwrap().event_rx.take();
        Some(DriverGuard { idx, event_rx: rx, pool: Arc::clone(&self) })  // pool 通过引用传入
    }
    fn cmd_tx(&self, idx: usize) -> Option<&Sender<DriverCommand>> { self.cmd_txs.get(idx) }
}
impl Drop for DriverGuard {
    fn drop(&mut self) {
        if let Some(rx) = self.event_rx.take() {
            *self.pool.slots[self.idx].lock().unwrap() = DriverSlot { event_rx: Some(rx) };
        }
        let _ = self.pool.free_tx.send(self.idx);
    }
}
```

### AppState 变更
```rust
pub struct AppState {
    pool: PgPool, jwt, auth,
    driver_pool: Arc<DriverPool>,
    turn_slots: Arc<Mutex<HashMap<i64, usize>>>,  // turn_db_id → slot idx（路由）
    broadcast: Arc<Mutex<HashMap<Uuid, broadcast::Sender<Value>>>>,
}
// 删除 runtime_cmd + runtime_events
```

### turn_start（破除全局 mutex）
```rust
async fn turn_start(...) {
    // M4 并发门控保留（max_concurrent_turns 429）
    let guard = st.driver_pool.acquire().await.ok_or((503, "pool exhausted"))?;
    // 记录路由
    st.turn_slots.lock().await.insert(turn_db_id, guard.idx);
    guard.cmd_tx.send(RunTurn{...});  // 经 cmd_txs[idx]
    let rx = guard.event_rx.as_mut().unwrap();
    while let Some(ev) = rx.recv().await {  // 独占 drain，无全局锁
        // ... 既有落库/审批/计量逻辑
        if ev.is_turn_completed { break; }
    }
    st.turn_slots.lock().await.remove(&turn_db_id);
    drop(guard);  // release slot
}
```
guard 的 cmd_tx：DriverGuard 需暴露 `cmd_tx(&self) -> &Sender`（从 pool.cmd_txs[self.idx]）。

### resolve/interrupt 路由
```rust
async fn approval_resolve(...) {
    // 查 ticket.turn_id → turn_slots → slot idx
    let turn_id = ...;  // from ticket
    let slot_idx = st.turn_slots.lock().await.get(&turn_id).copied();
    if let Some(idx) = slot_idx {
        st.driver_pool.cmd_tx(idx).send(ResolveApproval{...});
    }
}
async fn turn_interrupt(...) {
    let slot_idx = st.turn_slots.lock().await.get(&turn_id).copied();
    if let Some(idx) = slot_idx { st.driver_pool.cmd_tx(idx).send(Interrupt); }
}
```
ws.rs 撤销中断：原 `st.runtime_cmd.send(Interrupt)` → 需找该 thread 的 in-flight turn slot。ws 有 thread_id，查 `SELECT id FROM turns WHERE thread_id=$1 AND status='running'` → turn_id → turn_slots → slot。或 ws 直接发所有 slot Interrupt（粗暴）。**Surgical**：ws 查 running turn → route。

## 3. 任务分解

| 任务 | 文件 | 内容 |
|------|------|------|
| T5-1 | `runtime.rs` | DriverPool + DriverGuard + spawn_pool（AtomicI64 全局 approval_id） |
| T5-2 | `http_server.rs` | AppState 改 driver_pool + turn_slots；turn_start acquire/drain/release；resolve/interrupt 路由 |
| T5-3 | `ws.rs` | 撤销中断：查 running turn → turn_slots → slot cmd_tx |
| T5-4 | `main.rs` | spawn_pool 替换 spawn；NEXUS_POOL_SIZE（default 4） |
| T5-5 | e2e | 并发 2 turn 真并发（不互斥）+ SIMULATE 3 例零回归 + M4 计量零回归 |

## 4. 验证策略

| 验证 | 方法 |
|------|------|
| cargo check/test | 0 error 0 warning；单测不回归 |
| e2e 并发 | 2 turn 同时 start（pool_size≥2）→ 两个都 approved/completed（不互斥，不 429） |
| e2e 审批路由 | turn1 在 slot0、turn2 在 slot1，各 resolve 路由正确 |
| e2e 零回归 | SIMULATE approve/deny/interrupt + M4 usage 落库不变 |

## 5. 自审

- [x] 不改 codex-rs 内核
- [x] 向后兼容：pool_size=1 时等价 M4 单 driver（free_rx 单 slot）
- [x] approval_id 全局唯一（AtomicI64）
- [x] Surgical：只动 AppState/turn_start/resolve/interrupt/ws/main，不改 driver_loop 内部逻辑
- [x] 退出条件：cargo + e2e 并发 + 零回归

## 6. 方案自确认

✅ 方案 OK。DriverPool + free-list 队列 + DriverGuard(Drop release) + turn_slots 路由 + AtomicI64 全局 approval_id。开工 T5-1。
