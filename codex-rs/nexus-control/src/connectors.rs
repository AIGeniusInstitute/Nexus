//! M16 连接器生态市场 — roadmap T12-2。
//!
//! 连接器目录的元数据 + 治理层：社区贡献（draft→published）、分级标签
//!（official/enterprise/community）、质量分（基于 tool_call_logs 成功率）、
//! 上下线状态、调用代理骨架（stub 记录 intent，真实 MCP 转发留 T7-1）。
//!
//! 纯增量模块，不触碰 turn_start / drain / runtime。租户隔离：所有查询
//! `WHERE tenant_id=$X`；publish/offline 治理动作需 admin（`*:*`）。

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::PgPool;
use std::collections::HashMap;

#[derive(sqlx::FromRow, Serialize)]
pub struct ConnectorRow {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub kind: String,
    pub tier: String,
    pub status: String,
    pub quality_score: f32,
    pub contributor_user_id: Option<i64>,
    pub description: Option<String>,
    pub cred_ref: Option<String>,
    pub config_json: Json,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct ToolCallRow {
    pub id: i64,
    pub connector_id: Option<i64>,
    pub tool_name: String,
    pub args_json: Json,
    pub result_ref: Option<String>,
    pub success: Option<bool>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct CreateReq {
    pub name: String,
    pub kind: String,
    pub tier: Option<String>,       // default community
    pub description: Option<String>,
    pub cred_ref: Option<String>,
    pub config_json: Option<Json>,
}

#[derive(Deserialize)]
pub struct UpdateReq {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub tier: Option<String>,
    pub description: Option<String>,
    pub config_json: Option<Json>,
}

#[derive(Deserialize)]
pub struct InvokeReq {
    pub tool: String,
    pub args: Option<Json>,
    pub success: Option<bool>, // stub 默认 true
}

/// 质量分公式：success/total。total=0 → 0.0。
/// 供单测验证（不依赖 PG）。
fn quality_formula(success: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        success as f64 / total as f64
    }
}

pub async fn create_connector(
    pool: &PgPool,
    tenant_id: i64,
    contributor_user_id: i64,
    req: CreateReq,
) -> Result<ConnectorRow> {
    let tier = req.tier.unwrap_or_else(|| "community".into());
    let cfg = req.config_json.unwrap_or(Json::Object(serde_json::Map::new()));
    sqlx::query_as::<_, ConnectorRow>(
        "INSERT INTO connectors (tenant_id, name, kind, tier, status, contributor_user_id, description, cred_ref, config_json)
         VALUES ($1, $2, $3, $4, 'draft', $5, $6, $7, $8)
         RETURNING id, tenant_id, name, kind, tier, status, quality_score, contributor_user_id, description, cred_ref, config_json, created_at, updated_at",
    )
    .bind(tenant_id)
    .bind(req.name)
    .bind(req.kind)
    .bind(tier)
    .bind(contributor_user_id)
    .bind(req.description)
    .bind(req.cred_ref)
    .bind(cfg)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("create_connector: {e:?}"))
}

pub async fn list_connectors(
    pool: &PgPool,
    tenant_id: i64,
    status_filter: Option<&str>,
) -> Result<Vec<ConnectorRow>> {
    // NULL-or-equal 单查询覆盖有无过滤
    sqlx::query_as::<_, ConnectorRow>(
        "SELECT id, tenant_id, name, kind, tier, status, quality_score, contributor_user_id, description, cred_ref, config_json, created_at, updated_at
         FROM connectors
         WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2)
         ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .bind(status_filter)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_connectors: {e:?}"))
}

pub async fn get_connector(pool: &PgPool, tenant_id: i64, id: i64) -> Result<ConnectorRow> {
    sqlx::query_as::<_, ConnectorRow>(
        "SELECT id, tenant_id, name, kind, tier, status, quality_score, contributor_user_id, description, cred_ref, config_json, created_at, updated_at
         FROM connectors WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("get_connector: {e:?}"))
}

