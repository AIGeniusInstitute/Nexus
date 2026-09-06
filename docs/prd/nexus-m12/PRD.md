# Nexus M12 PRD — 评测中心 + CI 门禁（治理维度收尾）

## 背景
roadmap 阶段④"可靠性与治理"（M8-M10）：M10 审计 WORM + M11 全链路 tracing 已覆盖合规/可观测。剩 M8 评测体系（T8-1 评测中心 + T8-3 CI 门禁，P1）。本里程碑补齐评测维度，收尾治理阶段。

## 目标
1. `eval_cases` 表：定义评测用例（input + 期望 expected_status / expected_contains）
2. `eval_runs` 表：记录每次评测运行结果（passed + detail）
3. API：POST/GET `/v1/evals/cases`；POST `/v1/evals/runs/{case_id}`（提交 turn_id 断言）；GET `/v1/evals/runs`
4. CI 门禁脚本 `scripts/eval-gate.sh`：起 turn → 断言 → exit 0/1

## 非目标
- 不自动起 turn（eval 接收已完成的 turn_id 断言；起 turn 用既有 /v1/threads/{id}/turns，职责分离，Simplicity First）
- 不做多模型校准（T9-1 多模型路由，glm 配额超限，外部依赖留着）
- 不做五评测平面全集（简化为 status + contains 断言，骨架可扩展）

## AC
- AC12.1 eval_cases/eval_runs 表建立
- AC12.2 POST /v1/evals/cases 创建用例（admin）
- AC12.3 POST /v1/evals/runs/{case_id} 提交 turn_id → 查 turn+items 断言 → 记 eval_runs(passed)
- AC12.4 GET /v1/evals/runs 返回结果列表
- AC12.5 CI 门禁脚本：SIMULATE turn completed → eval passed → exit 0
- AC12.6 零回归：M0-M11 路径不退化
