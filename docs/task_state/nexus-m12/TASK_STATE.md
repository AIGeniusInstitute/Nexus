# Nexus M12 任务状态 — 评测中心 + CI 门禁

## 里程碑
M12 = 评测中心（eval cases + runs + 断言）+ CI 门禁脚本（roadmap M8 T8-1/T8-3，治理维度收尾）。
分支：`feat/nexus-m12`，base 含 M11 merge `251dd92`。

## 任务清单

| 任务 | 状态 | 说明 |
|---|---|---|
| T12-1 migration eval_cases/eval_runs | ✅ | 两表 + 3 索引（tenant/case/time）|
| T12-2 eval.rs 模块 | ✅ | EvalCase/EvalRun(FromRow+Serialize) + create_case/list_cases/run_eval(查 turn+items 断言)/list_runs |
| T12-3 http_server 路由 | ✅ | POST/GET /v1/evals/cases + POST /v1/evals/runs/{case_id} + GET /v1/evals/runs（case 创建 admin only）|
| T12-4 main.rs 标题 | ✅ | "Nexus M12: serve" |
| T12-5 CI 门禁脚本 | ✅ | scripts/eval-gate.sh（login→create case→start turn→run eval→exit 0/1）|

## 关键决策

1. **eval 不自动起 turn**：接收已完成的 turn_id 断言（起 turn 用既有 /v1/threads/{id}/turns，职责分离，避免复刻 turn_start drain 逻辑，Simplicity First）
2. **断言 status + contains**（骨架）：五评测平面留扩展（case.category 字段供分类扩展，expected_contains 可选）
3. **run_eval 查 turn 时 tenant 隔离**：turn JOIN threads tenant_id，跨租户 turn 不可断言
4. **eval_runs.detail JSONB**：记录 expected/actual status + matched_contains + items_count + turn_found，供 debug
5. **CI 脚本用 curl+python**：SIMULATE=0（mock 纯文本 turn 直接 completed）可跑，真实模式亦可用

## 坑
1. **SIMULATE_APPROVAL=1 下 turn park 阻塞**：eval-gate turn_start 审批 park 不完成（curl hang）。修复：CI gate 跑 SIMULATE_APPROVAL=0（mock 纯文本 turn 无命令→无审批→直接 completed）。
2. **approval_policy=untrusted**（M9 设）：SIMULATE=0 + mock 模型不发命令→不触发审批→turn completed。OK。

## 验证
- cargo check：0 error 0 warning
- cargo test：28/28（M11 27 + eval 1）零回归
- e2e（PG nexus-pg-m4:5434 + POOL=2 + SIMULATE_APPROVAL=0）AC12.1-12.6 全过：
  - AC12.1 eval_cases/eval_runs 表建立（migration applied incl m12）
  - AC12.2 POST /v1/evals/cases 创建 case（admin，id=2/3）
  - AC12.3 POST /v1/evals/runs/{case_id} 断言 turn 37→passed=True（completed==completed）；failed 路径 expected=interrupted→passed=False（actual=completed）
  - AC12.4 GET /v1/evals/runs 返回 run 列表（run1 passed True / run2 passed False）
  - AC12.5 scripts/eval-gate.sh exit=0（PASS）
  - AC12.6 零回归：M11 timeline + M10 audit 仍工作
- 不改 codex 内核（全部 nexus-control crate）；不动既有表（仅加 eval_cases/eval_runs）
