# PRD — Nexus M15 Runtime 池冷启动优化（Warm Pool）

## 1. 背景
roadmap T11-1：driver 池当前**懒初始化**——每个 driver 线程在首个 `RunTurn` 才 spawn + initialize app-server 子进程。首个 turn 承担冷启动（进程 spawn + JSON-RPC initialize + thread/start），延迟可观。目标：冷启动 <5s。

## 2. 目标
将 app-server 进程的 spawn + initialize 移到**启动期**（driver 线程启动即 eager init），首个 `RunTurn` 跳过冷启动直奔 turn_start。提供池可观测端点。

## 3. 非目标
- 不做多 Pod 分布式池（外部 K8s 依赖，留置）。
- 不改 turn_start drain 循环（外科手术原则）。
- 不做真实模型延迟基准（无真实模型 key 依赖），用进程存活 + 跳过分支验证。

## 4. 功能需求
| FR | 描述 |
|---|---|
| FR1 | driver_loop 启动即 eager spawn+initialize app-server，`RunTurn` 复用存活进程 |
| FR2 | eager init 失败降级 lazy（不影响启动），`NEXUS_DISABLE_WARM_POOL` 可关 |
| FR3 | `GET /v1/runtime/pool` 返回 pool_size / warmed / in_flight / free |
| FR4 | 零回归：M5 并发、M14 fork、SIMULATE 路径不退化 |

## 5. 验收标准
| AC | 验收点 |
|---|---|
| AC1 | serve 启动后（无 turn）`GET /v1/runtime/pool` warmed=pool_size |
| AC2 | 首个 turn 跳过 spawn+init 分支（tracing 日志 "warm: reuse alive proc"） |
| AC3 | `NEXUS_DISABLE_WARM_POOL=1` 时 warmed=0，首 turn lazy init（回归路径） |
| AC4 | eager init 失败时降级 lazy，turn 仍完成 |
| AC5 | 零回归：SIMULATE turn completed + 并发 2 turn + M14 fork 不退化 |

## 6. 设计要点
- **无竞态**：driver 线程单线程——eager init 在 `for cmd in cmd_rx.iter()` 之前；init 期间到达的 `RunTurn` 在 channel 排队，init 完成后才处理，proc 已存活，不会重复 spawn。
- warm_flag: per-slot `Arc<AtomicBool>`，eager init 成功置 true；in_flight: `Arc<AtomicUsize>` acquire+1/drop-1 算 free。
