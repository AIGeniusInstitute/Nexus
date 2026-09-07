//! Domain event sourcing — append-only event stream + as-of replay (M22, P3).
//!
//! Core lifecycle actions on versioned entities (golden_set / rubric /
//! judge_config / eval_batch_run / approval / esign) are recorded here as an
//! immutable event log. The table is enforced append-only at the Postgres
//! layer by `prevent_event_modification` (BEFORE UPDATE OR DELETE → RAISE).
//!
//! `event_seq` is per-entity monotonically increasing (computed as
//! `MAX(event_seq)+1` in a single atomic INSERT ... SELECT), so the event
//! history of any one entity is ordered and can be replayed to reconstruct
//! its state as of any point in time (as-of / time-travel query).
//!
//! Recording is best-effort: a failure is logged but does not fail the
//! caller's operation (the audit trail must never block the business path),
//! mirroring the M10 `audit_log` pattern.

use anyhow::anyhow;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(sqlx::FromRow, Serialize)]
pub struct DomainEventRow {
    pub id: i64,
    pub tenant_id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub event_type: String,
    pub event_seq: i64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

/// Append one domain event. `event_seq` is assigned atomically as
/// `MAX(event_seq)+1` within the same INSERT (per-entity monotonic). Returns
/// the new row id on success. Best-effort: failures are logged, not
/// propagated. Concurrent inserts on the same entity may collide on the
/// UNIQUE(entity_type, entity_id, event_seq) constraint — acceptable for the
/// audit/log use case (low concurrency); the duplicate is dropped with a
/// warning. Keep `payload` small (facts only); large content belongs in
/// `content_store` (CAS) referenced by hash.
pub async fn record_event(
    pool: &PgPool,
    tenant_id: i64,
    entity_type: &str,
    entity_id: i64,
    event_type: &str,
    payload: &Value,
) {
    let res = sqlx::query(
        "INSERT INTO domain_events \
         (tenant_id, entity_type, entity_id, event_type, event_seq, payload) \
         VALUES ($1, $2, $3, $4, \
                 (SELECT COALESCE(MAX(event_seq), 0) + 1 \
                  FROM domain_events WHERE entity_type = $2 AND entity_id = $3), \
                 $5)",
    )
    .bind(tenant_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await;
    if let Err(e) = res {
        tracing::warn!(
            "record_event failed ({entity_type}/{entity_id} {event_type}): {e:?}"
        );
    }
}

/// List events for an entity, optionally truncated to `as_of` (inclusive) for
/// time-travel replay. Ordered by `event_seq` ascending so the caller can
/// fold them in order to reconstruct state.
pub async fn list_events(
    pool: &PgPool,
    tenant_id: i64,
    entity_type: &str,
    entity_id: i64,
    as_of: Option<DateTime<Utc>>,
) -> Result<Vec<DomainEventRow>> {
    sqlx::query_as::<_, DomainEventRow>(
        "SELECT id, tenant_id, entity_type, entity_id, event_type, event_seq, \
                payload, occurred_at \
         FROM domain_events \
         WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3 \
           AND ($4::timestamptz IS NULL OR occurred_at <= $4) \
         ORDER BY event_seq ASC",
    )
    .bind(tenant_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(as_of)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_events: {e:?}"))
}

/// Replay an event stream into a reconstructed state snapshot. Later events
/// override earlier ones by merging `payload` top-level keys (last-write-wins
/// per field). Returns `null` if the stream is empty. MVP: shallow merge;
/// nested objects are replaced wholesale, not deep-merged.
pub fn replay_state(events: &[DomainEventRow]) -> Value {
    if events.is_empty() {
        return Value::Null;
    }
    let mut state = json!({});
    for ev in events {
        if let Some(obj) = ev.payload.as_object() {
            for (k, v) in obj {
                state[k.clone()] = v.clone();
            }
        }
        // stamp metadata so the caller can tell which event was last applied
        state["_last_event_type"] = json!(ev.event_type);
        state["_last_event_seq"] = json!(ev.event_seq);
        state["_as_of"] = json!(ev.occurred_at.to_rfc3339());
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_state_merges_in_order() {
        let mk = |et: &str, p: Value| DomainEventRow {
            id: 0,
            tenant_id: 1,
            entity_type: "eval_batch_run".into(),
            entity_id: 1,
            event_type: et.into(),
            event_seq: 0,
            payload: p,
            occurred_at: Utc::now(),
        };
        let events = vec![
            mk("created", json!({"status": "running", "v": 1})),
            mk("run_completed", json!({"status": "completed", "aggregate": {"accuracy": 0.5}})),
        ];
        let s = replay_state(&events).as_object().unwrap().clone();
        assert_eq!(s["status"], json!("completed")); // last wins
        assert_eq!(s["v"], json!(1)); // untouched by 2nd
        assert_eq!(s["aggregate"]["accuracy"], json!(0.5));
        assert_eq!(s["_last_event_type"], json!("run_completed"));
    }

    #[test]
    fn replay_state_empty_is_null() {
        assert!(replay_state(&[]).is_null());
    }
}
