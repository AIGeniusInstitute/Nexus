# M18 多 Agent 协作编排 任务状态

## 已完成（T18-1~T18-5）
- T18-1 migration `20260906000013_m18_orchestration.sql`：orchestrations + orchestration_agents 表
- T18-2 `orchestrator.rs`（~380 行）：3 模式实现
  - run_agent_turn 原语：tokio::select! 并发 drain turn_fut + 500ms 轮询 resolve 审批
  - orchestrator-worker：orchestrator→plan，N workers 串行，fan-in
  - peer：N agents tokio::spawn 并行，fan-in
  - critic-adversarial：producer→critic gate（REVISE/修订→修订循环 MAX_REVISE=2，APPROVE→完成）
- T18-3 http_server.rs：POST/GET /v1/orchestrations + GET /v1/orchestrations/{id}；AppState 加 base_url
- T18-4 main.rs：标题 M18；base_url 从 addr 派生（0.0.0.0→127.0.0.1 容器内自调用）
- T18-5 db.rs/lib.rs 注册 migration + 模块

## e2e 验证（Docker 8765 + SIMULATE_APPROVAL=1）
- orchestrator-worker（orch 1）：orchestrator+2 workers 全 completed，fan-in 落库
- peer（orch 2）：2 peers 并行 completed
- critic-adversarial（orch 3）：producer+critic 1 轮 completed（gate APPROVE）
- 编排层自动 resolve 审批（SIMULATE_APPROVAL=1 下 turn completed）

## bug 修复
- agent_seq INT4 vs i64(INT8) decode 失败 → AgentStepRow.agent_seq 改 i32

## 验证：cargo check 0 error 0 warning，cargo test 32/32 零回归
