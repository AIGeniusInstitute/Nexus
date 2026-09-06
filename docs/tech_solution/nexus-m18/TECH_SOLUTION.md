# M18 多 Agent 协作编排 技术方案

## 核心架构判断
**协作编排层位于 turn 之上，通过 reqwest 内部 HTTP 自调用现有端点，不重构 turn_start drain。**

turn_start handler（http_server.rs:411-683）的 ~140 行 drain 循环与 acquire/release RAII、approval park、usage 记录耦合，硬编码在 HTTP handler 内未抽成可复用函数。重构它触及 M3/M4/M5/M6/M7/M10/M11/M15 全部里程碑验证过的路径，风险高且违反外科手术原则。

**最小侵入路径**：新建 `orchestrator.rs` 模块，用 reqwest 调 localhost：
- POST /v1/threads（创建 agent thread）
- POST /v1/threads/{id}/turns（驱动 turn，阻塞至 completed）
- GET /v1/threads/{id}/items（读取 agent 输出，?since 增量）
- POST /v1/approvals/{aid}/resolve（自动审批）

## 数据模型
```sql
CREATE TABLE orchestrations (
  id BIGSERIAL PK, tenant_id BIGINT, name TEXT,
  mode TEXT CHECK in ('orchestrator-worker','peer','critic-adversarial'),
  status TEXT DEFAULT 'running' CHECK in ('running','completed','failed'),
  prompt TEXT, created_by BIGINT, created_at, completed_at
);
CREATE TABLE orchestration_agents (
  id BIGSERIAL PK, orchestration_id BIGINT FK,
  thread_id UUID, agent_seq INT, role TEXT,
  turn_id BIGINT, status TEXT DEFAULT 'pending',
  output_ref TEXT, created_at
);
```

## 模式实现
### run_agent_turn(client, base, auth, thread_id, input) → output
核心原语。spawn turn POST 为 tokio task，`tokio::select!` 并发：
- turn_fut 分支：turn completed → 返回
- sleep 分支（每 500ms）：poll GET /v1/threads/{id}/approvals → pending → POST resolve approve

turn 完成后 GET /v1/threads/{id}/items → 取最后 agentMessage content_ref 作为 output。

### orchestrator-worker
1. agent0(orchestrator) turn(prompt) → plan
2. agents 1..N(worker) turn("上下文:<plan> 你是 worker K，产出你的部分") → 各 output
3. fan-in：concat worker outputs

### peer
1. N agents 并行（tokio::join）各 turn(prompt + "你是 peer K 独立产出")
2. fan-in：concat（pool_size ≥ N 否则退化串行）

### critic-adversarial
1. producer turn(prompt) → output
2. critic turn("评审:<output> 需修订回答 REVISE:<意见> 否则 APPROVE") → critique
3. gate：critique 含 "revise"/"修订" → producer turn("根据评审修订:<critique>") → loop（MAX_REVISE=2）
4. else done

## 接入点
- AppState 加 `base_url: String`（main.rs 从 addr 设）
- orchestrator 用 `st.jwt` mint 短时 JWT（24h，复用请求用户 tid/uid/perms）
- 路由：POST /v1/orchestrations、GET /v1/orchestrations、GET /v1/orchestrations/{id}

## 关键决策
1. HTTP 自调用非函数复用——不碰 turn_start，零回归风险（Surgical）
2. 每 agent 独立 thread——codex_thread_id 是 thread 级别，共享会 resume 污染上下文
3. 自动审批——SIMULATE_APPROVAL=1 下编排无人值守，复用 M3 approval 端点
4. gate 关键词判定——简单 includes("revise")/includes("approve")，不引入 NLP
5. peer 并行靠 tokio::join——pool_size 限制实际并行度，不足时退化为串行（正确性不破）

## 风险
- driver pool 竞争：编排 turn 与外部 turn 共享 pool。pool_size ≥ 并行 agent 数才真并行。peer 模式 e2e 需 POOL_SIZE ≥ N。
- mock 模型返回固定文本：agent 输出质量取决于模型，mock 下演示状态机正确性，真实协作需真实模型。
