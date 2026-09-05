# Nexus 构建进度跟踪

> 来源：`roadmap.md` 开发路线图 · 实时更新 · 工作目录 `~/Nexus`
> 当前阶段：**① PoC（M0）** · 起始 2026-09-06

## 总览

| 维度 | 值 |
|---|---|
| 总任务 | 70 项（5 阶段 / 13 里程碑 M0-M12+） |
| 已完成 | 0 |
| 进行中 | 1（T0-1 app-server 集成） |
| 待启动 | 69 |
| 当前 worktree | `.worktrees/feat/nexus-m0-poc`（分支 `feat/nexus-m0-poc`） |
| 控制面 crate | `codex-rs/nexus-control/`（Rust，workspace member） |

## 阶段进度

| 阶段 | 里程碑 | 任务数 | 完成 | 状态 |
|---|---|---|---|---|
| ① PoC | M0 | 8 | 0 | 🔄 进行中 |
| ② 单租户 MVP | M1-M4 | 27 | 0 | ⏳ 待启动 |
| ③ 多租户隔离 | M5-M7 | 17 | 0 | ⏳ 待启动 |
| ④ 可靠性治理 | M8-M10 | 16 | 0 | ⏳ 待启动 |
| ⑤ 规模化生态 | M11-M12+ | 13 | 0 | ⏳ 待启动 |

## M0 PoC 任务状态（8 项）

| 任务ID | 任务名 | 状态 | 验收 | 备注 |
|---|---|---|---|---|
| T0-1 | app-server 协议集成 PoC | 🔄 | app-server 起 unix socket，thread/start+turn/start+事件流回吐 | 编码中 |
| T0-2 | 事件流消费→本地库落库 | ⏳ | ServerNotification 按 item_seq 幂等写入 | 依赖 T0-1 |
| T0-3 | thread/resume 跨进程恢复 | ⏳ | 杀进程→resume→事件 seq 连续 | 依赖 T0-1,T0-2 |
| T0-4 | execpolicy 规则集下发 | ⏳ | rm -rf /被拦截、ls放行 | 依赖 T0-1 |
| T0-5 | 三层沙箱容器层搭建 | ⏳ | K8s Pod+Seccomp；NetworkPolicy全禁 | |
| T0-6 | 沙箱启动自检脚本 | ⏳ | 5项自检通过 | 依赖 T0-5 |
| T0-7 | Model Gateway 代理验证 | ⏳ | 沙箱出站只到Gateway | 依赖 T0-1 |
| T0-8 | M0 PoC 集成验收 | ⏳ | 端到端跑通 | 依赖 T0-1~7 |

## 关键技术决策记录

| # | 决策 | 理由 | 日期 |
|---|---|---|---|
| D1 | Nexus 控制面用 Rust | 与 Harness 同语言，直接复用 app-server-protocol 类型；新 crate 不膨胀 core | 2026-09-06 |
| D2 | 控制面 crate 加入 codex-rs workspace | 共享构建缓存；类型引用简单；codex-rs 即 Nexus 工作区 | 2026-09-06 |
| D3 | app-server 经 unix socket 集成 | README 明示"intended for local control-plane clients"；JSON-RPC over unix socket | 2026-09-06 |
| D4 | PoC 阶段事件落库用 SQLite | 单租户 PoC 无需 Postgres；M1 再迁 Postgres+RLS | 2026-09-06 |

## 更新日志

- 2026-09-06：勘察仓库，创建 worktree `feat/nexus-m0-poc`，启动 M0 PoC。确认 app-server bin=`codex-app-server`，unix socket 传输。
