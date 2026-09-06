//! HTTP API gateway: axum router + handlers + idempotency/rate-limit middleware (T1-2/T1-3).
//! M2: turn_start drives the real app-server runtime; interrupt endpoint added.
//! M3: HITL approval — turn_start persists approval tickets and broadcasts
//! `approval/requested`; new `/v1/approvals` resolve/list endpoints.
//! M5: the single global-mutex event receiver is replaced by a `DriverPool`
//! — `turn_start` `acquire()`s a free slot, drains its `event_rx`
//! exclusively (no shared mutex → N-way concurrent turns), and `release()`s
//! on turn end. resolve/interrupt route to the slot holding the in-flight
//! turn via the `turn_slots` map (turn_db_id → slot idx).

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::auth::{AuthProvider, AuthUser, JwtIssuer};
use crate::audit;
use crate::eval;
use crate::kb;
use crate::metering;
use crate::policy;
use crate::runtime::{self, DriverCommand};
use crate::timeline;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt: Arc<JwtIssuer>,
    pub auth: Arc<dyn AuthProvider>,
    /// M5: driver pool — N independent app-server driver threads. turn_start
    /// `acquire()`s a free slot and drains its event_rx exclusively.
    pub driver_pool: Arc<runtime::DriverPool>,
    /// M5: routing map turn_db_id → slot idx, so approval resolve / interrupt
    /// handlers can dispatch to the driver actually running that turn.
    pub turn_slots: Arc<Mutex<HashMap<i64, usize>>>,
    /// M6: CODEX_HOME path — used to hot-rewrite tenant .rules files after a
    /// policy is auto-learned (app-server reloads rules per-turn).
    pub codex_home: std::path::PathBuf,
    /// Per-thread broadcast channels for live WS push.
    pub broadcast: Arc<Mutex<HashMap<Uuid, broadcast::Sender<Value>>>>,
}

/// Lazily create (or reuse) the broadcast channel for a thread.
pub async fn thread_broadcast(
    st: &AppState,
    thread_id: Uuid,
) -> broadcast::Sender<Value> {
    let mut map = st.broadcast.lock().await;
    if let Some(tx) = map.get(&thread_id) {
        return tx.clone();
    }
    let (tx, _rx) = broadcast::channel::<Value>(256);
    map.insert(thread_id, tx.clone());
    tx
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/me", get(me))
        .route("/v1/threads", get(threads_list).post(thread_create))
        .route("/v1/threads/{id}/turns", post(turn_start))
        .route("/v1/threads/{id}/turns/{turn_id}/interrupt", post(turn_interrupt))
        .route("/v1/threads/{id}/items", get(items_list))
        .route("/v1/threads/{id}/approvals", get(thread_approvals))
        .route("/v1/approvals", get(approvals_list))
        .route("/v1/approvals/{aid}/resolve", post(approval_resolve))
        .route("/v1/usage", get(usage_summary))
        .route("/v1/usage/users/{uid}", get(usage_user))
        .route("/v1/policy/feedback", get(policy_feedback))
        .route("/v1/policy/rules", get(policy_rules))
        .route("/v1/audit/logs", get(audit_logs))
        .route("/v1/audit/logs/{id}", get(audit_log_get))
        .route("/v1/threads/{id}/timeline", get(thread_timeline_handler))
        .route("/v1/traces/{trace_id}", get(trace_lookup_handler))
        .route("/v1/evals/cases", post(eval_case_create).get(eval_cases_list))
        .route("/v1/evals/runs/{case_id}", post(eval_run))
        .route("/v1/evals/runs", get(eval_runs_list))
        .route("/v1/kbs", post(kb_create).get(kb_list))
        .route("/v1/kbs/{id}/documents", post(kb_doc_ingest).get(kb_doc_list))
        .route("/v1/kbs/{id}/documents/{did}", axum::routing::delete(kb_doc_delete))
        .route("/v1/kbs/{id}/search", post(kb_search))
        .route("/v1/ws/threads/{id}/events", get(crate::ws::ws_handler))
        .layer(middleware::from_fn_with_state(state.clone(), idempotency_layer))
        .layer(middleware::from_fn_with_state(state.clone(), user_rate_limit))
        .layer(middleware::from_fn_with_state(state.clone(), ip_rate_limit))
        .layer(axum::Extension(state.jwt.clone()))
        .route_layer(axum::middleware::from_fn(require_auth_stateless))
        .with_state(state)
}

async fn ip_rate_limit(State(st): State<AppState>, req: axum::extract::Request, next: Next) -> Response {
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    let key = format!("ip:{ip}");
    let allowed: bool = sqlx::query_scalar(
        "INSERT INTO rate_limit_buckets (bucket_key, tokens, last_refill_at) VALUES ($1, 1, NOW())
         ON CONFLICT (bucket_key) DO UPDATE
           SET tokens = CASE WHEN rate_limit_buckets.last_refill_at < NOW() - interval '1 minute'
                             THEN 1 ELSE rate_limit_buckets.tokens + 1 END,
               last_refill_at = CASE WHEN rate_limit_buckets.last_refill_at < NOW() - interval '1 minute'
                             THEN NOW() ELSE rate_limit_buckets.last_refill_at END
         RETURNING (tokens <= 200)",
    )
    .bind(&key)
    .fetch_one(&st.pool)
    .await
    .unwrap_or(false);
    if !allowed {
        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("retry-after", "60")
            .body(axum::body::Body::empty())
            .unwrap();
    }
    next.run(req).await
}