pub async fn update_connector(
    pool: &PgPool,
    tenant_id: i64,
    id: i64,
    req: UpdateReq,
) -> Result<ConnectorRow> {
    // COALESCE 保留未提供字段
    sqlx::query_as::<_, ConnectorRow>(
        "UPDATE connectors SET
           name = COALESCE($3, name),
           kind = COALESCE($4, kind),
           tier = COALESCE($5, tier),
           description = COALESCE($6, description),
           config_json = COALESCE($7, config_json),
           updated_at = NOW()
         WHERE id = $1 AND tenant_id = $2
         RETURNING id, tenant_id, name, kind, tier, status, quality_score, contributor_user_id, description, cred_ref, config_json, created_at, updated_at",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(req.name)
    .bind(req.kind)
    .bind(req.tier)
    .bind(req.description)
    .bind(req.config_json)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("update_connector: {e:?}"))
}

/// 状态流转：draft→published / published→offline。校验前置状态。
pub async fn set_status(
    pool: &PgPool,
    tenant_id: i64,
    id: i64,
    new_status: &str,
) -> Result<ConnectorRow> {
    let row = get_connector(pool, tenant_id, id).await?;
    let ok = match (row.status.as_str(), new_status) {
        ("draft", "published") => true,
        ("published", "offline") => true,
        _ => false,
    };
    if !ok {
        return Err(anyhow!(
            "invalid transition: {} -> {} (allowed: draft->published, published->offline)",
            row.status,
            new_status
        ));
    }
    sqlx::query_as::<_, ConnectorRow>(
        "UPDATE connectors SET status = $3, updated_at = NOW()
         WHERE id = $1 AND tenant_id = $2
         RETURNING id, tenant_id, name, kind, tier, status, quality_score, contributor_user_id, description, cred_ref, config_json, created_at, updated_at",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(new_status)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("set_status: {e:?}"))
}

/// 删除：有 tool_call_logs 关联则拒绝（保留审计 trail）。
pub async fn delete_connector(pool: &PgPool, tenant_id: i64, id: i64) -> Result<()> {
    // 先校验租户归属（不存在→404 语义）
    let _ = get_connector(pool, tenant_id, id).await?;
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tool_call_logs WHERE connector_id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("delete_connector count: {e:?}"))?;
    if count > 0 {
        return Err(anyhow!("connector in_use: {count} tool_call_logs exist"));
    }
    sqlx::query("DELETE FROM connectors WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .map_err(|e| anyhow!("delete_connector: {e:?}"))?;
    Ok(())
}

/// 重算并落库质量分（success/total）。total=0 → 0.0。
pub async fn compute_quality(pool: &PgPool, tenant_id: i64, id: i64) -> Result<f64> {
    let _ = get_connector(pool, tenant_id, id).await?;
    let (success, total): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE success = true),
                COUNT(*)
         FROM tool_call_logs WHERE connector_id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("compute_quality: {e:?}"))?;
    let score = quality_formula(success as u64, total as u64);
    sqlx::query("UPDATE connectors SET quality_score = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant_id)
        .bind(score as f32)
        .execute(pool)
        .await
        .map_err(|e| anyhow!("compute_quality update: {e:?}"))?;
    Ok(score)
}

/// stub 调用代理：记录调用 intent 到 tool_call_logs（success 默认 true）。
/// 真实 MCP 转发留 T7-1；此处验证调用链路 + 质量分数据源。
pub async fn invoke_stub(
    pool: &PgPool,
    tenant_id: i64,
    id: i64,
    req: InvokeReq,
) -> Result<i64> {
    let _ = get_connector(pool, tenant_id, id).await?;
    let args = req.args.unwrap_or(Json::Object(serde_json::Map::new()));
    let success = req.success.unwrap_or(true);
    let result_ref = if success { "stub:ok" } else { "stub:fail" };
    let (row_id,): (i64,) = sqlx::query_as(
        "INSERT INTO tool_call_logs (connector_id, tool_name, args_json, result_ref, success)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(id)
    .bind(req.tool)
    .bind(args)
    .bind(result_ref)
    .bind(success)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("invoke_stub: {e:?}"))?;
    Ok(row_id)
}

