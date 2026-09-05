# Nexus M5 — 并发 Turn 池 · 任务执行状态

> 分支 `feat/nexus-m5` · 2026-09-06 · 基线 main@c76c25f（M4）

## 任务清单

| 任务 | 状态 | 说明 |
|------|------|------|
| T5-1 DriverPool + DriverGuard + spawn_pool | ✅ | `runtime.rs`：DriverPool（N slot，每 slot 独立 event_rx）+ free-list 队列 + DriverGuard(Drop release) + spawn_pool + AtomicI64 全局 approval_id |
| T5-2 http_server AppState + turn_start | ✅ | AppState 改 `driver_pool` + `turn_slots`；turn_start acquire/drain(独占)/release，破除全局 mutex |
| T5-2b resolve/interrupt 路由 | ✅ | approval_resolve 取 ticket.turn_id → turn_slots → slot cmd_tx；turn_interrupt 同理 |
| T5-3 ws 撤销中断路由 | ✅ | ws.rs：查 thread 的 running turn → turn_slots → slot cmd_tx（取代 M4 单 driver 广播） |
| T5-4 main.rs spawn_pool + NEXUS_POOL_SIZE | ✅ | run_serve 用 spawn_pool 替换 spawn；NEXUS_POOL_SIZE default 4；AppState 新字段 |
| T5-5 e2e 验证 | ✅ | 并发 2 turn + deny + interrupt + 计量零回归 |

## 关键设计决策

1. **free-list 队列调度**：`tokio::sync::mpsc::UnboundedChannel<usize>` 作空闲 slot 队列。acquire 从 free_rx `recv().await` 取 idx（阻塞直至有空 slot），take 出该 slot 的 event_rx；DriverGuard::Drop 把 event_rx 放回 slot + free_tx.send(idx)。无需扫描/轮询。
2. **独占 drain（无全局 mutex）**：每 slot 的 event_rx 在 acquire 时 take 出（Option），由 turn_start 独占 `recv().await` drain，turn 结束 Drop 放回。M3/M4 的 `Arc<Mutex<UnboundedReceiver>>` 全局串行锁被彻底移除 → N 路真并发。
3. **AtomicI64 全局 approval_id**：N 个 driver 各自 fetch_add 同一 `Arc<AtomicI64>`，保证跨 slot 生成的 approval_id 全局唯一连续（M3 的 per-driver 独立计数器在多 driver 下会碰撞）。
4. **turn_slots 路由**：`Arc<Mutex<HashMap<i64,usize>>>`（turn_db_id → slot idx）。turn_start acquire 后 insert，turn 终态 remove。approval_resolve/interrupt/ws-revoke 查 ticket.turn_id（或 running turn）→ turn_slots → `driver_pool.cmd_tx(idx)` 路由到持该 turn 的 driver。
5. **DriverGuard RAII**：持有 `pool: Arc<DriverPool>`，Drop 自动 release（event_rx 归位 + slot 入 free 队列），turn_start 异常早退也能保证 slot 回收。
6. **向后兼容**：pool_size=1 时等价 M4 单 driver（free_rx 单 slot，acquire 立即返回）。旧 `spawn()`/`RuntimeHandle` 保留供 PoC CLI 单 driver 场景。

## 编译/测试

- `cargo check -p nexus-control`：0 error 0 warning
- `cargo test -p nexus-control`：19/19 PASS（policy 4 + event_store 2 + metering 2 + model_gateway 2 + rbac 4 + execpolicy_rules 4 + policy glob 1）
- `cargo build -p nexus-control`：成功

## e2e 验证（PG container nexus-pg-m4:5434 + NEXUS_POOL_SIZE=2 + NEXUS_SIMULATE_APPROVAL=1）

| 用例 | 结果 | 证据 |
|------|------|------|
| AC5.1 并发 2 turn 同时 park | PASS | GET /v1/approvals 返回 2 pending（aid=6 turn7、aid=7 turn6），两 turn 同时阻塞在审批——M4 全局 mutex 下不可能 |
| AC5.2 acquire/drain/release | PASS | 两 turn 各占一 slot，独占 drain，resolve 后均 completed（turn6/turn7） |
| AC5.3 审批路由到正确 slot | PASS | resolve aid=7→turn6、aid=6→turn7 各自路由，两 turn 均收到 resolve 并 completed |
| AC5.1 approval_id 全局唯一 | PASS | aid=6,7 连续唯一（AtomicI64 跨 slot fetch_add） |
| AC5.4 NEXUS_POOL_SIZE | PASS | 日志 "driver pool spawned: 2 slots" |
| M4 计量零回归 | PASS | 2 usage_records（turn6/7，model=nexus-gateway-mock，in=10 out=20）；turns.model 写回；/v1/usage 聚合返回 |
| M3 deny 零回归 | PASS | ticket id=8 status=denied decided_by=2；turn=8 completed |
| M3 interrupt 零回归 | PASS | interrupt 路由到 turn9 的 slot；turn=9 status=interrupted |
| M4 并发门控 429 | PASS | max_concurrent_turns=1 时 turn2 返回 too_many_concurrent_turns: limit=1（门控保留，语义升级为"池满/租户满"） |

## 未实现（非目标，留 M6+）

- 真实云多 Pod 分布式池（需 K8s + Redis 调度，M6+）
- driver 动态扩缩容 / pool size 热改（M6+）
- per-tenant 独占 slot 隔离（M6+，当前池共享）
- 真实模型并发 turn（当前用 SIMULATE 验证池调度，真实模型需多 codex 进程并发，逻辑相同）

## 退出判定

✅ 全部 AC5.1–AC5.5 达成。cargo 0 error 0 warning + 19/19 单测 + e2e 并发/审批路由/deny/interrupt/计量零回归全 PASS。M5 交付完成。
