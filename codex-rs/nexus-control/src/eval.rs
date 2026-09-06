//! Evaluation center — eval cases + run results (M12, roadmap M8 T8-1).
//!
//! An eval case declares an `input` plus expectations (`expected_status`,
//! optional `expected_contains`). A run asserts a *completed* turn against a
//! case: it loads the turn's status + item contents, evaluates the predicates,
//! and records a pass/fail row. Turning a turn is intentionally out of scope
//! here — the caller starts a turn via `/v1/threads/{id}/turns` (M2) and then
//! submits the resulting `turn_id` for evaluation, keeping the turn lifecycle
//! and the eval lifecycle decoupled (Simplicity First).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(sqlx::FromRow, Serialize)]
pub struct EvalCase {
    pub id: i64,
    pub tenant_id: i64,
    pub name: String,
    pub category: Option<String>,
    pub input: String,
    pub expected_status: String,
    pub expected_contains: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct EvalRun {
    pub id: i64,
    pub tenant_id: i64,
    pub case_id: i64,
    pub turn_id: i64,
    pub passed: bool,
    pub detail: Value,
    pub created_at: DateTime<Utc>,
}

/// Create an eval case (admin). Returns the new case id.
pub async fn create_case(
    pool: &PgPool,
    tenant_id: i64,
    name: &str,
    category: Option<&str>,
    input: &str,
    expected_status: &str,
    expected_contains: Option<&str>,
) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO eval_cases (tenant_id, name, category, input, expected_status, expected_contains)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .bind(category)
    .bind(input)
    .bind(expected_status)
    .bind(expected_contains)
    .fetch_one(pool)
    .await
    .context("create eval_case")?;
    Ok(row.0)
}

pub async fn list_cases(pool: &PgPool, tenant_id: i64) -> Result<Vec<EvalCase>> {
    sqlx::query_as::<_, EvalCase>(
        "SELECT id, tenant_id, name, category, input, expected_status, expected_contains, created_at \
         FROM eval_cases WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT 200",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .context("list eval_cases")
}

/// Evaluate a completed turn against a case. Loads turn status + item
/// contents (scoped to the caller's tenant via thread ownership), runs the
/// predicates, and persists an `eval_runs` row. Returns the recorded run.
pub async fn run_eval(
    pool: &PgPool,
    tenant_id: i64,
    case_id: i64,
    turn_id: i64,
) -> Result<EvalRun> {
    // Load the case.
    let case: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT expected_status, expected_contains FROM eval_cases WHERE id=$1 AND tenant_id=$2",
    )
    .bind(case_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("eval: load case")?;
    let (expected_status, expected_contains) = case
        .ok_or_else(|| anyhow::anyhow!("eval case not found"))?;

    // Load the turn status (tenant-scoped via thread ownership).
    let turn: Option<(String,)> = sqlx::query_as(
        "SELECT t.status FROM turns t JOIN threads th ON th.id=t.thread_id \
         WHERE t.id=$1 AND th.tenant_id=$2",
    )
    .bind(turn_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("eval: load turn")?;
    let actual_status = turn
        .as_ref()
        .map(|(s,)| s.as_str())
        .unwrap_or("not_found");

    // Load item contents for the turn (used by expected_contains).
    let items: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT content_ref FROM items WHERE turn_id=$1",
    )
    .bind(turn_id)
    .fetch_all(pool)
    .await
    .context("eval: load items")?;
    let matched_contains = match &expected_contains {
        Some(needle) if !needle.is_empty() => items.iter().any(|(c,)| {
            c.as_deref().is_some_and(|s| s.contains(needle.as_str()))
        }),
        _ => true, // no contains expectation → pass this predicate
    };

    let status_ok = actual_status == expected_status;
    let passed = status_ok && matched_contains;
    let detail = json!({
        "expected_status": expected_status,
        "actual_status": actual_status,
        "matched_contains": matched_contains,
        "items_count": items.len(),
        "turn_found": turn.is_some(),
    });

    let row: (i64,) = sqlx::query_as(
        "INSERT INTO eval_runs (tenant_id, case_id, turn_id, passed, detail)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(tenant_id)
    .bind(case_id)
    .bind(turn_id)
    .bind(passed)
    .bind(&detail)
    .fetch_one(pool)
    .await
    .context("eval: insert run")?;

    // Re-read the full row.
    let run = sqlx::query_as::<_, EvalRun>(
        "SELECT id, tenant_id, case_id, turn_id, passed, detail, created_at \
         FROM eval_runs WHERE id=$1",
    )
    .bind(row.0)
    .fetch_one(pool)
    .await
    .context("eval: fetch run")?;
    Ok(run)
}

pub async fn list_runs(pool: &PgPool, tenant_id: i64, limit: i64) -> Result<Vec<EvalRun>> {
    sqlx::query_as::<_, EvalRun>(
        "SELECT id, tenant_id, case_id, turn_id, passed, detail, created_at \
         FROM eval_runs WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind(tenant_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("list eval_runs")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure predicate check: status matches AND (no needle OR needle in some item).
    #[test]
    fn eval_predicate_logic() {
        let cases = [
            ("completed", Some("hello"), "completed", vec!["hello world"], true),
            ("completed", Some("hello"), "completed", vec!["bye"], false), // contains miss
            ("completed", None, "failed", vec!["x"], false),              // status miss
            ("completed", None, "completed", vec![], true),              // no contains, status ok
        ];
        for (exp_status, exp_contains, actual, items, want) in cases {
            let status_ok = actual == exp_status;
            let matched = match exp_contains {
                Some(n) if !n.is_empty() => items.iter().any(|s| s.contains(n)),
                _ => true,
            };
            assert_eq!(status_ok && matched, want, "case {exp_status}/{exp_contains:?}/{actual}");
        }
    }
}
