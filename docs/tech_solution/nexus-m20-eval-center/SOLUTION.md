# Nexus M20 技术方案 — 评测中心：版本化 Golden Set + 运行冻结快照

## 设计原则

1. **Simplicity First**：不引入 Temporal/Kafka/Neo4j/CAS，用 PG + 现有基础设施；评分仅确定性规则层（Judge LLM 留 M21）
2. **Surgical Changes**：不删旧表（M12 eval_cases/eval_runs 保留兼容），新表纯增量；不重构 turn_start（复用 M18 HTTP self-call 模式跑 agent）
3. **版本化模式复用**：复用 M17 skill_versions 的 version+checksum+content_ref+active_version_id 模式，泛化为 entity_version 骨架
4. **快照冻结**：EvalRun 启动时构造 Manifest JSON（golden_set 版本 + rubric 版本 hash），存 snapshot_hash，运行只读已锁定版本

## 架构

```
┌─ http_server.rs (新增 ~12 路由) ─────────────────────────────┐
│  /v1/golden-sets  /v1/golden-sets/{id}/cases  /publish       │
│  /v1/rubrics  /v1/rubrics/{id}/publish                         │
│  /v1/evals/runs (POST 批量)  /v1/evals/runs/{id}  /compare    │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌─ eval_center.rs (新模块, ~450 行) ──────────────────────────┐
│  GoldenSet: create/list/get/add_case/publish_version          │
│  Rubric: create/list/publish_version                          │
│  EvalBatchRun: start_run(freeze snapshot→run cases→score→agg)  │
│  Scoring: score_case(must_hit/must_not/acceptable) — 确定性    │
│  Compare: compare_runs(case 级 diff)                          │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌─ migrations/20260906000014_m20_eval_center.sql ──────────────┐
│  entity_version (六类骨架) + golden_sets + golden_set_cases   │
│  + rubrics + eval_batch_runs + eval_case_results              │
└───────────────────────────────────────────────────────────────┘
```

## 数据模型

### 新表（纯增量，M12 旧表 eval_cases/eval_runs 保留不动）

```sql
-- ① 版本化实体骨架（六类共用，M20 激活 golden_set + rubric）
CREATE TABLE entity_version (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL REFERENCES tenants(id),
  entity_type TEXT NOT NULL CHECK (entity_type IN
    ('golden_set','rubric','agent_release','judge_config','eval_pipeline','policy')),
  entity_id BIGINT NOT NULL,        -- 指向 golden_sets.id / rubrics.id
  version_no INT NOT NULL,
  semver TEXT NOT NULL,             -- "1.0.0"
  manifest_hash CHAR(64) NOT NULL, -- SHA-256 over content_ref
  content_ref TEXT NOT NULL,       -- 完整内容 JSON 序列化（M20 存 PG，M22 迁 CAS）
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','locked')),
  created_by BIGINT REFERENCES users(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  parent_id BIGINT REFERENCES entity_version(id),
  UNIQUE(entity_type, entity_id, version_no)
);
-- locked 行不可变：应用层检查 + 不提供 UPDATE API（只 INSERT 新版本）

-- ② Golden Set 逻辑实体
CREATE TABLE golden_sets (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL REFERENCES tenants(id),
  name TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','locked','archived')),
  active_version_id BIGINT,        -- 指向 entity_version.id（最新锁定版本）
  created_by BIGINT REFERENCES users(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ③ Case 池（draft 状态可增删，publish 时快照入 content_ref）
CREATE TABLE golden_set_cases (
  id BIGSERIAL PRIMARY KEY,
  golden_set_id BIGINT NOT NULL REFERENCES golden_sets(id) ON DELETE CASCADE,
  tenant_id BIGINT NOT NULL REFERENCES tenants(id),
  case_key TEXT NOT NULL,          -- "CS-ELIG-0231"
  domain TEXT,                     -- "入排筛选"
  difficulty TEXT,                 -- "P1"
  source TEXT,                     -- "专家构造"
  input_json JSONB NOT NULL,      -- {user_query, context_refs, patient_summary}
  expected_json JSONB NOT NULL,    -- {reference_answer, acceptable_answers, must_hit_points, must_not}
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE(golden_set_id, case_key)
);

-- ④ Rubric 逻辑实体（评分点集合）
CREATE TABLE rubrics (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL REFERENCES tenants(id),
  name TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','locked','archived')),
  active_version_id BIGINT,
  created_by BIGINT REFERENCES users(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
-- rubric 的 criteria 存在 entity_version.content_ref JSON 内（无独立 case 表）

-- ⑤ 批量评测运行（一批 GoldenSet 全部 case）
CREATE TABLE eval_batch_runs (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL REFERENCES tenants(id),
  golden_set_id BIGINT NOT NULL REFERENCES golden_sets(id),
  golden_set_version_id BIGINT NOT NULL REFERENCES entity_version(id),
  rubric_version_id BIGINT REFERENCES entity_version(id),
  thread_id UUID,                  -- agent 运行线程
  snapshot_manifest JSONB NOT NULL, -- {golden_set:{id,version,hash}, rubric:{...}, agent:{...}}
  snapshot_hash CHAR(64) NOT NULL, -- manifest 序列化后 SHA-256
  trigger_type TEXT NOT NULL DEFAULT 'manual',
  status TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running','completed','failed')),
  baseline_run_id BIGINT REFERENCES eval_batch_runs(id),
  gate_result TEXT,                -- pass/pass_with_warning/blocked（M20 留空，M21 填充）
  aggregate JSONB,                 -- {accuracy, must_hit_rate, must_not_violation_rate, total, passed}
  started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  finished_at TIMESTAMPTZ
);
CREATE INDEX idx_eval_batch_tenant_time ON eval_batch_runs(tenant_id, started_at DESC);

-- ⑥ 逐 case 结果
CREATE TABLE eval_case_results (
  id BIGSERIAL PRIMARY KEY,
  run_id BIGINT NOT NULL REFERENCES eval_batch_runs(id) ON DELETE CASCADE,
  case_id BIGINT NOT NULL REFERENCES golden_set_cases(id),
  case_key TEXT NOT NULL,
  turn_id BIGINT,                  -- agent turn
  agent_output TEXT,               -- 原文（M20 存 PG，大对象 M22 迁对象存储）
  scores JSONB NOT NULL,           -- {must_hit_points:[{point,hit}], must_not_violations:[], acceptable_match, total_score, passed}
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE(run_id, case_id)
);
CREATE INDEX idx_eval_case_results_run ON eval_case_results(run_id);
```

