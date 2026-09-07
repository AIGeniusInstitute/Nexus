# M20 评测中心 — 任务执行状态

## 里程碑
M20="评测中心（版本化 Golden Set + Rubric + 运行冻结快照 + 确定性评分）"，roadmap 评测体系增强维度。

## 状态
✅ 全部完成（编码+e2e+零回归+测试报告）

## 任务清单
| 任务 | 状态 | 说明 |
|------|------|------|
| T20-1 PRD | ✅ | `docs/prd/nexus-m20-eval-center/PRD.md` 11 AC + 11 测试用例 |
| T20-2 技术方案 | ✅ | `docs/tech_solution/nexus-m20-eval-center/SOLUTION.md` 6 表 DDL+模块设计 |
| T20-3 migration | ✅ | `migrations/20260906000014_m20_eval_center.sql` 6 表（entity_version/golden_sets/golden_set_cases/rubrics/eval_batch_runs/eval_case_results）纯增量不碰 M12 |
| T20-4 eval_center.rs | ✅ | ~570 行，GoldenSet/Rubric 版本化发布+score_case+start_run(HTTP self-call)+compare_runs |
| T20-5 http_server.rs | ✅ | 12 路由+12 handler+GssCreateReq/EvalBatchCompareReq |
| T20-6 Web 控制台 | ✅ | `GoldenSets.tsx` 三 tab（GoldenSet/Rubric 评分标准/批量评测运行）+api.ts 接口+App.tsx 路由 |
| T20-7 e2e 验证 | ✅ | 11 AC 全过（见测试报告） |
| T20-8 零回归 | ✅ | M10 audit/M12 eval/M14 fork/M15 pool/M16 connectors/M17 skills 全不退化 |
| T20-9 测试报告 | ✅ | `docs/test_report/nexus-m20-eval-center/TEST_REPORT.html` + 5 截图 |

## 关键决策
1. **里程碑范围**：设计文档是 3 个月企业级系统（P1/P2/P3 三阶段），遵循一里程碑一 worktree + Simplicity First，M20 聚焦 P1 MVP（版本化 GoldenSet+Rubric+运行冻结快照+确定性评分），P2(Judge LLM+CI+质量门)→M21，P3(事件溯源+Lineage+电子签名)→M22
2. **版本化复用**：不重新发明，复用 M17 skill_versions 的 version+checksum+content_ref+active_version_id 模式，泛化为 entity_version 六类骨架表
3. **turn 执行不重构 turn_start**：复用 M18 orchestrator HTTP self-call 模式，eval batch run 经 reqwest 调 localhost /v1/threads/{id}/turns，零触碰 turn_start drain（外科手术原则）
4. **SIMULATE 模式输出回退**：SIMULATE_APPROVAL 模式 driver 只发 item/completed（content_ref="approved: Approve"）不发 agentMessage，run_case_turn 加 item/completed 回退提取，使空输出评分有非空 content
5. **manifest_hash 用 SHA-256**：sha2 (workspace 0.10) 序列化 cases 全文→Sha256→hex
6. **semver bump**：首次 1.0.0，patch +0.0.1 / minor +0.1.0 / major +1.0.0

## 关键 bug 修复（Issue 闭环）
### BUG-1: compare_runs ColumnDecode 类型不匹配
- **现象**：`POST /v1/evals/batch-runs/{id}/compare` 返回 500 `ColumnDecode Option<bool> not compatible with SQL type TEXT`
- **根因**：SQL `a.scores->>'passed'` 返回 TEXT，但 Rust 声明 `Vec<(String, Option<bool>, Option<bool>)>`，`->>` 运算符返回 text 不能 decode 为 bool
- **修复**：SQL 改 `(a.scores->>'passed')::boolean AS baseline_passed`，PG cast TEXT→BOOL
- **验证**：compare 返回 `incomparable=false` + 3 case_key diff（delta=same）

## e2e 验证（Docker 8765 + POOL=2 + SIMULATE_APPROVAL=1）
| AC | 描述 | 结果 |
|----|------|------|
| AC20.1 | entity_version 骨架表建立 | ✅ 6 表全建 |
| AC20.2 | 创建 GoldenSet(draft) | ✅ id=2 status=draft |
| AC20.3 | 添加 Case | ✅ 3 case(CS-FREE/CS-HIT/CS-MUSTNOT) |
| AC20.4 | 发布版本+semver+不可变 | ✅ v1.0.0 manifest_hash=d886... status=locked |
| AC20.5 | locked 后加 Case 失败 | ✅ HTTP 400 |
| AC20.6 | 运行冻结快照 snapshot_hash | ✅ c6eeb5e9... |
| AC20.7 | Rubric 发布 | ✅ v1.0.0 manifest_hash=5977... |
| AC20.8 | 逐 case 评分 | ✅ CS-FREE pass/CS-HIT fail/CS-MUSTNOT pass |
| AC20.9 | 聚合 accuracy | ✅ 0.667 passed=2/3 |
| AC20.10 | 基线对比 | ✅ incomparable=false 3 diff delta=same |
| AC20.11 | 零回归 | ✅ M10/M12/M14/M15/M16/M17 全不退化 |

## 验证产物
- `cargo check` 0 error 0 warning
- `cargo test` 36/36（含 4 新单测：semver_bump/score_case_hit_and_violation/score_case_no_constraints/sha256_deterministic）
- `tsc --noEmit` exit 0
- `vite build` 45 modules
- e2e 11 AC 全过
- 5 张 Web 截图
- 测试报告 `docs/test_report/nexus-m20-eval-center/TEST_REPORT.html`
