//! Lineage — provenance graph for the eval center (M22, P3).
//!
//! Given an entity (typically an `eval_batch_run`), reconstruct the upstream
//! chain (which golden set + version + cases fed it) and the downstream /
//! lateral facts (case results, agent outputs, audit + esign trail). Returns a
//! `{nodes, edges}` graph the web console renders. Built from SQL JOINs over
//! existing M20/M21/M22 tables — no graph database, no materialized edges
//! table (MVP: the join set is small enough to assemble per-query).

use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Serialize)]
pub struct LineageGraph {
    pub root: String,
    pub nodes: Vec<LineageNode>,
    pub edges: Vec<LineageEdge>,
}

#[derive(Serialize)]
pub struct LineageNode {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub detail: Value,
}

#[derive(Serialize)]
pub struct LineageEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

/// Build the lineage graph for `eval_batch_run/{id}`. The run is the root;
/// upstream = golden_set → cases; lateral = case_results + agent outputs,
/// domain events and esign records on the run itself.
pub async fn lineage_for_run(
    pool: &PgPool,
    tenant_id: i64,
    run_id: i64,
) -> Result<LineageGraph> {
    let root = format!("eval_batch_run:{run_id}");
    let mut nodes = vec![LineageNode {
        id: root.clone(),
        kind: "eval_batch_run".into(),
        label: format!("Run #{run_id}"),
        detail: json!({}),
    }];
    let mut edges: Vec<LineageEdge> = Vec::new();

    // 1. run row → golden_set + version
    let run: Option<(i64, i64, Option<i64>, String, Option<Value>, Option<String>)> =
        sqlx::query_as(
            "SELECT golden_set_id, golden_set_version_id, rubric_version_id, \
                    snapshot_hash, aggregate, status \
             FROM eval_batch_runs WHERE id = $1 AND tenant_id = $2",
        )
        .bind(run_id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| anyhow!("lineage: load run: {e:?}"))?;
    let (gs_id, gs_vid, rb_vid, snap_hash, aggregate, status) = match run {
        Some(r) => r,
        None => {
            return Ok(LineageGraph { root, nodes, edges });
        }
    };
    nodes[0].detail = json!({
        "golden_set_id": gs_id,
        "golden_set_version_id": gs_vid,
        "rubric_version_id": rb_vid,
        "snapshot_hash": snap_hash,
        "aggregate": aggregate,
        "status": status,
    });

    // 2. golden_set node + version node
    let gs: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM golden_sets WHERE id = $1 AND tenant_id = $2",
    )
    .bind(gs_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| anyhow!("lineage: load golden_set: {e:?}"))?;
    let gs_name = gs.map(|(n,)| n).unwrap_or_default();
    let gs_node = format!("golden_set:{gs_id}");
    nodes.push(LineageNode {
        id: gs_node.clone(),
        kind: "golden_set".into(),
        label: gs_name.clone(),
        detail: json!({"id": gs_id}),
    });
    edges.push(LineageEdge {
        from: gs_node.clone(),
        to: root.clone(),
        relation: "feeds".into(),
    });
    let gv_node = format!("golden_set_version:{gs_vid}");
    nodes.push(LineageNode {
        id: gv_node.clone(),
        kind: "golden_set_version".into(),
        label: format!("v{gs_vid}"),
        detail: json!({"version_id": gs_vid}),
    });
    edges.push(LineageEdge {
        from: gv_node.clone(),
        to: gs_node.clone(),
        relation: "version_of".into(),
    });

    // 3. case_results → agent_output
    let results: Vec<(i64, String, Option<i64>, Option<String>, Value)> = sqlx::query_as(
        "SELECT id, case_key, turn_id, agent_output, scores \
         FROM eval_case_results WHERE run_id = $1 ORDER BY id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("lineage: load case_results: {e:?}"))?;
    for (cr_id, case_key, turn_id, agent_output, scores) in &results {
        let cr_node = format!("case_result:{cr_id}");
        nodes.push(LineageNode {
            id: cr_node.clone(),
            kind: "case_result".into(),
            label: case_key.clone(),
            detail: json!({"case_key": case_key, "turn_id": turn_id, "passed": scores.get("passed")}),
        });
        edges.push(LineageEdge {
            from: root.clone(),
            to: cr_node.clone(),
            relation: "produced".into(),
        });
        if let Some(out) = agent_output {
            let out_node = format!("agent_output:{cr_id}");
            // truncate long output for the label, keep full in detail
            let lbl: String = out.chars().take(40).collect();
            nodes.push(LineageNode {
                id: out_node.clone(),
                kind: "agent_output".into(),
                label: lbl,
                detail: json!({"content": out}),
            });
            edges.push(LineageEdge {
                from: cr_node.clone(),
                to: out_node,
                relation: "output".into(),
            });
        }
    }

    // 4. domain_events on the run
    let events: Vec<(i64, String, Value, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT event_seq, event_type, payload, occurred_at \
         FROM domain_events \
         WHERE tenant_id = $1 AND entity_type = 'eval_batch_run' AND entity_id = $2 \
         ORDER BY event_seq ASC",
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("lineage: load events: {e:?}"))?;
    for (seq, etype, payload, ts) in &events {
        let ev_node = format!("event:{run_id}:{seq}");
        nodes.push(LineageNode {
            id: ev_node.clone(),
            kind: "domain_event".into(),
            label: etype.clone(),
            detail: json!({"seq": seq, "event_type": etype, "payload": payload, "occurred_at": ts.to_rfc3339()}),
        });
        edges.push(LineageEdge {
            from: root.clone(),
            to: ev_node,
            relation: "lifecycle".into(),
        });
    }

    // 5. esign records on the run
    let sigs: Vec<(i64, i64, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, signer_user_id, signature_purpose, signed_at \
         FROM esign_records \
         WHERE tenant_id = $1 AND entity_type = 'eval_batch_run' AND entity_id = $2 \
         ORDER BY signed_at ASC",
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("lineage: load esigns: {e:?}"))?;
    for (sid, signer, purpose, ts) in &sigs {
        let sig_node = format!("esign:{sid}");
        nodes.push(LineageNode {
            id: sig_node.clone(),
            kind: "esign".into(),
            label: purpose.clone(),
            detail: json!({"id": sid, "signer_user_id": signer, "signed_at": ts.to_rfc3339()}),
        });
        edges.push(LineageEdge {
            from: root.clone(),
            to: sig_node,
            relation: "signed_by".into(),
        });
    }

    Ok(LineageGraph { root, nodes, edges })
}

/// Dispatch lineage by entity type. Currently only `eval_batch_run` is
/// supported; other types return an empty graph (the web console shows "no
/// lineage available").
pub async fn lineage_for(
    pool: &PgPool,
    tenant_id: i64,
    entity_type: &str,
    entity_id: i64,
) -> Result<LineageGraph> {
    match entity_type {
        "eval_batch_run" => lineage_for_run(pool, tenant_id, entity_id).await,
        _ => Ok(LineageGraph {
            root: format!("{entity_type}:{entity_id}"),
            nodes: vec![],
            edges: vec![],
        }),
    }
}
