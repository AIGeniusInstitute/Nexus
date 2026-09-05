# Nexus 构建进度跟踪

> 来源：`roadmap.md` 开发路线图 · 实时更新 · 工作目录 `~/Nexus`
> 当前阶段：**① PoC（M0）✅ 完成 → ② 单租户 MVP（M1）待启动** · 起始 2026-09-06

## 总览

| 维度 | 值 |
|---|---|
| 总任务 | 70 项（5 阶段 / 13 里程碑 M0-M12+） |
| 已完成 | 8（M0 PoC 全部交付） |
| 进行中 | 0 |
| 待启动 | 62 |
| 当前 worktree | `.worktrees/feat/nexus-m0-poc`（分支 `feat/nexus-m0-poc`，待合并 main） |
| 控制面 crate | `codex-rs/nexus-control/`（Rust，workspace member） |

## 阶段进度

| 阶段 | 里程碑 | 任务数 | 完成 | 状态 |
|---|---|---|---|---|
| ① PoC | M0 | 8 | 8 | ✅ 完成 |
| ② 单租户 MVP | M1-M4 | 27 | 0 | ⏳ 待启动（下一步 M1） |
| ③ 多租户隔离 | M5-M7 | 17 | 0 | ⏳ 待启动 |
| ④ 可靠性治理 | M8-M10 | 16 | 0 | ⏳ 待启动 |
| ⑤ 规模化生态 | M11-M12+ | 13 | 0 | ⏳ 待启动 |

## M0 PoC 任务状态（8/8 ✅ 完成）

| 任务ID | 任务名 | 状态 | 验收 | 证据 |
|---|---|---|---|---|
| T0-1 | app-server 协议集成 PoC | ✅ | AC1.1-1.5 | stdio JSON-RPC，initialize/thread/turn，130 events |
| T0-2 | 事件流消费→本地库落库 | ✅ | AC2.1-2.2 | file-backed store，HashSet O(1) 幂等，130=130 |
| T0-3 | thread/resume 跨进程恢复 | ✅ | AC3.1-3.3 | kill→respawn→resume，seq 106→107-130 连续 |
| T0-4 | execpolicy 规则集下发 | ✅ | AC4.1-4.3 | Starlark .rules，rm→Forbidden/ls→Allow，4 单测 |
| T0-5 | 三层沙箱容器层 | ✅ | AC5.1,5.3 | Docker non-root+readonly+seccomp+network none，ping 拒 |
| T0-6 | 沙箱启动自检 | ✅ | AC6.1-6.2 | 5 项自检 exit 0；破坏→exit 1 |
| T0-7 | Model Gateway 代理验证 | ✅ | AC7.1-7.3 | 6 请求经 gateway，令牌 401，计量 count |
| T0-8 | M0 PoC 集成验收 | ✅ | AC8.1-8.3 | 端到端 ls，aggregatedOutput 产物，resume 一致 |

**三大假设验证**：✅ H1 长会话可恢复 | ✅ H2 execpolicy 可下发 | ✅ H3 三层沙箱生效

## 关键技术决策记录

| # | 决策 | 理由 | 日期 |
|---|---|---|---|
| D1 | Nexus 控制面用 Rust | 与 Harness 同语言，直接复用 app-server-protocol 类型；新 crate 不膨胀 core | 2026-09-06 |
| D2 | 控制面 crate 加入 codex-rs workspace | 共享构建缓存；类型引用简单；codex-rs 即 Nexus 工作区 | 2026-09-06 |
| D3 | app-server 经 stdio 子进程集成 | 可 kill/restart 验证 resume；控制面/执行面物理分离 | 2026-09-06 |
| D4 | PoC 事件落库用 file-backed JSON store | rusqlite links=sqlite3 与 workspace libsqlite3-sys 冲突；M1 迁 Postgres | 2026-09-06 |
| D5 | seccomp defaultAction=ALLOW + 显式禁逃逸 syscall | 保证 codex 可跑（不禁 clone/fork），同时验证机制生效；M2 换严格 allow list | 2026-09-06 |
| D6 | 沙箱网络隔离用 --network none / --internal | AC5.1 双保险；AC5.2 gateway 容器化联调留 M2 | 2026-09-06 |

## 更新日志

- 2026-09-06：勘察仓库，创建 worktree `feat/nexus-m0-poc`，启动 M0 PoC。
- 2026-09-06：T0-1/2/3 交付（首个 Agent）— H1 验证通过，130 events + resume seq 连续，10 AC PASS。commit feat 分支。
- 2026-09-06：T0-4/7 交付（扩展 Agent）— execpolicy_rules.rs（4 单测）+ model_gateway.rs（3 单测），AC4.1-4.3 + AC7.1-7.3 PASS。
- 2026-09-06：T0-5/6 交付（主线）— sandbox/（seccomp profile + Dockerfile + selfcheck.sh + run-sandbox.sh），Docker 容器三层隔离验证，AC5.1/5.3 + AC6.1/6.2 PASS。
- 2026-09-06：T0-8 集成验收 — 复用首次 PoC 130 events + 独立核查事件库提取 ls 产物，AC8.1-8.3 PASS。**M0 PoC 全部 8 任务完成，三大假设验证通过。**
- 2026-09-06：写 test_report HTML（`docs/test_report/nexus-m0-poc/TEST_REPORT.html`，13 用例 + AC 矩阵 + 三大假设 + 关键证据）。准备合并 feat→main + push。
- **下一步：M1 单租户 MVP**（Postgres+RLS / 结构化 tracing / thread 所有权 / Fork-Rollback / 真实模型经 gateway）。
