//! M17 Skills 市场 — roadmap T12-3。
//!
//! 企业 Skill 发布/版本/回滚治理层：发布版本快照（version+checksum+content_ref）、
//! 激活版本（active_version_id）、回滚到历史版本。纯增量，不触碰核心路径。

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(sqlx::FromRow, Serialize)]
pub struct SkillRow {
    pub id: i64,
    pub tenant_id: Option<i64>,
    pub scope: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub owner_user_id: Option<i64>,
    pub active_version_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct SkillVersionRow {
    pub id: i64,
    pub skill_id: i64,
    pub version: String,
    pub checksum: String,
    pub content_ref: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateSkillReq {
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
pub struct PublishVersionReq {
    pub version: String,
    pub checksum: String,
    pub content_ref: String,
}

#[derive(Deserialize)]
pub struct RollbackReq {
    pub version_id: i64,
}

pub async fn create_skill(
    pool: &PgPool,
    tenant_id: i64,
    owner_user_id: i64,
    req: CreateSkillReq,
) -> Result<SkillRow> {
    let scope = req.scope.unwrap_or_else(|| "tenant".into());
    sqlx::query_as::<_, SkillRow>(
        "INSERT INTO skills (tenant_id, scope, name, description, status, owner_user_id)
         VALUES ($1, $2, $3, $4, 'draft', $5)
         RETURNING id, tenant_id, scope, name, description, status, owner_user_id, active_version_id, created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(scope)
    .bind(req.name)
    .bind(req.description)
    .bind(owner_user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("create_skill: {e:?}"))
}

pub async fn list_skills(
    pool: &PgPool,
    tenant_id: i64,
    status_filter: Option<&str>,
) -> Result<Vec<SkillRow>> {
    sqlx::query_as::<_, SkillRow>(
        "SELECT id, tenant_id, scope, name, description, status, owner_user_id, active_version_id, created_at, updated_at
         FROM skills WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2)
         ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .bind(status_filter)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_skills: {e:?}"))
}

pub async fn get_skill(pool: &PgPool, tenant_id: i64, id: i64) -> Result<SkillRow> {
    sqlx::query_as::<_, SkillRow>(
        "SELECT id, tenant_id, scope, name, description, status, owner_user_id, active_version_id, created_at, updated_at
         FROM skills WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("get_skill: {e:?}"))
}

/// 发布版本：INSERT skill_versions + UPDATE skills.active_version_id + status=published。
pub async fn publish_version(
    pool: &PgPool,
    tenant_id: i64,
    skill_id: i64,
    req: PublishVersionReq,
) -> Result<SkillVersionRow> {
    let mut tx = pool.begin().await.map_err(|e| anyhow!("tx: {e:?}"))?;
    let row: SkillVersionRow = sqlx::query_as::<_, SkillVersionRow>(
        "INSERT INTO skill_versions (skill_id, version, checksum, content_ref)
         VALUES ($1, $2, $3, $4)
         RETURNING id, skill_id, version, checksum, content_ref, created_at",
    )
    .bind(skill_id)
    .bind(req.version)
    .bind(req.checksum)
    .bind(req.content_ref)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| anyhow!("publish_version insert: {e:?}"))?;
    sqlx::query("UPDATE skills SET active_version_id = $3, status = 'published', updated_at = NOW()
                 WHERE id = $1 AND tenant_id = $2")
        .bind(skill_id)
        .bind(tenant_id)
        .bind(row.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("publish_version update: {e:?}"))?;
    tx.commit().await.map_err(|e| anyhow!("commit: {e:?}"))?;
    Ok(row)
}

pub async fn list_versions(pool: &PgPool, tenant_id: i64, skill_id: i64) -> Result<Vec<SkillVersionRow>> {
    let _ = get_skill(pool, tenant_id, skill_id).await?;
    sqlx::query_as::<_, SkillVersionRow>(
        "SELECT id, skill_id, version, checksum, content_ref, created_at
         FROM skill_versions WHERE skill_id = $1 ORDER BY created_at DESC",
    )
    .bind(skill_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_versions: {e:?}"))
}

/// 回滚：激活某历史版本（更新 active_version_id）。不删版本。
pub async fn rollback(
    pool: &PgPool,
    tenant_id: i64,
    skill_id: i64,
    version_id: i64,
) -> Result<SkillRow> {
    let _ = get_skill(pool, tenant_id, skill_id).await?;
    sqlx::query_as::<_, SkillRow>(
        "UPDATE skills SET active_version_id = $3, updated_at = NOW()
         WHERE id = $1 AND tenant_id = $2
         RETURNING id, tenant_id, scope, name, description, status, owner_user_id, active_version_id, created_at, updated_at",
    )
    .bind(skill_id)
    .bind(tenant_id)
    .bind(version_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("rollback: {e:?}"))
}

/// 删除：有 versions 则拒绝（保留版本历史）。
pub async fn delete_skill(pool: &PgPool, tenant_id: i64, id: i64) -> Result<()> {
    let _ = get_skill(pool, tenant_id, id).await?;
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM skill_versions WHERE skill_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow!("delete_skill count: {e:?}"))?;
    if count > 0 {
        return Err(anyhow!("skill in_use: {count} versions exist"));
    }
    sqlx::query("DELETE FROM skills WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .map_err(|e| anyhow!("delete_skill: {e:?}"))?;
    Ok(())
}
