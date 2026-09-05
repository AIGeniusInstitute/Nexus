# Nexus M5 — 并发 Turn 池 · PRD

> 里程碑 M5（Driver Pool / Concurrent Turns）· 2026-09-06 · 分支 `feat/nexus-m5`

## 1. 背景

M0–M4 全部交付。但当前架构有一处硬瓶颈：**全局单 mutex 串行所有 turn**。

`runtime_events: Arc<Mutex<UnboundedReceiver<TurnEvent>>>` 被 turn_start 在整个 turn 期间持锁 drain。这意味着：**所有租户、所有用户的 turn 全局串行**——一个长 turn 会阻塞所有其他 turn（M4 的 max_concurrent_turns 门控只能 429 拒绝，不能真正并发）。单 driver 线程只持有 1 个 AppServerProcess。

M5 破除这一瓶颈：**driver 池**——N 个独立 driver 线程，各持一个 app-server 进程，支持 N 路真并发 turn。

## 2. 目标

| # | 目标 | 价值 |
|---|------|------|
| G1 | Driver 池 | N 个独立 driver（N 个 app-server 进程），N 路真并发 turn |
| G2 | 池调度 | turn_start acquire 空闲 slot（满则 429/排队上限），drain 该 slot 事件（无 mutex，独占） |
| G3 | 审批路由 | resolve/interrupt 路由到持该 turn 的 driver slot（turn→slot 映射） |
| G4 | 池配置 | `NEXUS_POOL_SIZE`（default 4），可水平扩展 |
| G5 | 零回归 | SIMULATE/真实模型/M3 审批/M4 计量 全部不受影响 |

## 3. MVP 范围

| 范围 | 说明 |
|------|------|
| ✅ M5 | G1–G5 |
| ⏭ M6+ | 真实云多 Pod 分布式池（K8s Deployment + Redis 调度），当前单进程内池 |

## 4. 验收标准

### AC5.1 Driver 池
- `spawn_pool(codex_bin, codex_home, pool_size, start_approval_id)` 创建 N 个独立 driver 线程，各持 AppServerProcess。
- `DriverPool` 管理 N slots，每 slot 独立 cmd_tx + event_rx（无共享 mutex）。

### AC5.2 池调度
- turn_start：`pool.acquire().await` 取空闲 slot → 发 RunTurn → 独占 drain 该 slot event_rx（无全局锁）→ release（turn 终态时）。
- 全部 slot 忙且达 max_concurrent_turns → 429（M4 门控保留，但语义升级为"池满"）。
- 同一 turn 的所有事件（approval/requested、item/*、turn/completed）来自同一 slot（不串味）。

### AC5.3 审批路由
- turn_start acquire slot K 后，记录 `turn_db_id → K` 映射。
- `POST /v1/approvals/{id}/resolve`：查 ticket 的 turn_id → turn→slot 映射 → 发 ResolveApproval 到 driver K 的 cmd_tx。
- turn 终态时清映射。
- interrupt 同理路由。

### AC5.4 池配置
- `NEXUS_POOL_SIZE`（default 4）控制 driver 数。
- main.rs run_serve 用 spawn_pool 替换 spawn。

### AC5.5 零回归
- SIMULATE 3 例（approve/deny/interrupt）+ M4 计量 e2e 全部不变。
- cargo check/test 不回归。

## 5. 非目标

- 真实云多 Pod 分布式池（M6+，需 K8s + Redis 调度）。
- driver 动态扩缩容（M6+，需 pool size 热改）。
- per-tenant 独占 slot 隔离（M6+，当前池共享）。

## 6. 风险

| 风险 | 缓解 |
|------|------|
| 多 driver 共用 codex_home（config.toml/rules 冲突） | 共享只读（config.toml/rules 是启动期一次性写，各 driver 读同一路径 OK）；codex_thread_id 全局唯一（uuid）不冲突 |
| approval 路由错 slot | turn→slot 映射 + ticket.turn_id 双向查；路由前校验 slot alive |
| 并发 turn 的 app_server_events seq 冲突 | seq 是 per-thread 单调（M2 设计），各 turn 在自己 thread 内递增，不跨 turn 冲突 |
| 池满饥饿 | max_concurrent_turns 429 兜底 + pool_size >= 典型租户并发需求 |
