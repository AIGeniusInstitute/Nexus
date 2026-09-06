//! Per-thread unified timeline + trace lookup (M11).
//!
//! `thread_timeline` merges turns / items / approval_tickets for one thread
//! into a single time-ordered stream (the "what happened" replay view).
//! `trace_lookup` correlates a turn's `trace_id` across `audit_logs` (the
//! "why / who decided" view). The two are deliberately separate: timeline is
//! structural replay, trace is the audit correlate — joined via trace_id but
//! not squashed into one table (Surgical: M3/M10 tables untouched).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::audit;

/// One row in a unified timeline.
#[derive(Serialize)]
pub struct TimelineEntry {
    pub ts: DateTime<Utc>,
    /// "turn" | "item" | "approval"
    pub kind: String,
    pub turn_id: Option<i64>,
    pub payload: Value,
}

/// Build the merged timeline for a thread. Caller must have already verified
/// the thread belongs to the requesting tenant. Three independent queries +
/// a Rust-side merge/sort — avoids brittle SQL UNION casts across the three
/// heterogeneous row shapes.
pub async fn thread_timeline(pool: &PgPool, thread_id: Uuid) -> Result<Vec<TimelineEntry>> {
    let mut out: Vec<TimelineEntry> = Vec::new();

    // Turns.
    let turns: Vec<(DateTime<Utc>, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT started_at, id, status, model FROM turns WHERE thread_id=$1 \
         ORDER BY started_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .context("timeline: turns")?;
    for (ts, id, status, model) in turns {
        out.push(TimelineEntry {
            ts,
            kind: "turn".into(),
            turn_id: Some(id),
            payload: json!({ "status": status, "model": model }),
        });
    }

    // Items.
    let items: Vec<(DateTime<Utc>, i64, i64, String, Option<String>)> = sqlx::query_as(
        "SELECT created_at, turn_id, seq, item_type, content_ref \
         FROM items WHERE thread_id=$1 ORDER BY created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await
    .context("timeline: items")?;
    for (ts, turn_id, seq, item_type, content_ref) in items {
        out.push(TimelineEntry {
            ts,
            kind: "item".into(),
            turn_id: Some(turn_id),
            payload: json!({ "seq": seq, "item_type": item_type, "content_ref": content_ref }),
        });
    }

    // Approvals.
    let aps: Vec<(DateTime<Utc>, i64, Option<String>, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT created_at, turn_id, command, status, policy_decision, risk_level, kind \
             FROM approval_tickets WHERE thread_id=$1 ORDER BY created_at ASC",
        )
        .bind(thread_id)
        .fetch_all(pool)
        .await
        .context("timeline: approvals")?;
    for (ts, turn_id, command, status, policy_decision, risk_level, kind) in aps {
        out.push(TimelineEntry {
            ts,
            kind: "approval".into(),
            turn_id: Some(turn_id),
            payload: json!({
                "command": command,
                "status": status,
                "policy_decision": policy_decision,
                "risk_level": risk_level,
                "kind": kind,
            }),
        });
    }

    // Stable sort by timestamp ascending.
    out.sort_by(|a, b| a.ts.cmp(&b.ts));
    Ok(out)
}

/// Correlate a trace_id across turns + audit_logs (admin sees all tenants,
/// non-admin scoped via `tenant_filter`).
pub async fn trace_lookup(
    pool: &PgPool,
    trace_id: Uuid,
    tenant_filter: Option<i64>,
) -> Result<Value> {
    // The turn (if any) carrying this trace_id. input_tokens/output_tokens
    // are integer (INT4, NOT NULL, default 0) so decode as i32, not i64.
    let turn: Option<(i64, Uuid, String, Option<String>, i32, i32)> =
        sqlx::query_as(
            "SELECT t.id, t.thread_id, t.status, t.model, t.input_tokens, t.output_tokens \
             FROM turns t WHERE t.trace_id=$1 \
               AND ($2::bigint IS NULL OR EXISTS \
                    (SELECT 1 FROM threads h WHERE h.id=t.thread_id AND h.tenant_id=$2))",
        )
        .bind(trace_id)
        .bind(tenant_filter)
        .fetch_optional(pool)
        .await
        .context("trace: turn")?;
    let turn_json = turn.map(|(id, thread_id, status, model, input_tokens, output_tokens)| json!({
        "id": id, "thread_id": thread_id, "status": status, "model": model,
        "input_tokens": input_tokens, "output_tokens": output_tokens,
    }));

    // Audit rows for this trace_id. audit_logs.trace_id is TEXT (M10) holding
    // the UUID string, so bind the string form rather than the Uuid.
    let trace_str = trace_id.to_string();
    let audit: Vec<audit::AuditLogRow> = sqlx::query_as::<_, audit::AuditLogRow>(
        "SELECT id, tenant_id, actor_user_id, action, target_type, target_id, \
                detail_json, trace_id, created_at \
         FROM audit_logs WHERE trace_id=$1 \
           AND ($2::bigint IS NULL OR tenant_id=$2) \
         ORDER BY created_at ASC",
    )
    .bind(&trace_str)
    .bind(tenant_filter)
    .fetch_all(pool)
    .await
    .context("trace: audit")?;

    Ok(json!({ "trace_id": trace_id, "turn": turn_json, "audit": audit }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_sort_is_stable_ascending() {
        let t0 = chrono::DateTime::parse_from_rfc3339("2026-09-06T01:00:00Z").unwrap().with_timezone(&Utc);
        let t1 = chrono::DateTime::parse_from_rfc3339("2026-09-06T02:00:00Z").unwrap().with_timezone(&Utc);
        let mut v = vec![
            TimelineEntry { ts: t1, kind: "item".into(), turn_id: Some(1), payload: json!({}) },
            TimelineEntry { ts: t0, kind: "turn".into(), turn_id: Some(1), payload: json!({}) },
        ];
        v.sort_by(|a, b| a.ts.cmp(&b.ts));
        assert!(v[0].ts < v[1].ts);
        assert_eq!(v[0].kind, "turn");
    }
}