### 语义化版本生成
- 首次 publish：`1.0.0`
- 后续 publish 带 `bump_type`：`patch`→+0.0.1 / `minor`→+0.1.0 / `major`→+1.0.0
- version_no 递增（MAX+1）
- content_ref = JSON 序列化（golden_set: cases 全文 + distribution_meta；rubric: criteria 列表）
- manifest_hash = SHA-256(content_ref)

## 模块设计：eval_center.rs

### GoldenSet 管理
```rust
pub struct GoldenSetRow { id, tenant_id, name, description, status, active_version_id, ... }
pub struct GoldenSetCaseRow { id, golden_set_id, case_key, domain, difficulty, source, input_json, expected_json, ... }

pub async fn create_golden_set(pool, tid, uid, req) -> Result<GoldenSetRow>
pub async fn list_golden_sets(pool, tid) -> Result<Vec<GoldenSetRow>>
pub async fn get_golden_set(pool, tid, id) -> Result<GoldenSetRow>  // 含 case 列表
pub async fn add_case(pool, tid, gs_id, req) -> Result<i64>
pub async fn list_cases(pool, tid, gs_id) -> Result<Vec<GoldenSetCaseRow>>
pub async fn publish_version(pool, tid, gs_id, uid, bump_type) -> Result<EntityVersionRow>
// publish: 校验 status==draft → 序列化 cases 全文 → SHA-256 → INSERT entity_version(locked) 
//          → UPDATE golden_sets.active_version_id+status=locked
```

### Rubric 管理
```rust
pub async fn create_rubric(pool, tid, uid, req) -> Result<RubricRow>
pub async fn list_rubrics(pool, tid) -> Result<Vec<RubricRow>>
pub async fn publish_rubric_version(pool, tid, rb_id, uid, bump_type, criteria_json) -> Result<EntityVersionRow>
// criteria 存 entity_version.content_ref（[{name, weight, tolerance}, ...]）
```

### 批量评测运行（核心）
```rust
pub async fn start_run(pool, tid, req: StartRunReq, base_url, auth_token) -> Result<i64>
// StartRunReq { golden_set_id, rubric_id?, thread_id, trigger_type? }
// 1. 查 golden_set.active_version_id → 加载 locked entity_version（不可变）
// 2. 查 rubric.active_version_id（可选）
// 3. 构造 Manifest JSON → snapshot_hash = SHA-256
// 4. INSERT eval_batch_runs(running, snapshot_manifest, snapshot_hash, ...)
// 5. 逐 case：HTTP self-call POST /v1/threads/{tid}/turns → 轮询完成 → GET items → 提取 agent_output
//    → score_case(rubric, output) → INSERT eval_case_results
// 6. 聚合 → UPDATE eval_batch_runs(completed, aggregate)
```

