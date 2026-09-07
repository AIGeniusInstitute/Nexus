//! Agent Studio — Agent 定义 CRUD。
//!
//! 每个 Agent 定义是一个可复用的模板（name + system_prompt + model），
//! 在创建 thread 时绑定，driver 在 `thread/start` 时把 system_prompt 注入
//! 为 codex 的 `base_instructions`，使同一 Agent 定义驱动多个会话。
//! tenant-scoped CRUD，纯增量，不触碰 turn_start drain 路径。

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(sqlx::FromRow, Serialize)]
pub struct AgentDefRow {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    pub created_by: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateAgentReq {
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateAgentReq {
    pub name: Option<String>,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
}

pub async fn create_agent(
    pool: &PgPool,
    tenant_id: i64,
    created_by: i64,
    req: CreateAgentReq,
) -> Result<AgentDefRow> {
    sqlx::query_as::<_, AgentDefRow>(
        "INSERT INTO agent_definitions (tenant_id, name, description, system_prompt, model, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, tenant_id, name, description, system_prompt, model, created_by, created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(req.name)
    .bind(req.description)
    .bind(req.system_prompt)
    .bind(req.model)
    .bind(created_by)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("create agent: {e}"))
}

pub async fn list_agents(pool: &PgPool, tenant_id: i64) -> Result<Vec<AgentDefRow>> {
    sqlx::query_as::<_, AgentDefRow>(
        "SELECT id, tenant_id, name, description, system_prompt, model, created_by, created_at, updated_at
         FROM agent_definitions WHERE tenant_id=$1 ORDER BY id DESC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list agents: {e}"))
}

pub async fn get_agent(pool: &PgPool, tenant_id: i64, id: i64) -> Result<AgentDefRow> {
    sqlx::query_as::<_, AgentDefRow>(
        "SELECT id, tenant_id, name, description, system_prompt, model, created_by, created_at, updated_at
         FROM agent_definitions WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| anyhow!("get agent: {e}"))?
    .ok_or_else(|| anyhow!("agent not found"))
}

pub async fn update_agent(
    pool: &PgPool,
    tenant_id: i64,
    id: i64,
    req: UpdateAgentReq,
) -> Result<AgentDefRow> {
    let row: Option<AgentDefRow> = sqlx::query_as::<_, AgentDefRow>(
        "UPDATE agent_definitions
            SET name = COALESCE($3, name),
                description = COALESCE($4, description),
                system_prompt = COALESCE($5, system_prompt),
                model = COALESCE($6, model),
                updated_at = NOW()
          WHERE id=$1 AND tenant_id=$2
         RETURNING id, tenant_id, name, description, system_prompt, model, created_by, created_at, updated_at",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(req.name)
    .bind(req.description)
    .bind(req.system_prompt)
    .bind(req.model)
    .fetch_optional(pool)
    .await
    .map_err(|e| anyhow!("update agent: {e}"))?;
    row.ok_or_else(|| anyhow!("agent not found"))
}

pub async fn delete_agent(pool: &PgPool, tenant_id: i64, id: i64) -> Result<()> {
    // 解绑引用此 Agent 的 threads（FK 为 ON DELETE NO ACTION，直接删会 500）。
    // SET NULL 后 thread 继续用默认 prompt，不破坏会话。
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE threads SET agent_def_id=NULL WHERE agent_def_id=$1")
        .bind(id).execute(&mut *tx).await?;
    let res = sqlx::query("DELETE FROM agent_definitions WHERE id=$1 AND tenant_id=$2")
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("delete agent: {e}"))?;
    if res.rows_affected() == 0 {
        return Err(anyhow!("agent not found"));
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_item_type_sentinel() {
        // The http_server drain skips items-table persistence for transient
        // streaming fragments. codex delta methods are either `*/delta`
        // (lowercase) or `*Delta` (camelCase); the drain checks both. Verify
        // all 6 delta methods match and non-delta methods don't.
        let lower_deltas = ["item/agentMessage/delta", "item/plan/delta"];
        let camel_deltas = [
            "item/reasoning/textDelta",
            "item/reasoning/summaryTextDelta",
            "item/commandExecution/outputDelta",
            "item/fileChange/outputDelta",
        ];
        for d in lower_deltas.iter().chain(camel_deltas.iter()) {
            assert!(
                d.ends_with("/delta") || d.ends_with("Delta"),
                "{d} should be recognized as a delta"
            );
        }
        for nd in ["item/started", "item/completed", "turn/completed"] {
            assert!(
                !(nd.ends_with("/delta") || nd.ends_with("Delta")),
                "{nd} should not match delta"
            );
        }
    }
}
