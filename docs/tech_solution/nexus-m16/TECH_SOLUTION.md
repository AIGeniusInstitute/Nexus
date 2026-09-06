# 技术方案 — Nexus M16 连接器生态市场

## 1. 现状
- `connectors(id/tenant_id/name/kind/cred_ref/config_json/created_at)` — 无 tier/status/quality/contributor/description/updated_at
- `tool_call_logs(id/thread_id/turn_id/tool_name/args_json/result_ref/created_at)` — 无 success/connector_id
- lib.rs 无 `connectors` module；http_server.rs 无 `/v1/connectors` 路由
- `check_permission(&perms, resource, action)`：`*:*` admin、`threads:*`、`threads:read` 三级
- `AuthUser(c).tid/.uid`

## 2. 改动（全部增量，不碰 turn_start/drain/runtime）

### T16-1 migration `20260906000011_m16_connector_market.sql`
```sql
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS tier TEXT NOT NULL DEFAULT 'community'
  CHECK(tier IN ('official','enterprise','community'));
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'draft'
  CHECK(status IN ('draft','published','offline'));
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS contributor_user_id BIGINT REFERENCES users(id);
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS quality_score REAL NOT NULL DEFAULT 0;
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
CREATE INDEX IF NOT EXISTS idx_connectors_tenant_status ON connectors(tenant_id, status);
-- 质量分数据源
ALTER TABLE tool_call_logs ADD COLUMN IF NOT EXISTS success BOOLEAN;
ALTER TABLE tool_call_logs ADD COLUMN IF NOT EXISTS connector_id BIGINT REFERENCES connectors(id);
CREATE INDEX IF NOT EXISTS idx_toolcall_connector ON tool_call_logs(connector_id) WHERE connector_id IS NOT NULL;
```
db.rs 接线 raw_sql。

### T16-2 `connectors.rs`（新）
```rust
pub struct ConnectorRow { id, tenant_id, name, kind, tier, status, quality_score, contributor_user_id, description, cred_ref, config_json, created_at, updated_at } // FromRow+Serialize
pub async fn create_connector(pool, tid, uid, req) -> Result<ConnectorRow>
pub async fn list_connectors(pool, tid, status_filter: Option<&str>) -> Result<Vec<ConnectorRow>>
pub async fn get_connector(pool, tid, id) -> Result<ConnectorRow>
pub async fn update_connector(pool, tid, id, req) -> Result<ConnectorRow>  // name/kind/description/tier/config_json
pub async fn set_status(pool, tid, id, new_status) -> Result<ConnectorRow>  // draft→published / published→offline
pub async fn delete_connector(pool, tid, id) -> Result<()>  // 有 tool_call_logs→anyhow!("in_use")
pub async fn compute_quality(pool, tid, id) -> Result<f64>  // UPDATE quality_score + 返回
pub async fn invoke_stub(pool, tid, id, tool, args_json, success: bool) -> Result<i64>  // INSERT tool_call_logs RETURNING id
pub async fn list_calls(pool, tid, connector_id, limit) -> Vec<ToolCallRow>
```
单测 `quality_formula`（3 断言：0/0→0.0、2/3→0.667、3/3→1.0 用 Rust 算模拟）。

### T16-3 `http_server.rs` 路由
```
POST   /v1/connectors                      → connector_create (本租户)
GET    /v1/connectors                      → connector_list (本租户, ?status=)
GET    /v1/connectors/{id}                 → connector_get
PUT    /v1/connectors/{id}                 → connector_update
DELETE /v1/connectors/{id}                 → connector_delete
POST   /v1/connectors/{id}/publish         → connector_publish (admin *:*)
POST   /v1/connectors/{id}/offline         → connector_offline (admin *:*)
GET    /v1/connectors/{id}/quality        → connector_quality (重算+返回)
POST   /v1/connectors/{id}/invoke          → connector_invoke (body: tool, args, success?)
GET    /v1/connectors/{id}/calls           → connector_calls (?limit=)
```
admin 判 `rbac::check_permission(&c.perms, "connectors", "publish") || check_permission(&c.perms, "*", "*")`。

### T16-4 lib.rs + main.rs
- lib.rs 加 `pub mod connectors;`
- main.rs 标题 "Nexus M16: serve"
- migration 文件加入 db.rs raw_sql 序列

## 3. 关键决策
1. **纯增量**（Simplicity First）：不重构 turn_start，不接真实 MCP，invoke stub 记 tool_call_logs 验证链路+质量分数据源。
2. **删除约束**：有 tool_call_logs 的连接器拒绝删除（409），保留审计 trail。
3. **质量分公式**：`COUNT FILTER(WHERE success) / NULLIF(COUNT(*),0)`，PG NULLIF(total=0)→NULL→coalesce 0.0。
4. **publish/offline 治理权**：需 admin（`*:*`），目录 CRUD 本租户即可（贡献者提交 draft，admin 审批 publish）。
5. **跨租户隔离**：所有查询 `WHERE tenant_id=$X`，invoke/get/calls/quality/delete 全部校验本租户。

## 4. 验证
- AC1-7：CRUD + 分级 + 状态流转 + invoke + quality + 删除约束
- AC8：零回归（SIMULATE turn + 审批 + 计量 + KB + fork）
- cargo test +31（M15 31 + connectors 1）
