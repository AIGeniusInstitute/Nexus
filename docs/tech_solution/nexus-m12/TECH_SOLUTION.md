# Nexus M12 技术方案 — 评测中心 + CI 门禁

## 数据流
```
admin 定义 case → eval_cases
用户/CI 起 turn → /v1/threads/{id}/turns (M2)
POST /v1/evals/runs/{case_id} {turn_id}
  → 查 case + turn(status) + items(content_ref)
  → 断言 expected_status==turn.status && expected_contains∈items
  → INSERT eval_runs (passed, detail)
GET /v1/evals/runs → 结果
scripts/eval-gate.sh → 循环 case → exit 0/1
```

## 表结构
```sql
CREATE TABLE eval_cases (
  id BIGSERIAL PK, tenant_id BIGINT NOT NULL REFERENCES tenants(id),
  name TEXT NOT NULL, category TEXT, input TEXT NOT NULL,
  expected_status TEXT NOT NULL DEFAULT 'completed',
  expected_contains TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE eval_runs (
  id BIGSERIAL PK, tenant_id BIGINT NOT NULL REFERENCES tenants(id),
  case_id BIGINT NOT NULL REFERENCES eval_cases(id),
  turn_id BIGINT NOT NULL, passed BOOLEAN NOT NULL,
  detail JSONB NOT NULL DEFAULT '{}', created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

## 修改点
1. `migrations/20260906000008_m12_eval.sql`（新）：eval_cases + eval_runs + 索引
2. `db.rs`：M12_MIGRATION_SQL
3. `eval.rs`（新）：EvalCase/EvalRun(FromRow+Serialize) + create_case/list_cases/run_eval/list_runs
4. `http_server.rs`：4 路由
5. `main.rs`：标题 "Nexus M12: serve"
6. `scripts/eval-gate.sh`（新）：CI 门禁
7. `lib.rs`：pub mod eval

## 简化决策
1. eval 不自动起 turn（接收 turn_id 断言）——起 turn 用既有 /turns，职责分离，避免复刻 turn_start drain 逻辑
2. 断言 status + contains（骨架）——五评测平面留扩展（case.category 字段供分类扩展）
3. run_eval 查 turn 时验证 tenant（turn JOIN threads tenant_id），跨租户隔离
4. CI 脚本用 curl + python，SIMULATE 模式可跑

## 测试
- 单测：run_eval 断言逻辑（mock case/turn）——纯逻辑可测（构造 expected vs actual）
- e2e：SIMULATE turn completed → eval passed；改 expected_status 不匹配 → failed
- 零回归：M11 trace + M10 audit 不退化
