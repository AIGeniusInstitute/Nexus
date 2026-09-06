# 技术方案 — Nexus M15 Warm Pool

## 1. 现状
`driver_loop`（runtime.rs:263）在 `for cmd in cmd_rx.iter()` 前 `let mut proc: Option<AppServerProcess> = None`，首个 `RunTurn` 走 `if !proc.as_mut().is_some_and(|p| p.is_alive())` 分支 spawn+initialize。冷启动在首 turn 路径上。

## 2. 改动（全部 runtime.rs + http_server.rs，不改 turn_start drain）

### T15-1 driver_loop eager init
在 `let mut proc = None` 之后、`for cmd in cmd_rx.iter()` 之前插入：
```rust
let warm = std::env::var("NEXUS_DISABLE_WARM_POOL").is_err();
if warm {
    match AppServerProcess::spawn(&codex_bin, &codex_home)
        .and_then(|mut p| { p.initialize().context("warm init")?; Ok(p) }) {
        Ok(p) => { if let Some(f) = &warm_flag { f.store(true, Ordering::SeqCst); } proc = Some(p); }
        Err(e) => tracing::warn!(error=%e, "warm: eager init failed, lazy fallback"),
    }
}
```
`RunTurn` 分支不变：`is_alive()` true → 跳过 spawn+init，tracing "warm: reuse alive proc"。

### T15-2 driver_loop 签名 + warm_flag
`driver_loop(codex_bin, codex_home, cmd_rx, event_tx, approval_counter, warm_flag: Arc<AtomicBool>)`。`spawn()`（PoC 单 driver）传 dummy flag。

### T15-3 DriverPool 字段 + 可观测
- `warm_flags: Vec<Arc<AtomicBool>>`（per-slot，spawn_pool 创建，传各 driver_loop）
- `in_flight: Arc<AtomicUsize>`（acquire 后 +1，DriverGuard::Drop -1）
- `pub fn status(&self) -> PoolStatus { pool_size, warmed=count flags true, in_flight, free=pool_size-in_flight }`

### T15-4 路由
`GET /v1/runtime/pool`（需鉴权）→ `{ pool_size, warmed, in_flight, free }`。

## 3. 无竞态论证
driver 线程单线程顺序执行：eager init → `for cmd`。init 期间到达的 RunTurn 在 mpsc channel 排队；init 完成后取出处理，`proc.is_alive()`=true 跳过 spawn。无重复 spawn、无泄漏。

## 4. 验证
- AC1: 启动后 `GET /v1/runtime/pool` warmed=pool_size（4）
- AC2: 首个 turn 日志含 "warm: reuse alive proc"，无 "spawn app-server"
- AC3: `NEXUS_DISABLE_WARM_POOL=1` → warmed=0，首 turn lazy
- AC5: SIMULATE turn completed + 2 并发 + M14 fork 零回归
