# Nexus M21 技术方案 — Judge LLM + CI 质量门 + 失败样本回流

## 设计原则
1. **Simplicity First**：Judge 直接调 dashscope chat API（reqwest），不经过 codex/gateway（Judge 是文本评分，无需 harness/工具）；不引入统计 crate（McNemar 留扩展）；单 Judge（3-Judge 留扩展）
2. **Surgical Changes**：不重构 start_run drain 循环；Judge 评分在 `score_case` 之后叠加；M20 的 6 表不动，纯增量
3. **SIMULATE 兼容**：NEXUS_SIMULATE_JUDGE=1 返回合成评分（e2e pipeline 可验证）；真实模式调 dashscope

## 架构

```
start_run(case)
  ├─ run_case_turn → agent_output
  ├─ score_case(expected, output)          ← M20 确定性层
  └─ judge_score(judge_cfg, rubric, case, output)  ← M21 LLM 层
       └─ 构造 prompt → dashscope chat API → 解析 JSON
  → scores = {deterministic, judge, passed}
```

## 数据层（纯增量）

### migration `20260906000015_m21_judge_ci.sql`
```sql
-- JudgeConfig 逻辑实体（复用 entity_version.judge_config 类型）
CREATE TABLE IF NOT EXISTS judge_configs (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','locked','archived')),
  active_version_id BIGINT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_judge_configs_tenant_status ON judge_configs(tenant_id, status);

-- eval_batch_runs 加 judge_config_version_id（M20 表 ALTER ADD COLUMN）
ALTER TABLE eval_batch_runs ADD COLUMN IF NOT EXISTS judge_version_id BIGINT;
-- 失败样本回流：golden_set_cases 加 source 标注（M20 已有 source 列，无需 ALTER）
```

**关键**：JudgeConfig 复用 M20 entity_version 表（entity_type='judge_config'），发布逻辑与 GoldenSet/Rubric 一致（draft→publish semver+manifest_hash+locked）。content_ref 存 `{prompt_template, dimensions:[{name,weight,description}], judge_model, temperature}`。

## eval_center.rs 扩展

### judge_score 函数
```rust
pub async fn judge_score(
    judge_content: &str,   // entity_version.content_ref 序列化 JSON
    rubric_content: Option<&str>,
    case_input: &str,
    agent_output: &str,
) -> Result<Value>
```
- 反序列化 judge_content → {prompt_template, dimensions, judge_model, temperature}
- 构造 chat messages：system=prompt_template + 维度说明，user=case_input + agent_output + "返回JSON"
- 调 dashscope：`POST {UPSTREAM_MODEL_URL}/v1/chat/completions`（reqwest，Authorization NEXUS_MODEL_KEY）
- SIMULATE_JUDGE=1：返回合成 `{dimensions:[{name,score:0.75,score:0.75,rationale:"simulated"}], overall_score:0.75, passed:true}`
- 解析 LLM 返回 JSON → 结构化 `{dimensions:[{name,score,rationale}], overall_score, passed}`

### start_run 扩展
- StartRunReq 加 `judge_config_id: Option<i64>`
- 加载 judge_config locked 版本（与 rubric 同模式）
- manifest 加 judge 版本信息 → snapshot_hash 重算（含 judge 版本）
- 逐 case：score_case 之后，若 judge_content 存在 → judge_score → scores 合并
  ```json
  {"deterministic": {M20 scores}, "judge": {dimensions, overall_score, passed}, "passed": det_passed && judge_passed}
  ```
- 聚合加 `avg_judge_score`

### 失败样本回流
```rust
pub async fn case_from_turn(pool, tenant_id, gs_id, turn_id, case_key, expected_json) -> Result<i64>
```
- 校验 golden_set status==draft
- GET turn items（HTTP self-call /v1/threads/{id}/items）提取 agent_output
- INSERT golden_set_cases(input_json={user_query: 原input或output摘要}, expected_json, source=format!("trace:{turn_id}"))
- 返回 case_id

## http_server.rs 新增路由
- `POST /v1/judge-configs`（create draft）
- `GET /v1/judge-configs`（list tenant）
- `POST /v1/judge-configs/{id}/dimensions`（add dimension，仅 draft）
- `POST /v1/judge-configs/{id}/publish`（publish semver+locked）
- `POST /v1/golden-sets/{id}/cases/from-turn`（失败回流）
- start_run body 加 `judge_config_id`

## CI 质量门 `scripts/eval-gate-v2.sh`
- login → 创建/复用 golden set + cases → publish → start_run（带 judge）→ 查 aggregate
- P0：accuracy < NEXUS_EVAL_P0_THRESHOLD（默认 0.6）→ exit 1（阻断发布）
- P1：与 baseline run accuracy delta > NEXUS_EVAL_P1_DELTA（默认 0.1 下降）→ exit 0 + warn 输出
- env：NEXUS_BASE/NEXUS_EMAIL/NEXUS_PASSWORD/NEXUS_EVAL_P0_THRESHOLD/NEXUS_EVAL_P1_DELTA

## 关键决策
1. Judge 直调 dashscope（不经 codex/gateway）—— Judge 是无状态文本评分，无需 harness/工具调用/审批
2. SIMULATE_JUDGE 模式 —— e2e 可验证 pipeline，真实模式调真实模型
3. scores 两层共存 —— deterministic + judge，passed 取交集（两者都过才过）
4. JudgeConfig 复用 entity_version —— 不新建版本化机制，与 GoldenSet/Rubric 一致
5. 失败回流仅 draft golden set —— locked 不可变原则一致
6. CI 显著性 MVP 用 accuracy delta —— McNemar 需统计库，留扩展

## 风险与对策
- **dashscope 响应慢**：deepseek-v4-pro reasoning 模型 >60s → judge_score timeout 120s + e2e 默认 SIMULATE_JUDGE=1
- **LLM 返回非 JSON**：prompt 强约束 + 容错解析（提取首个 JSON 对象）
- **Judge 评分不稳定**：temperature=0 + 固定 prompt；3-Judge 留扩展