async fn health() -> &'static str { "ok\n" }

// ---------- Auth ----------
#[derive(Deserialize)]
struct LoginReq { email: String, password: String }

#[derive(Serialize)]
struct LoginResp { token: String, user_id: i64, perms: Vec<String> }

async fn login(State(st): State<AppState>, Json(req): Json<LoginReq>) -> Result<Json<LoginResp>, (StatusCode, String)> {
    let (claims, token) = st
        .auth
        .login(&req.email, &req.password)
        .await
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid credentials".into()))?;
    // M10: audit auth.login (best-effort, never blocks).
    audit::audit_log(
        &st.pool, claims.tid, Some(claims.uid), "auth.login",
        Some("user"), Some(&claims.uid.to_string()), None, None,
    ).await;
    Ok(Json(LoginResp { token, user_id: claims.uid, perms: claims.perms }))
}

async fn me(AuthUser(c): AuthUser) -> Json<Value> {
    Json(serde_json::json!({
        "user_id": c.uid, "tenant_id": c.tid, "email": c.sub, "perms": c.perms,
    }))
}

// ---------- Threads / Turns / Items ----------
#[derive(Serialize, sqlx::FromRow)]
struct ThreadRow {
    id: Uuid,
    title: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
}

async fn threads_list(AuthUser(c): AuthUser, State(st): State<AppState>) -> Result<Json<Vec<ThreadRow>>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, ThreadRow>(
        "SELECT id, title, status, created_at FROM threads WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(c.tid)
    .fetch_all(&st.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateThreadReq { title: Option<String> }

async fn thread_create(AuthUser(c): AuthUser, State(st): State<AppState>, Json(req): Json<CreateThreadReq>) -> Result<Json<Value>, (StatusCode, String)> {
    let row: (Uuid,) = sqlx::query_as("INSERT INTO threads (tenant_id, owner_user_id, title) VALUES ($1, $2, $3) RETURNING id")
        .bind(c.tid).bind(c.uid).bind(req.title)
        .fetch_one(&st.pool).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "id": row.0 })))
}

#[derive(Deserialize)]
struct TurnReq { input: Option<String> }

