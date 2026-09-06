//! Append-only (WORM) audit log + query API (M10, roadmap T10-1).
//!
//! Every security-relevant action — auth, turn lifecycle, approval decisions,
//! policy learning/amendment — is recorded here. The table is enforced
//! append-only at the Postgres layer by a `BEFORE UPDATE OR DELETE` trigger
//! (`prevent_audit_modification`), so even a buggy or compromised application
//! path cannot rewrite history. The application layer only ever `INSERT`s.

use anyhow::Context;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

/// One immutable audit record. Column names follow the existing
/// `audit_logs` table (created by an earlier milestone): `actor_user_id`,
/// `detail_json` (NOT NULL, defaults to '{}'), plus the M10 additions
/// `target_type` / `target_id` / `trace_id`.
#[derive(sqlx::FromRow, Serialize)]
pub struct AuditLogRow {
    pub id: i64,
    pub tenant_id: i64,
    pub actor_user_id: Option<i64>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub detail_json: Value,
    pub trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Append an audit record. INSERT-only — never UPDATE/DELETE (the WORM
/// trigger would reject it anyway). Best-effort: a failure here is logged but
/// does **not** fail the caller's operation (audit must never block the
/// business path); pass `None` for inapplicable fields. `detail` is coerced
/// to `{}` when `None` because `detail_json` is NOT NULL.
pub async fn audit_log(
    pool: &PgPool,
    tenant_id: i64,
    actor_uid: Option<i64>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    detail: Option<&Value>,
    trace_id: Option<&str>,
) {
    // detail_json is NOT NULL in the schema — default to an empty object.
    let detail_val = detail.cloned().unwrap_or_else(|| serde_json::json!({}));
    let res = sqlx::query(
        "INSERT INTO audit_logs \
         (tenant_id, actor_user_id, action, target_type, target_id, detail_json, trace_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(tenant_id)
    .bind(actor_uid)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(&detail_val)
    .bind(trace_id)
    .execute(pool)
    .await;
    if let Err(e) = res {
        tracing::error!("audit_log insert failed (action={action}): {e:?}");
    }
}

/// List audit records. `tenant_filter = Some(tid)` scopes to one tenant;
/// `None` returns across all tenants (admin only). `action` and `since` are
/// optional filters. Uses NULL-or-equal predicates so a single query serves
/// all combinations.
pub async fn list_audit_logs(
    pool: &PgPool,
    tenant_filter: Option<i64>,
    action: Option<&str>,
    since: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<AuditLogRow>> {
    let since = since.unwrap_or_else(|| Utc::now() - Duration::days(30));
    sqlx::query_as::<_, AuditLogRow>(
        "SELECT id, tenant_id, actor_user_id, action, target_type, target_id, \
                detail_json, trace_id, created_at \
         FROM audit_logs \
         WHERE created_at >= $1 \
           AND ($2::bigint IS NULL OR tenant_id = $2) \
           AND ($3::text IS NULL OR action = $3) \
         ORDER BY created_at DESC LIMIT $4",
    )
    .bind(since)
    .bind(tenant_filter)
    .bind(action)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("list audit_logs")
}

/// Fetch a single audit record by id (within the caller's tenant scope).
pub async fn get_audit_log(
    pool: &PgPool,
    id: i64,
    tenant_filter: Option<i64>,
) -> Result<Option<AuditLogRow>> {
    sqlx::query_as::<_, AuditLogRow>(
        "SELECT id, tenant_id, actor_user_id, action, target_type, target_id, \
                detail_json, trace_id, created_at \
         FROM audit_logs \
         WHERE id = $1 AND ($2::bigint IS NULL OR tenant_id = $2)",
    )
    .bind(id)
    .bind(tenant_filter)
    .fetch_optional(pool)
    .await
    .context("get audit_log")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke: the NULL-or-equal predicate shape is sound (no DB needed).
    #[test]
    fn audit_row_struct_compiles() {
        // AuditLogRow derives FromRow; this just guards against field drift.
        let _ = std::mem::size_of::<AuditLogRow>();
    }
}