**HTTP self-call 复用 M18 模式**：用 reqwest 调 localhost /v1/threads/{id}/turns + GET /v1/threads/{id}/items，service JWT（jwt.issue(claims)）。

### 评分器（确定性规则层）
```rust
pub fn score_case(expected: &Expected, output: &str) -> CaseScores
// expected = expected_json 反序列化
// 1. must_hit_points: 逐点检查 output.contains(point) → [{point, hit:bool}]
// 2. must_not: 逐项检查 output.contains(forbidden) → violations: Vec<String>
// 3. acceptable_answers: output 在可接受答案集合内 → bool
// 4. total_score = hit_count/total_points * 100; passed = must_not.is_empty() && acceptable_match
pub struct CaseScores {
    must_hit_points: Vec<PointHit>,   // [{point, hit}]
    must_not_violations: Vec<String>,
    acceptable_match: bool,
    total_score: f64,                 // 0-100
    passed: bool,
}
```

### 基线对比
```rust
pub async fn compare_runs(pool, tid, run_id, baseline_id) -> Result<CompareResult>
// 1. 查两 run 的 golden_set_version_id → 不同则标记 incomparable=true
// 2. 同版本：按 case_key JOIN 两次 eval_case_results → case 级 diff（pass↔fail 变化）
// 3. 返回 {incomparable, baseline_version, run_version, diffs: [{case_key, baseline_passed, run_passed, delta}]}
```

## http_server.rs 路由（~12 新路由）

```rust
// GoldenSet
.route("/v1/golden-sets", post(gs_create).get(gs_list))
.route("/v1/golden-sets/{id}", get(gs_get))
.route("/v1/golden-sets/{id}/cases", post(gs_add_case).get(gs_list_cases))
.route("/v1/golden-sets/{id}/publish", post(gs_publish))
// Rubric
.route("/v1/rubrics", post(rb_create).get(rb_list))
.route("/v1/rubrics/{id}/publish", post(rb_publish))
// 评测运行
.route("/v1/evals/runs", post(eval_batch_start).get(eval_batch_list))
.route("/v1/evals/runs/{id}", get(eval_batch_get))
.route("/v1/evals/runs/{id}/compare", post(eval_batch_compare))
```

AppState 加 `base_url` 字段已存在（M18），无需新增。

## 接线

- `db.rs`：`const M20_MIGRATION_SQL = include_str!("../migrations/20260906000014_m20_eval_center.sql");` + `run_migrations` 末尾执行
- `lib.rs`：`pub mod eval_center;`
- `http_server.rs`：`use crate::eval_center;` + 12 路由 + handler
- `main.rs`：标题 "Nexus M20: serve"

## Web 控制台

新增 2 页（复用 web-console 既有 ui.tsx 原语 + archify 设计语言）：
1. **GoldenSets 页**：列表 + 创建 + 查看 case + 发布版本（semver + bump_type）
2. **EvalRuns 页**：起 run（选 golden_set + rubric + thread）+ 查看 manifest + 逐 case 得分 + 基线对比 diff

## SIMULATE 模式

无真实模型时（NEXUS_SIMULATE_APPROVAL=1 或无 NEXUS_MODEL_KEY），turn 走 mock gateway 返回固定文本。评分仍可验证（must_hit/must_not 命中逻辑不依赖真实模型）。e2e 测试用 SIMULATE 模式跑通全链路。

## 依赖

- `sha2` 已在 workspace（eval_center 用 SHA-256 算 manifest_hash）——需确认，若无则用 std DefaultHasher（M14 fork.rs 先例）先验证 sha2：
- `reqwest` 已在 workspace（M18 orchestrator 用）

## 风险与对策

| 风险 | 对策 |
|---|---|
| HTTP self-call turn hang（审批 park） | SIMULATE 模式 + 500ms 轮询 approve（复用 M18 run_agent_turn） |
| batch run N case 串行慢 | MVP 接受串行（M21 加并发），或限制 case 数 ≤20 |
| content_ref 大文本存 PG | M20 接受（case 量级小），M22 迁对象存储 |
| locked 版本被改 | 应用层不提供 UPDATE（只 INSERT 新版本）+ golden_sets.status=locked 后拒绝 add_case |

## 验证策略

1. `cargo check` 0 error 0 warning
2. `cargo test` 零回归 + 新增 score_case 单测
3. Docker 部署 e2e：SIMULATE 模式全链路（create gs → add cases → publish → create rubric → publish → start run → score → report → compare）
4. Web 控制台截图
5. 零回归：M12 eval 旧 API + M14 fork + M10 audit 不退化