async fn turn_start(AuthUser(c): AuthUser, State(st): State<AppState>, Path(id): Path<Uuid>, Json(req): Json<TurnReq>) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify thread belongs to user's tenant + read codex_thread_id (NULL=new).
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT codex_thread_id FROM threads WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id).bind(c.tid).fetch_optional(&st.pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let codex_thread_id = match row {
        Some((existing,)) => existing,
        None => return Err((StatusCode::NOT_FOUND, "thread not found".into())),
    };

    // M4: 多租户并发上限（锁 mutex 前置门控，防同租户请求积压）。
    let running: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turns t JOIN threads th ON th.id=t.thread_id
         WHERE th.tenant_id=$1 AND t.status='running'",
    )
    .bind(c.tid).fetch_one(&st.pool).await
    .unwrap_or(0);
    let limit: i32 = sqlx::query_scalar("SELECT max_concurrent_turns FROM tenants WHERE id=$1")
        .bind(c.tid).fetch_one(&st.pool).await
        .unwrap_or(1);
    if running >= limit as i64 {
        return Err((StatusCode::TOO_MANY_REQUESTS,
            format!("too_many_concurrent_turns: limit={limit}")));
    }

    // Create the turn row (status running). M11: RETURNING trace_id.
    let trow: (i64, Uuid) = sqlx::query_as(
        "INSERT INTO turns (thread_id, status, started_at) VALUES ($1, 'running', NOW()) RETURNING id, trace_id",
    )
    .bind(id).fetch_one(&st.pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let turn_db_id = trow.0;
    let trace_id = trow.1;

    // Max seq persisted for this thread (continue monotonic thread-level seq).
    let max_seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM app_server_events WHERE thread_id=$1",
    )
    .bind(id).fetch_one(&st.pool).await
    .unwrap_or(0);

    let input = req.input.unwrap_or_else(|| "(empty turn)".into());

    let bcast = thread_broadcast(&st, id).await;

    // M5: acquire a free driver slot from the pool. Each slot drains its OWN
    // event_rx exclusively — no global mutex, so N turns can run concurrently.
    let mut guard = match st.driver_pool.acquire().await {
        Some(g) => g,
        None => {
            return Err((StatusCode::SERVICE_UNAVAILABLE, "driver pool exhausted".into()));
        }
    };
    // Record turn→slot routing so resolve/interrupt can reach this driver.
    st.turn_slots.lock().await.insert(turn_db_id, guard.idx);

    // Dispatch RunTurn to this slot's driver thread.
    if guard
        .cmd_tx()
        .send(DriverCommand::RunTurn {
            thread_id: id,
            codex_thread_id: codex_thread_id.clone(),
            turn_db_id,
            input: input.clone(),
            start_seq: max_seq,
        })
        .is_err()
    {
        st.turn_slots.lock().await.remove(&turn_db_id);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "runtime driver gone".into()));
    }

    let mut last_usage: Option<runtime::Usage> = None;
    let mut resolved_codex_thread_id: Option<String> = codex_thread_id.clone();
    let mut final_status = "completed".to_string();
    // Drain this slot's event stream exclusively (scoped so the borrow ends
    // before the guard is dropped).
    {
        let rx = guard.event_rx_mut();
        while let Some(ev) = rx.recv().await {
            // Persist codex_thread_id on first resolve.
            if let Some(cid) = &ev.codex_thread_id {
                resolved_codex_thread_id = Some(cid.clone());
                let _ = sqlx::query("UPDATE threads SET codex_thread_id=$1 WHERE id=$2")
                    .bind(cid).bind(id).execute(&st.pool).await;
            }

            // M3: approval/requested — persist a pending ticket, audit, broadcast;
            // then keep draining (driver is parked, no more events until resolve).
            if ev.item_type == "approval/requested" {
                if let Some(ap) = &ev.approval {
                    let kind_str = match ap.kind {
                        runtime::ApprovalKind::CommandExecution => "command_execution",
                        runtime::ApprovalKind::FileChange => "file_change",
                    };
                    // M4: 策略推荐（evaluate）+ 风险标注（risk_of）。人仍最终决策。
                    let cmd = ap.command.as_deref().unwrap_or("");
                    let pol = policy::evaluate(&st.pool, c.tid, "admin", "command_execution", cmd)
                        .await.unwrap_or(policy::PolicyDecision::Prompt);
                    let pol_str = match pol { policy::PolicyDecision::Allow => "allow",
                        policy::PolicyDecision::Deny => "deny", policy::PolicyDecision::Prompt => "prompt" };
                    let risk = policy::risk_of(cmd);
                    // 先落库再回写（R3：Pod 崩溃后 pending ticket 不丢）。
                    let _ = sqlx::query(
                        "INSERT INTO approval_tickets
                           (id, thread_id, turn_id, tenant_id, kind, status, item_id,
                            jsonrpc_id, command, cwd, reason, raw_params,
                            policy_decision, risk_level, created_at)
                         VALUES ($1,$2,$3,$4,$5,'pending',$6,$7,$8,$9,$10,$11,$12,$13,NOW())
                         ON CONFLICT (id) DO NOTHING",
                    )
                    .bind(ap.approval_id).bind(id).bind(turn_db_id).bind(c.tid)
                    .bind(kind_str).bind(ev.codex_item_id.as_deref())
                    .bind(ev.raw_json.clone())  // jsonrpc_id stored as raw event (sim) — see note
                    .bind(ap.command.as_deref()).bind(ap.cwd.as_deref())
                    .bind(ap.reason.as_deref()).bind(&ap.raw_params)
                    .bind(pol_str).bind(risk)
                    .execute(&st.pool).await;
                    let _ = sqlx::query(
                        "INSERT INTO approval_audit (approval_id, actor_user_id, action, params_digest)
                         VALUES ($1,$2,'created',$3)",
                    )
                    .bind(ap.approval_id).bind(c.uid).bind(ap.command.as_deref())
                    .execute(&st.pool).await;
                }
                let frame = serde_json::json!({
                    "thread_id": id, "seq": ev.seq, "type": ev.item_type,
                    "content": ev.content_ref,
                    "approval_id": ev.approval.as_ref().map(|a| a.approval_id),
                    "command": ev.approval.as_ref().and_then(|a| a.command.clone()),
                });
                let _ = bcast.send(frame);
                continue;
            }

            if ev.item_type == "approval/interrupted" {
                final_status = "interrupted".to_string();
                // Mark any pending tickets for this thread as interrupted.
                let _ = sqlx::query(
                    "UPDATE approval_tickets SET status='interrupted'
                     WHERE thread_id=$1 AND status='pending'",
                )
                .bind(id).execute(&st.pool).await;
                let _ = sqlx::query(
                    "INSERT INTO approval_audit (approval_id, actor_user_id, action, decision)
                     SELECT id, $2, 'interrupted', 'cancelled' FROM approval_tickets
                     WHERE thread_id=$1 AND status='interrupted'
                     ON CONFLICT DO NOTHING",
                )
                .bind(id).bind(c.uid).execute(&st.pool).await;
                let _ = bcast.send(serde_json::json!({
                    "thread_id": id, "seq": ev.seq, "type": ev.item_type,
                }));
                if ev.is_turn_completed { break; }
                continue;
            }

            // M7: execpolicy amendment (app-server-proposed, human-accepted) —
            // merge into policies + refresh tenant .rules.
            if ev.item_type == "execpolicy/amendment" {
                if let Some(cmd) = &ev.amendment {
                    let merged = policy::merge_amendment(&st.pool, c.tid, cmd)
                        .await.unwrap_or(None);
                    if merged.is_some() {
                        if let Ok(content) = policy::generate_rules(&st.pool, c.tid).await {
                            if !content.is_empty() {
                                let _ = policy::write_tenant_rules(c.tid, &st.codex_home, &content);
                            }
                        }
                        tracing::info!(?cmd, "execpolicy amendment merged + rules refreshed");
                    }
                }
                let _ = bcast.send(serde_json::json!({
                    "thread_id": id, "seq": ev.seq, "type": ev.item_type,
                }));
                continue;
            }

            // app_server_events: raw event log (idempotent on thread+seq).
            let _ = sqlx::query(
                "INSERT INTO app_server_events (thread_id, turn_id, seq, event_json)
                 VALUES ($1, $2, $3, $4) ON CONFLICT (thread_id, seq) DO NOTHING",
            )
            .bind(id).bind(turn_db_id).bind(ev.seq).bind(&ev.raw_json)
            .execute(&st.pool).await;

            // items: only item/* notifications carry a codex_item_id.
            if let Some(cid) = &ev.codex_item_id {
                let _ = sqlx::query(
                    "INSERT INTO items (thread_id, turn_id, seq, item_type, content_ref, codex_item_id)
                     VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (codex_item_id) WHERE codex_item_id IS NOT NULL DO UPDATE
                       SET content_ref = EXCLUDED.content_ref, item_type = EXCLUDED.item_type",
                )
                .bind(id).bind(turn_db_id).bind(ev.seq).bind(&ev.item_type)
                .bind(ev.content_ref.as_deref()).bind(cid)
                .execute(&st.pool).await;
            }

            // Accumulate usage.
            if let Some(u) = &ev.usage {
                last_usage = Some(u.clone());
            }

            // Broadcast to WS subscribers.
            let frame = serde_json::json!({
                "thread_id": id, "seq": ev.seq, "type": ev.item_type,
                "content": ev.content_ref, "item_id": ev.codex_item_id,
            });
            let _ = bcast.send(frame);

            if ev.is_turn_completed {
                break;
            }
        }
    }
    // Release the slot back to the pool + clear turn→slot routing.
    st.turn_slots.lock().await.remove(&turn_db_id);
    drop(guard);

    // Finalize the turn.
    let status = if final_status == "interrupted" {
        let _ = sqlx::query(
            "UPDATE turns SET status='interrupted', completed_at=NOW() WHERE id=$1",
        )
        .bind(turn_db_id).execute(&st.pool).await;
        "interrupted"
    } else if let Some(u) = &last_usage {
        // M4: 推导 cost + 写 usage_records（per-tenant 计量）+ 写 turns.model。
        // M8: 真实模式下 `Usage.model` 为 None（codex 的
        // ThreadTokenUsageUpdated 不带 model），fallback 到 NEXUS_MODEL env
        // (deepseek-v4-pro) 而非泛化的 "nexus-gateway"，使真实 model 名落库。
        let model = u.model.clone().unwrap_or_else(|| {
            std::env::var("NEXUS_MODEL").unwrap_or_else(|_| "nexus-gateway".into())
        });
        let cost = metering::record_usage(
            &st.pool, c.tid, c.uid, id, turn_db_id,
            &model, u.input_tokens, u.output_tokens,
        ).await.unwrap_or(0);
        let _ = sqlx::query(
            "UPDATE turns SET status='completed', completed_at=NOW(),
                 input_tokens=$1, output_tokens=$2, cost_micros=$3, model=$4
             WHERE id=$5",
        )
        .bind(u.input_tokens).bind(u.output_tokens).bind(cost).bind(&model)
        .bind(turn_db_id).execute(&st.pool).await;
        "completed"
    } else {
        let _ = sqlx::query(
            "UPDATE turns SET status='completed', completed_at=NOW() WHERE id=$1",
        )
        .bind(turn_db_id).execute(&st.pool).await;
        "completed"
    };

    // M10: audit turn completion (best-effort). M11: carry trace_id.
    let detail = serde_json::json!({
        "thread_id": id, "status": status,
        "codex_thread_id": resolved_codex_thread_id.clone(),
    });
    audit::audit_log(
        &st.pool, c.tid, Some(c.uid), "turn.complete",
        Some("turn"), Some(&turn_db_id.to_string()),
        Some(&detail), Some(&trace_id.to_string()),
    ).await;

    Ok(Json(serde_json::json!({
        "turn_id": turn_db_id, "status": status,
        "codex_thread_id": resolved_codex_thread_id,
    })))
}