/// M19: 真实 MCP 转发。从 connector.config_json 解析 {command,args,env,cwd}，
/// spawn MCP server 子进程，initialize → call_tool，结果落 tool_call_logs。
/// config_json 缺 command 字段 → 回退 invoke_stub 语义（便于旧 connector）。
pub async fn invoke_mcp(
    pool: &PgPool,
    tenant_id: i64,
    id: i64,
    req: InvokeReq,
) -> Result<McpInvokeResult> {
    let conn = get_connector(pool, tenant_id, id).await?;
    let args = req.args.unwrap_or(Json::Object(serde_json::Map::new()));

    // 解析 config_json：{command, args, env, cwd}
    let command = conn.config_json.get("command").and_then(|v| v.as_str());
    if command.is_none() {
        // 旧 connector 无 command → stub 回退
        let success = req.success.unwrap_or(true);
        let result_ref = if success { "stub:ok" } else { "stub:fail" };
        let (row_id,): (i64,) = sqlx::query_as(
            "INSERT INTO tool_call_logs (connector_id, tool_name, args_json, result_ref, success)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(id).bind(&req.tool).bind(&args).bind(result_ref).bind(success)
        .fetch_one(pool).await
        .map_err(|e| anyhow!("invoke_mcp stub fallback: {e:?}"))?;
        return Ok(McpInvokeResult { call_id: row_id, success, result: format!("{{stub:{}}}", result_ref), mcp: false });
    }
    let command = command.unwrap();
    let tool_args: Vec<String> = conn.config_json.get("args")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let env_map: HashMap<String, String> = conn.config_json.get("env")
        .and_then(|v| v.as_object())
        .map(|o| o.iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect())
        .unwrap_or_default();
    let cwd = conn.config_json.get("cwd").and_then(|v| v.as_str());

    // spawn + initialize + call_tool
    let mut client = crate::mcp::McpClient::spawn(command, &tool_args, &env_map, cwd)?;
    client.initialize().await
        .map_err(|e| anyhow!("mcp initialize: {e}"))?;
    let result = crate::mcp::call_tool_with_timeout(
        &mut client, &req.tool, Some(args.clone()),
        std::time::Duration::from_secs(30),
    ).await;
    // shutdown（无论成功失败都关进程）
    client.shutdown().await;

    let (success, result_ref) = match &result {
        Ok(r) => {
            let ok = !r.is_error;
            (ok, serde_json::to_string(&r.content).unwrap_or_else(|_| "mcp:ok".into()))
        }
        Err(e) => (false, format!("mcp_error:{e}")),
    };

    let (row_id,): (i64,) = sqlx::query_as(
        "INSERT INTO tool_call_logs (connector_id, tool_name, args_json, result_ref, success)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(id).bind(&req.tool).bind(&args).bind(&result_ref).bind(success)
    .fetch_one(pool).await
    .map_err(|e| anyhow!("invoke_mcp insert: {e:?}"))?;

    // 重算质量分（best-effort）
    let _ = compute_quality(pool, tenant_id, id).await;

    Ok(McpInvokeResult {
        call_id: row_id,
        success,
        result: result_ref,
        mcp: true,
    })
}

#[derive(serde::Serialize)]
pub struct McpInvokeResult {
    pub call_id: i64,
    pub success: bool,
    pub result: String,
    pub mcp: bool,
}

pub async fn list_calls(
    pool: &PgPool,
    tenant_id: i64,
    connector_id: i64,
    limit: i64,
) -> Result<Vec<ToolCallRow>> {
    // 校验租户归属
    let _ = get_connector(pool, tenant_id, connector_id).await?;
    sqlx::query_as::<_, ToolCallRow>(
        "SELECT id, connector_id, tool_name, args_json, result_ref, success, created_at
         FROM tool_call_logs WHERE connector_id = $1
         ORDER BY created_at DESC LIMIT $2",
    )
    .bind(connector_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_calls: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::quality_formula;

    #[test]
    fn quality_formula_cases() {
        assert_eq!(quality_formula(0, 0), 0.0);
        assert!((quality_formula(2, 3) - 0.6666666666666666).abs() < 1e-9);
        assert_eq!(quality_formula(3, 3), 1.0);
        assert_eq!(quality_formula(0, 5), 0.0); // 全失败
    }
}
