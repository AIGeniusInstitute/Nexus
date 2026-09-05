# Nexus M2 — 执行状态

> 分支：`feat/nexus-m2` · worktree：`.worktrees/feat/nexus-m2`
> 技术方案：`docs/tech_solution/nexus-m2/TECH_SOLUTION.md`（自主审查通过，§7）
> 日期：2026-09-06

## 依赖序与进度

| 任务 | 描述 | 状态 |
|---|---|---|
| T2-1 | Runtime 适配层（驱动线程+channel 桥） | ⏳ 待开始 |
| T2-2 | 事件落库映射（items+app_server_events+turns usage） | ⏳ 待开始 |
| T2-3 | turn_start 真实接线 + interrupt 端点 | ⏳ 待开始 |
| T2-4 | WS broadcast 实时推送 | ⏳ 待开始 |
| T2-5 | 断线重连完整历史（kill→respawn→resume） | ⏳ 待开始 |
| T2-6 | model_gateway upstream passthrough | ⏳ 待开始 |
| T2-7 | 结构化 tracing + 计量 | ⏳ 待开始 |

图例：🔄 进行中 / ✅ 完成 / ⏳ 待开始

## 执行记录

### 2026-09-06
- PRD + 技术方案编写完成，§7 自主审查通过（3 处简化符合 MVP 边界）
- worktree feat/nexus-m2 创建（base main 034fad0）
- 开工 T2-1...

### 2026-09-06（验证收尾）

**编译与单测**
- `cargo check -p nexus-control` → 0 error 0 warning
- `cargo test -p nexus-control` → 14/14 PASS（零回归）

**端到端验证（Docker postgres:16-alpine + codex app-server + mock gateway，端口 8765）**
| 用例 | 结果 |
|---|---|
| TC-01 m2 迁移增量（items.codex_item_id + unique partial index） | ✅ |
| TC-02 turn 真实执行（app-server 驱动 → items+events 落库 → completed） | ✅ |
| TC-03 事件幂等（item/started+completed 同 codex_item_id → ON CONFLICT DO UPDATE 1 行） | ✅ |
| TC-04 WS 实时推送（broadcast 通道即时 frame） | ✅ |
| TC-05 断线重连 codex_thread_id 稳定（kill -9 → respawn+resume → 两轮 id 相同） | ✅ |
| TC-06 历史无缺口（GET items 返回全部，seq 7→26 跨 respawn 续接） | ✅ |
| TC-07 thread/resume 复用（第 2 turn 走 resume 路径） | ✅ |
| TC-08 interrupt（turn→interrupted，后续 turn 正常 completed） | ✅ |
| TC-09 model_gateway mock 模式跑通闭环 | ✅ |
| TC-10 计量路径就位（mock=0，真实模型触发 tokenUsage/updated） | ✅ |

**修复的 bug**
1. items 落库静默失败：partial unique index `ON CONFLICT (codex_item_id)` 需带 WHERE 谓词 → `ON CONFLICT (codex_item_id) WHERE codex_item_id IS NOT NULL DO UPDATE`
2. 断线重连 codex_thread_id 变化：死进程未检测，resume 失败回退 thread/start（新线程丢状态）→ 加 `is_alive()` 检测，死了 respawn+resume（非 thread/start）

**交付物**
- 测试报告：`docs/test_report/nexus-m2/TEST_REPORT.html`（archify 风格 + 10 用例 + AC2.1~AC2.7 矩阵 + 证据 + SVG 数据流图）

## 最终状态

| 任务 | 状态 |
|---|---|
| T2-1 Runtime 适配层（驱动线程+channel 桥） | ✅ 完成 |
| T2-2 事件落库映射（items+app_server_events+turns usage） | ✅ 完成 |
| T2-3 turn_start 真实接线 + interrupt 端点 | ✅ 完成 |
| T2-4 WS broadcast 实时推送 | ✅ 完成 |
| T2-5 断线重连完整历史（kill→respawn→resume） | ✅ 完成 |
| T2-6 model_gateway upstream passthrough | ✅ 完成 |
| T2-7 结构化 tracing + 计量 | ✅ 完成 |

M2 全部完成，准备合并 feat/nexus-m2 → main + push 两远端。