async fn turn_interrupt(
    AuthUser(c): AuthUser,
    State(st): State<AppState>,
    Path((_id, turn_id)): Path<(Uuid, i64)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify ownership.
    let owned: Option<(i64,)> = sqlx::query_as(
        "SELECT t.id FROM turns t JOIN threads th ON th.id=t.thread_id
         WHERE t.id=$1 AND th.tenant_id=$2",
    )
    .bind(turn_id).bind(c.tid).fetch_optional(&st.pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if owned.is_none() {
        return Err((StatusCode::NOT_FOUND, "turn not found".into()));
    }
    // M5: route the interrupt to the slot actually running this turn (if it
    // is still in-flight). If the turn already completed, there is no driver
    // to interrupt — the DB update below still records the final state.
    let slot_idx = st.turn_slots.lock().await.get(&turn_id).copied();
    if let Some(idx) = slot_idx {
        if let Some(tx) = st.driver_pool.cmd_tx(idx) {
            let _ = tx.send(DriverCommand::Interrupt);
        }
    }
    let _ = sqlx::query("UPDATE turns SET status='interrupted', completed_at=NOW() WHERE id=$1")
        .bind(turn_id).execute(&st.pool).await;
    // M11: look up trace_id to correlate the audit record.
    let trace_id: Option<String> = sqlx::query_scalar(
        "SELECT trace_id::text FROM turns WHERE id=$1")
        .bind(turn_id).fetch_optional(&st.pool).await.ok().flatten();
    // M10: audit interrupt.
    audit::audit_log(
        &st.pool, c.tid, Some(c.uid), "turn.interrupt",
        Some("turn"), Some(&turn_id.to_string()), None,
        trace_id.as_deref(),
    ).await;
    Ok(Json(serde_json::json!({ "turn_id": turn_id, "status": "interrupted" })))
}

// ---------- M3: Approvals ----------
#[derive(Deserialize)]
struct ResolveReq {
    decision: String, // approve | deny | cancel | approve_with_amendment
    /// M7: argv prefix for `approve_with_amendment` (the execpolicy amendment).
    #[serde(default)]
    amendment_command: Option<Vec<String>>,
}

async fn approval_resolve(
    AuthUser(c): AuthUser,
    State(st): State<AppState>,
    Path(aid): Path<i64>,
    Json(req): Json<ResolveReq>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Load ticket status + turn_id + command/policy_decision/risk_level (M6:
    // need these to record learning feedback). Verify tenant + pending.
    let row: Option<(String, i64, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT status, turn_id, command, policy_decision, risk_level
         FROM approval_tickets WHERE id=$1 AND tenant_id=$2",
    )
    .bind(aid).bind(c.tid).fetch_optional(&st.pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (status, turn_id, command, policy_decision, risk_level) = match row {
        Some(s) => s,
        None => return Err((StatusCode::NOT_FOUND, "approval not found".into())),
    };
    if status != "pending" {
        return Err((StatusCode::CONFLICT, format!("approval already {status}")));
    }
    let decision = match req.decision.as_str() {
        "approve" => runtime::DecisionInput::Approve,
        // M7: approve + apply proposed execpolicy amendment (allow prefix).
        "approve_with_amendment" => {
            let cmd = req.amendment_command.clone().unwrap_or_default();
            if cmd.is_empty() {
                return Err((StatusCode::BAD_REQUEST,
                    "amendment_command required for approve_with_amendment".into()));
            }
            runtime::DecisionInput::ApproveWithAmendment { command: cmd }
        }
        "deny" => runtime::DecisionInput::Deny,
        "cancel" => runtime::DecisionInput::Cancel,
        _ => return Err((StatusCode::BAD_REQUEST, "decision must be approve|deny|cancel|approve_with_amendment".into())),
    };
    let new_status = match &decision {
        runtime::DecisionInput::Approve => "approved",
        runtime::DecisionInput::ApproveWithAmendment { .. } => "approved_with_amendment",
        runtime::DecisionInput::Deny => "denied",
        runtime::DecisionInput::Cancel => "cancelled",
    };
    let _ = sqlx::query(
        "UPDATE approval_tickets SET status=$1, decided_by=$2, decided_at=NOW() WHERE id=$3",
    )
    .bind(new_status).bind(c.uid).bind(aid).execute(&st.pool).await;
    let _ = sqlx::query(
        "INSERT INTO approval_audit (approval_id, actor_user_id, action, decision)
         VALUES ($1,$2,'resolved',$3)",
    )
    .bind(aid).bind(c.uid).bind(new_status).execute(&st.pool).await;

    // M6: 记录决策反馈 + 学习（连续 N 次一致且与当前策略矛盾 → 自动提升）。
    // 学习是叠加：不改 resolve 主流程，仅追加 record + learn + 刷 rules。
    let decision_verb = match &decision {
        runtime::DecisionInput::Approve => "approve",
        runtime::DecisionInput::ApproveWithAmendment { .. } => "approve_amendment",
        runtime::DecisionInput::Deny => "deny",
        runtime::DecisionInput::Cancel => "cancel",
    };
    let pattern = policy::extract_pattern(command.as_deref().unwrap_or(""));
    let rec = policy_decision.as_deref().unwrap_or("prompt");
    let _ = policy::record_feedback(
        &st.pool, c.tid, &pattern, decision_verb, rec,
        risk_level.as_deref(), Some(turn_id),
    ).await;
    let learned = policy::learn(&st.pool, c.tid).await.unwrap_or_default();
    if !learned.is_empty() {
        // 热刷新 tenant rules 文件（app-server 下一 turn 自动加载）。
        if let Ok(content) = policy::generate_rules(&st.pool, c.tid).await {
            if !content.is_empty() {
                let _ = policy::write_tenant_rules(c.tid, &st.codex_home, &content);
            }
        }
        tracing::info!(?learned, "policy auto-learned + rules refreshed");
    }

    // M10: audit the approval resolution (decision + command). M11: trace_id.
    let trace_id: Option<String> = sqlx::query_scalar(
        "SELECT trace_id::text FROM turns WHERE id=$1")
        .bind(turn_id).fetch_optional(&st.pool).await.ok().flatten();
    let detail = serde_json::json!({
        "decision": new_status, "turn_id": turn_id, "command": command,
    });
    audit::audit_log(
        &st.pool, c.tid, Some(c.uid), "approval.resolve",
        Some("approval"), Some(&aid.to_string()),
        Some(&detail), trace_id.as_deref(),
    ).await;

    // M5: dispatch the resolve to the driver slot running this turn. If the
    // turn already completed (no slot mapping), the ticket is already marked
    // resolved above — a late resolve is a benign no-op (matches M4's
    // "stale ResolveApproval — ignore" semantics on the driver side).
    let slot_idx = st.turn_slots.lock().await.get(&turn_id).copied();
    if let Some(idx) = slot_idx {
        if let Some(tx) = st.driver_pool.cmd_tx(idx) {
            if tx
                .send(DriverCommand::ResolveApproval { approval_id: aid, decision })
                .is_err()
            {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, "runtime driver gone".into()));
            }
        }
    } else {
        tracing::warn!(aid, turn_id, "approval resolve: turn not in-flight (already completed?)");
    }
    Ok(Json(serde_json::json!({ "approval_id": aid, "status": new_status })))
}

#[derive(Serialize, sqlx::FromRow)]
struct ApprovalRow {
    id: i64,
    thread_id: Uuid,
    turn_id: i64,
    kind: Option<String>,
    status: String,
    command: Option<String>,
    cwd: Option<String>,
    reason: Option<String>,
    policy_decision: Option<String>,
    risk_level: Option<String>,
    created_at: DateTime<Utc>,
}

async fn approvals_list(AuthUser(c): AuthUser, State(st): State<AppState>) -> Result<Json<Vec<ApprovalRow>>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, ApprovalRow>(
        "SELECT id, thread_id, turn_id, kind, status, command, cwd, reason,
                policy_decision, risk_level, created_at
         FROM approval_tickets WHERE tenant_id=$1 AND status='pending'
         ORDER BY created_at DESC LIMIT 100",
    )
    .bind(c.tid).fetch_all(&st.pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

async fn thread_approvals(AuthUser(c): AuthUser, State(st): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<Vec<ApprovalRow>>, (StatusCode, String)> {
    // Verify thread belongs to tenant.
    let owned: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM threads WHERE id=$1 AND tenant_id=$2")
        .bind(id).bind(c.tid).fetch_optional(&st.pool).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if owned.is_none() {
        return Err((StatusCode::NOT_FOUND, "thread not found".into()));
    }
    let rows = sqlx::query_as::<_, ApprovalRow>(
        "SELECT id, thread_id, turn_id, kind, status, command, cwd, reason,
                policy_decision, risk_level, created_at
         FROM approval_tickets WHERE thread_id=$1 ORDER BY created_at DESC LIMIT 200",
    )
    .bind(id).fetch_all(&st.pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

// ---------- M4: Usage metering ----------
#[derive(Deserialize, Default)]
struct UsageQuery { days: Option<i32> }

async fn usage_summary(
    AuthUser(c): AuthUser, State(st): State<AppState>, Query(q): Query<UsageQuery>,
) -> Result<Json<Vec<metering::DailyUsage>>, (StatusCode, String)> {
    let days = q.days.unwrap_or(7).clamp(1, 365);
    let rows = metering::daily_usage(&st.pool, c.tid, days).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

async fn usage_user(
    AuthUser(c): AuthUser, State(st): State<AppState>,
    Path(uid): Path<i64>, Query(q): Query<UsageQuery>,
) -> Result<Json<Vec<metering::DailyUsage>>, (StatusCode, String)> {
    // 仅 admin（*:* 权限）可查任意用户；普通用户只能查自己。
    let is_admin = c.perms.iter().any(|p| p == "*:*");
    if !is_admin && uid != c.uid {
        return Err((StatusCode::FORBIDDEN, "not allowed".into()));
    }
    let days = q.days.unwrap_or(7).clamp(1, 365);
    let rows = metering::daily_usage_user(&st.pool, c.tid, uid, days).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

// ---------- M6: Policy learning observability ----------
async fn policy_feedback(
    AuthUser(c): AuthUser, State(st): State<AppState>, Query(q): Query<UsageQuery>,
) -> Result<Json<Vec<policy::FeedbackRow>>, (StatusCode, String)> {
    let days = q.days.unwrap_or(7).clamp(1, 365);
    let rows = policy::list_feedback(&st.pool, c.tid, days).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

async fn policy_rules(
    AuthUser(c): AuthUser, State(st): State<AppState>,
) -> Result<Json<Vec<policy::PolicyRow>>, (StatusCode, String)> {
    let rows = policy::list_rules(&st.pool, c.tid).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

// ---------- M10: Audit log (WORM) query ----------
#[derive(Deserialize, Default)]
struct AuditQuery {
    action: Option<String>,
    /// RFC-3339 lower bound (defaults to 30 days ago server-side).
    since: Option<String>,
    limit: Option<i64>,
}

async fn audit_logs(
    AuthUser(c): AuthUser, State(st): State<AppState>, Query(q): Query<AuditQuery>,
) -> Result<Json<Vec<audit::AuditLogRow>>, (StatusCode, String)> {
    // Admin (*:*) sees across tenants; everyone else is scoped to own tenant.
    let is_admin = c.perms.iter().any(|p| p == "*:*");
    let tenant_filter = if is_admin { None } else { Some(c.tid) };
    let since = q.since.as_deref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
    });
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let rows = audit::list_audit_logs(&st.pool, tenant_filter, q.action.as_deref(), since, limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

async fn audit_log_get(
    AuthUser(c): AuthUser, State(st): State<AppState>, Path(id): Path<i64>,
) -> Result<Json<audit::AuditLogRow>, (StatusCode, String)> {
    let is_admin = c.perms.iter().any(|p| p == "*:*");
    let tenant_filter = if is_admin { None } else { Some(c.tid) };
    let row = audit::get_audit_log(&st.pool, id, tenant_filter)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match row {
        Some(r) => Ok(Json(r)),
        None => Err((StatusCode::NOT_FOUND, "audit log not found".into())),
    }
}

// ---------- M11: Timeline + trace lookup ----------
async fn thread_timeline_handler(
    AuthUser(c): AuthUser, State(st): State<AppState>, Path(id): Path<Uuid>,
) -> Result<Json<Vec<timeline::TimelineEntry>>, (StatusCode, String)> {
    // Verify the thread belongs to the caller's tenant.
    let owned: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM threads WHERE id=$1 AND tenant_id=$2",
    )
    .bind(id).bind(c.tid).fetch_optional(&st.pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if owned.is_none() {
        return Err((StatusCode::NOT_FOUND, "thread not found".into()));
    }
    let rows = timeline::thread_timeline(&st.pool, id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

async fn trace_lookup_handler(
    AuthUser(c): AuthUser, State(st): State<AppState>, Path(trace_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let is_admin = c.perms.iter().any(|p| p == "*:*");
    let tenant_filter = if is_admin { None } else { Some(c.tid) };
    let v = timeline::trace_lookup(&st.pool, trace_id, tenant_filter).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(v))
}

// ---------- M12: Evaluation center ----------
#[derive(Deserialize)]
struct EvalCaseReq {
    name: String,
    category: Option<String>,
    input: String,
    #[serde(default = "default_completed_status")]
    expected_status: String,
    expected_contains: Option<String>,
}
fn default_completed_status() -> String { "completed".into() }

#[derive(Deserialize)]
struct EvalRunReq { turn_id: i64 }

#[derive(Deserialize, Default)]
struct EvalQuery { limit: Option<i64> }

async fn eval_case_create(
    AuthUser(c): AuthUser, State(st): State<AppState>, Json(req): Json<EvalCaseReq>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let is_admin = c.perms.iter().any(|p| p == "*:*");
    if !is_admin {
        return Err((StatusCode::FORBIDDEN, "admin only".into()));
    }
    let id = eval::create_case(
        &st.pool, c.tid, &req.name, req.category.as_deref(), &req.input,
        &req.expected_status, req.expected_contains.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn eval_cases_list(
    AuthUser(c): AuthUser, State(st): State<AppState>,
) -> Result<Json<Vec<eval::EvalCase>>, (StatusCode, String)> {
    let rows = eval::list_cases(&st.pool, c.tid).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

async fn eval_run(
    AuthUser(c): AuthUser, State(st): State<AppState>,
    Path(case_id): Path<i64>, Json(req): Json<EvalRunReq>,
) -> Result<Json<eval::EvalRun>, (StatusCode, String)> {
    let run = eval::run_eval(&st.pool, c.tid, case_id, req.turn_id).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(run))
}

async fn eval_runs_list(
    AuthUser(c): AuthUser, State(st): State<AppState>, Query(q): Query<EvalQuery>,
) -> Result<Json<Vec<eval::EvalRun>>, (StatusCode, String)> {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let rows = eval::list_runs(&st.pool, c.tid, limit).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

#[derive(Serialize, sqlx::FromRow)]
struct ItemRow {
    id: i64,
    turn_id: i64,
    seq: i64,
    item_type: String,
    content_ref: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize, Default)]
struct ItemsQuery { since: Option<i64> }

async fn items_list(AuthUser(_c): AuthUser, State(st): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<ItemsQuery>) -> Result<Json<Vec<ItemRow>>, (StatusCode, String)> {
    let rows = if let Some(s) = q.since {
        sqlx::query_as::<_, ItemRow>("SELECT id, turn_id, seq, item_type, content_ref, created_at FROM items WHERE thread_id=$1 AND seq>$2 ORDER BY seq ASC LIMIT 500")
            .bind(id).bind(s).fetch_all(&st.pool).await
    } else {
        sqlx::query_as::<_, ItemRow>("SELECT id, turn_id, seq, item_type, content_ref, created_at FROM items WHERE thread_id=$1 ORDER BY seq ASC LIMIT 500")
            .bind(id).fetch_all(&st.pool).await
    }.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

// ---------- Middleware: idempotency (POST + Idempotency-Key) ----------
async fn idempotency_layer(State(st): State<AppState>, req: axum::extract::Request, next: Next) -> Response {
    if req.method() != Method::POST {
        return next.run(req).await;
    }
    let key = match req.headers().get("Idempotency-Key").and_then(|h| h.to_str().ok()) {
        Some(k) if !k.is_empty() => k.to_string(),
        _ => return next.run(req).await,
    };
    // Cache hit?
    if let Ok(Some((cached,))) = sqlx::query_as::<_, (Option<Value>,)>(
        "SELECT response_json FROM idempotency_records WHERE key=$1 AND expires_at > NOW()",
    )
    .bind(&key)
    .fetch_optional(&st.pool)
    .await
    {
        if let Some(v) = cached {
            return Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .header("x-idempotent-replay", "true")
                .body(axum::body::Body::from(v.to_string()))
                .unwrap();
        }
    }
    let resp = next.run(req).await;
    if !resp.status().is_success() {
        return resp;
    }
    let (parts, body) = resp.into_parts();
    let bytes = axum::body::to_bytes(body, 256 * 1024).await.unwrap_or_default();
    let payload = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
    let _ = sqlx::query(
        "INSERT INTO idempotency_records (key, response_json, expires_at) VALUES ($1, $2, NOW() + interval '24 hours') ON CONFLICT (key) DO NOTHING",
    )
    .bind(&key)
    .bind(&payload)
    .execute(&st.pool)
    .await;
    Response::from_parts(parts, axum::body::Body::from(bytes))
}

// ---------- Middleware: per-user rate limit (DB token bucket, 1-min window) ----------
async fn user_rate_limit(State(st): State<AppState>, req: axum::extract::Request, next: Next) -> Response {
    let uid = match req.headers().get(axum::http::header::AUTHORIZATION).and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer ")) {
        Some(tok) => match st.jwt.verify(tok) { Ok(c) => Some(c.uid), Err(_) => None },
        None => None,
    };
    if let Some(uid) = uid {
        let key = format!("u:{uid}");
        let allowed: bool = sqlx::query_scalar(
            "INSERT INTO rate_limit_buckets (bucket_key, user_id, tokens, last_refill_at)
             VALUES ($1, $2, 1, NOW())
             ON CONFLICT (bucket_key) DO UPDATE
               SET tokens = CASE WHEN rate_limit_buckets.last_refill_at < NOW() - interval '1 minute'
                                 THEN 1 ELSE rate_limit_buckets.tokens + 1 END,
                   last_refill_at = CASE WHEN rate_limit_buckets.last_refill_at < NOW() - interval '1 minute'
                                 THEN NOW() ELSE rate_limit_buckets.last_refill_at END
             RETURNING (tokens <= 100)",
        )
        .bind(&key).bind(uid)
        .fetch_one(&st.pool)
        .await
        .unwrap_or(false);
        if !allowed {
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("retry-after", "60")
                .body(axum::body::Body::empty())
                .unwrap();
        }
    }
    next.run(req).await
}

// ---------- Middleware: stateless auth gate ----------
async fn require_auth_stateless(req: axum::extract::Request, next: Next) -> Response {
    let path = req.uri().path();
    let is_public = path == "/health" || path == "/v1/auth/login" || path.starts_with("/v1/ws/");
    if is_public {
        return next.run(req).await;
    }
    let has_bearer = req.headers().get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.starts_with("Bearer "))
        .unwrap_or(false);
    if !has_bearer && path.starts_with("/v1") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(req).await
}

// silence unused import warnings for types used in macros only
#[allow(dead_code)]
fn _touch(_: HeaderMap, _: Method) {}

// ---------- M13: 知识库 RAG (pgvector) ----------

#[derive(Deserialize)]
struct KbCreateReq {
    workspace_id: Option<i64>,
    name: String,
    description: Option<String>,
}

async fn kb_create(
    AuthUser(c): AuthUser, State(st): State<AppState>, Json(req): Json<KbCreateReq>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let id = kb::create_kb(
        &st.pool, c.tid, req.workspace_id, &req.name, req.description.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn kb_list(
    AuthUser(c): AuthUser, State(st): State<AppState>,
) -> Result<Json<Vec<kb::KbRow>>, (StatusCode, String)> {
    let rows = kb::list_kbs(&st.pool, c.tid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct KbDocIngestReq {
    title: String,
    content: String,
    source_uri: Option<String>,
}

async fn kb_doc_ingest(
    AuthUser(c): AuthUser, State(st): State<AppState>, Path(id): Path<i64>,
    Json(req): Json<KbDocIngestReq>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let did = kb::ingest_document(
        &st.pool, c.tid, id, &req.title, &req.content, req.source_uri.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "id": did })))
}

async fn kb_doc_list(
    AuthUser(c): AuthUser, State(st): State<AppState>, Path(id): Path<i64>,
) -> Result<Json<Vec<kb::KbDocRow>>, (StatusCode, String)> {
    let rows = kb::list_documents(&st.pool, c.tid, id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows))
}

async fn kb_doc_delete(
    AuthUser(c): AuthUser, State(st): State<AppState>,
    Path((_id, did)): Path<(i64, i64)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ok = kb::delete_document(&st.pool, c.tid, did)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "deleted": ok })))
}

#[derive(Deserialize)]
struct KbSearchReq {
    query: String,
    keyword: Option<String>,
    #[serde(default = "default_top_k")]
    top_k: i64,
}
fn default_top_k() -> i64 { 5 }

async fn kb_search(
    AuthUser(c): AuthUser, State(st): State<AppState>, Path(id): Path<i64>,
    Json(req): Json<KbSearchReq>,
) -> Result<Json<Vec<kb::SearchHit>>, (StatusCode, String)> {
    let hits = kb::search(
        &st.pool, c.tid, id, &req.query, req.keyword.as_deref(), req.top_k,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(hits))
}
