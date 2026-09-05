//! Postgres pool + migrations + admin seed (T1-7 / T1-1).

use anyhow::{Context, Result};
use bcrypt::hash as bcrypt_hash;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Connect to Postgres and configure pool.
pub async fn connect(database_url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(8)
        .connect(database_url)
        .await
        .context("connect to postgres")
}

/// Embedded initial migration SQL (compile-time include; no `migrate` feature,
/// which would pull in sqlx-sqlite and conflict with codex-state's libsqlite3-sys).
const MIGRATION_SQL: &str = include_str!("../migrations/20260906000001_initial.sql");
const M2_MIGRATION_SQL: &str = include_str!("../migrations/20260906000002_m2_runtime.sql");
const M3_MIGRATION_SQL: &str = include_str!("../migrations/20260906000003_m3_approval.sql");
const M4_MIGRATION_SQL: &str = include_str!("../migrations/20260906000004_m4_metering.sql");

/// Run embedded migrations (idempotent: IF NOT EXISTS / ON CONFLICT). Uses
/// raw SQL (simple-query protocol) so the multi-statement DDL runs in one call
/// and we avoid `sqlx::query`'s `'static` `SqlSafeStr` requirement.
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(pool)
        .await
        .context("run migrations")?;
    sqlx::raw_sql(M2_MIGRATION_SQL)
        .execute(pool)
        .await
        .context("run m2 migrations")?;
    sqlx::raw_sql(M3_MIGRATION_SQL)
        .execute(pool)
        .await
        .context("run m3 migrations")?;
    sqlx::raw_sql(M4_MIGRATION_SQL)
        .execute(pool)
        .await
        .context("run m4 migrations")?;
    tracing::info!("migrations applied (incl. m2, m3, m4)");
    Ok(())
}

/// Seed an admin user for the default tenant; returns the user id.
/// Idempotent: if the user already exists, returns its id without re-hashing.
pub async fn seed_admin(pool: &PgPool, email: &str, password: &str) -> Result<i64> {
    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM users WHERE tenant_id = (SELECT id FROM tenants WHERE slug='default') AND email = $1")
            .bind(email)
            .fetch_optional(pool)
            .await?;
    if let Some((id,)) = existing {
        return Ok(id);
    }
    let pw_hash = bcrypt_hash(password, 12).context("bcrypt hash admin password")?;
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO users (tenant_id, email, password_hash, display_name, status)
         VALUES ((SELECT id FROM tenants WHERE slug='default'), $1, $2, 'admin', 'active')
         RETURNING id",
    )
    .bind(email)
    .bind(pw_hash)
    .fetch_one(pool)
    .await?;
    let uid = row.0;
    // Assign admin role.
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role_id)
         SELECT t.id, $1, r.id FROM tenants t, roles r
         WHERE t.slug='default' AND r.name='admin' AND r.tenant_id=t.id
         ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .execute(pool)
    .await?;
    tracing::info!(uid, "admin user seeded");
    Ok(uid)
}

/// Load the union of permissions_json arrays for a user's roles.
pub async fn user_permissions(pool: &PgPool, user_id: i64) -> Result<Vec<String>> {
    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT COALESCE(r.permissions_json, '[]'::jsonb)
         FROM tenant_memberships m JOIN roles r ON m.role_id = r.id
         WHERE m.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut perms: Vec<String> = Vec::new();
    for (arr,) in rows {
        if let Some(a) = arr.as_array() {
            for v in a {
                if let Some(s) = v.as_str() {
                    perms.push(s.to_string());
                }
            }
        }
    }
    Ok(perms)
}
