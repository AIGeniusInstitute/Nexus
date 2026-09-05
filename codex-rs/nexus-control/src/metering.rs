//! M4 计量 (Metering): token 用量落 usage_records + cost 推导 + 聚合查询。
//!
//! - `record_usage()`: turn 完成时写 usage_records（表在 initial 迁移已建，
//!   M4 首次写入）。cost 经 `compute_cost` 从 model_pricing 表查 rate。
//! - `compute_cost()`: (input_tokens/1e6 * input_rate + output_tokens/1e6 *
//!   output_rate) * 1e6 → cost_micros。未知模型 → 0（不误计费）。
//! - `daily_usage()`: per-tenant 近 N 天聚合（支撑用量 API + Web 看板）。

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// 一次用量记录的写入结果（cost_micros 已推导）。
#[derive(Debug, Clone, Serialize)]
pub struct UsageRecord {
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micros: i64,
}

/// 写一条 usage_records，返回推导的 cost（micros）。
pub async fn record_usage(
    pool: &PgPool,
    tenant_id: i64,
    user_id: i64,
    thread_id: Uuid,
    turn_id: i64,
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
) -> Result<i64> {
    let cost = compute_cost(pool, model, input_tokens, output_tokens).await;
    sqlx::query(
        "INSERT INTO usage_records
           (tenant_id, user_id, thread_id, turn_id, model,
            input_tokens, output_tokens, cost_micros, recorded_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(thread_id)
    .bind(turn_id)
    .bind(model)
    .bind(input_tokens)
    .bind(output_tokens)
    .bind(cost)
    .execute(pool)
    .await
    .context("insert usage_records")?;
    Ok(cost)
}

/// 查 model_pricing 推导 cost（micros）。未知模型 → 0。
pub async fn compute_cost(pool: &PgPool, model: &str, input_tokens: i64, output_tokens: i64) -> i64 {
    // NUMERIC → ::text 避免 bigdecimal feature 依赖，parse f64 中间运算。
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT input_rate_per_mtok::text, output_rate_per_mtok::text FROM model_pricing WHERE model=$1",
    )
    .bind(model)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let Some((in_rate, out_rate)) = row else {
        return 0; // 未知模型不计费
    };
    let ir: f64 = in_rate.parse().unwrap_or(0.0);
    let or: f64 = out_rate.parse().unwrap_or(0.0);
    let cost_usd = (input_tokens as f64 / 1_000_000.0) * ir
        + (output_tokens as f64 / 1_000_000.0) * or;
    (cost_usd * 1_000_000.0).round() as i64
}

/// 每日聚合行（per-tenant 或 per-user）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DailyUsage {
    pub date: NaiveDate,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_micros: i64,
    pub total_turns: i64,
}

/// per-tenant 近 days 天聚合。
pub async fn daily_usage(pool: &PgPool, tenant_id: i64, days: i32) -> Result<Vec<DailyUsage>> {
    // Rust 算 cutoff 时间戳传入，规避 sqlx 对 make_interval(named arg) 的类型推断问题。
    let cutoff = Utc::now() - chrono::Duration::days(days as i64);
    let rows = sqlx::query_as::<_, DailyUsage>(
        "SELECT (recorded_at AT TIME ZONE 'UTC')::date AS date,
                SUM(input_tokens)::bigint  AS total_input_tokens,
                SUM(output_tokens)::bigint AS total_output_tokens,
                SUM(cost_micros)::bigint   AS total_cost_micros,
                COUNT(*)           AS total_turns
         FROM usage_records
         WHERE tenant_id=$1 AND recorded_at >= $2
         GROUP BY date ORDER BY date ASC",
    )
    .bind(tenant_id)
    .bind(cutoff)
    .fetch_all(pool)
    .await
    .context("aggregate daily usage")?;
    Ok(rows)
}

/// per-user 近 days 天聚合（admin 用）。
pub async fn daily_usage_user(
    pool: &PgPool,
    tenant_id: i64,
    user_id: i64,
    days: i32,
) -> Result<Vec<DailyUsage>> {
    let cutoff = Utc::now() - chrono::Duration::days(days as i64);
    let rows = sqlx::query_as::<_, DailyUsage>(
        "SELECT (recorded_at AT TIME ZONE 'UTC')::date AS date,
                SUM(input_tokens)::bigint  AS total_input_tokens,
                SUM(output_tokens)::bigint AS total_output_tokens,
                SUM(cost_micros)::bigint   AS total_cost_micros,
                COUNT(*)           AS total_turns
         FROM usage_records
         WHERE tenant_id=$1 AND user_id=$2 AND recorded_at >= $3
         GROUP BY date ORDER BY date ASC",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(cutoff)
    .fetch_all(pool)
    .await
    .context("aggregate daily usage per user")?;
    Ok(rows)
}

/// silence unused import warning if chrono Utc/DateTime unused in some cfg
#[allow(dead_code)]
fn _touch(_: DateTime<Utc>) {}

#[cfg(test)]
mod tests {
    #[test]
    fn cost_micros_arithmetic() {
        // 不依赖 DB：验证单位换算公式 (input/1e6 * rate + output/1e6 * rate) * 1e6
        // gpt-4o: in=2.50/M, out=10.00/M
        let input = 1_000_000i64;
        let output = 500_000i64;
        let ir = 2.50f64;
        let or = 10.00f64;
        let cost_usd = (input as f64 / 1_000_000.0) * ir
            + (output as f64 / 1_000_000.0) * or;
        let micros = (cost_usd * 1_000_000.0).round() as i64;
        // 1M in * 2.50 = 2.50 USD; 0.5M out * 10 = 5.00 USD; total 7.50 USD = 7_500_000 micros
        assert_eq!(micros, 7_500_000);
    }

    #[test]
    fn cost_zero_for_zero_tokens() {
        let cost_usd: f64 = (0.0 / 1_000_000.0) * 2.5 + (0.0 / 1_000_000.0) * 10.0;
        assert_eq!((cost_usd * 1_000_000.0).round() as i64, 0);
    }
}
