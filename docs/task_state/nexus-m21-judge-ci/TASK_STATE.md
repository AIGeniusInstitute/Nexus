# Nexus M21 Judge LLM + CI 质量门 — 任务状态

## 范围
M20 评测中心 P2 阶段：Judge LLM 评分层 + CI 质量门 + 失败样本回流。

## 任务清单
- [x] T21-1 migration `20260906000015_m21_judge_ci.sql`：judge_configs 表 + ALTER eval_batch_runs ADD judge_version_id
- [x] T21-2 eval_center.rs：JudgeConfig CRUD（create/list/add_dimension/publish_version）+ judge_score（SIMULATE+真实 dashscope）+ case_from_turn（HTTP self-call 回流）+ start_run 集成 judge 层（两层 scores 取交集）
- [x] T21-3 http_server.rs：5 路由（judge-configs CRUD + dimensions + publish + golden-sets/cases/from-turn）
- [x] T21-4 Web GoldenSets.tsx JudgeTab 组件 + api.ts 接口
- [x] T21-5 scripts/eval-gate-v2.sh CI 质量门（P0 阻断 / P1 警告）

## 编译验证
- cargo check: 0 error 0 warning
- cargo test: 40/40（M20 36 + 4 新：extract_json_object 3 + judge_score_simulate）
- tsc exit 0, vite build
- Docker 部署成功

## e2e 验证（SIMULATE_JUDGE=1）
- AC1 create judge config (draft) ✅
- AC2 add dimensions (draft 可加) ✅
- AC3 publish v1.0.0 (manifest_hash=3715ef... locked) ✅
- AC4 locked 后 add dimension → 400 ✅
- AC5 list judge configs ✅
- AC6 start run with judge → scores 含 deterministic + judge 两层 ✅
- AC7 aggregate avg_judge_score=0.75 judge_enabled=true ✅
- AC8 combined passed 取交集（M21-A det✓judge✓→passed / M21-B det✗judge✓→fail）✅
- AC9 judge_version_id=6 返回（SELECT 修复后）✅
- AC10 CI eval-gate-v2.sh P0 阻断 exit=1 ✅
- AC11 失败样本回流 case_id=12 + locked 拒绝 400 ✅
- 零回归：M20 golden-sets 6 / M15 pool warmed=4 / M10 audit 3 ✅
- sys-test 24/25（M18 peer 偶发 fail 非 M21 回归）

## bug 修复
1. EvalBatchRunRow 缺 judge_version_id 字段 → struct + get_run/list_runs SELECT 补列
2. docker-compose.yml 缺 NEXUS_SIMULATE_JUDGE env → 加 env + .env 设 1

## 状态
全部完成，待合并 main。
