//! HTTP API gateway: axum router + handlers + idempotency/rate-limit middleware (T1-2/T1-3).
//! M2: turn_start drives the real app-server runtime; interrupt endpoint added.

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
use crate::runtime::{self, RuntimeHandle};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt: Arc<JwtIssuer>,
    pub auth: Arc<dyn AuthProvider>,
    /// Single runtime driver handle (M2: single app-server process, turns
    /// serialized via this mutex).
    pub runtime: Arc<Mutex<RuntimeHandle>>,
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

    // Create the turn row (status running).
    let trow: (i64,) = sqlx::query_as(
        "INSERT INTO turns (thread_id, status, started_at) VALUES ($1, 'running', NOW()) RETURNING id",
    )
    .bind(id).fetch_one(&st.pool).await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let turn_db_id = trow.0;

    // Max seq persisted for this thread (continue monotonic thread-level seq).
    let max_seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) FROM app_server_events WHERE thread_id=$1",
    )
    .bind(id).fetch_one(&st.pool).await
    .unwrap_or(0);

    let input = req.input.unwrap_or_else(|| "(empty turn)".into());

    // Dispatch to the runtime driver (single mutex serializes turns).
    let mut rh = st.runtime.lock().await;
    if rh.cmd_tx
        .send(runtime::DriverCommand::RunTurn {
            thread_id: id,
            codex_thread_id: codex_thread_id.clone(),
            turn_db_id,
            input: input.clone(),
            start_seq: max_seq,
        })
        .is_err()
    {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "runtime driver gone".into()));
    }

    let bcast = thread_broadcast(&st, id).await;
    // Drain events until turn/completed (or nexus/error).
    let mut last_usage: Option<runtime::Usage> = None;
    let mut resolved_codex_thread_id: Option<String> = codex_thread_id.clone();
    while let Some(ev) = rh.event_rx.recv().await {
        // Persist codex_thread_id on first resolve.
        if let Some(cid) = &ev.codex_thread_id {
            resolved_codex_thread_id = Some(cid.clone());
            let _ = sqlx::query("UPDATE threads SET codex_thread_id=$1 WHERE id=$2")
                .bind(cid).bind(id).execute(&st.pool).await;
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

        // Broadcast to WS subscribers (ignore send errors — no subscribers).
        let frame = serde_json::json!({
            "thread_id": id, "seq": ev.seq, "type": ev.item_type,
            "content": ev.content_ref, "item_id": ev.codex_item_id,
        });
        let _ = bcast.send(frame);

        if ev.is_turn_completed {
            break;
        }
    }
    drop(rh);

    // Finalize the turn.
    let status = match &last_usage {
        Some(u) => {
            let _ = sqlx::query(
                "UPDATE turns SET status='completed', completed_at=NOW(),
                     input_tokens=$1, output_tokens=$2, cost_micros=$3
                 WHERE id=$4",
            )
            .bind(u.input_tokens).bind(u.output_tokens).bind(u.cost_micros)
            .bind(turn_db_id).execute(&st.pool).await;
            "completed"
        }
        None => {
            // No usage observed (mock gateway) — still mark completed.
            let _ = sqlx::query(
                "UPDATE turns SET status='completed', completed_at=NOW() WHERE id=$1",
            )
            .bind(turn_db_id).execute(&st.pool).await;
            "completed"
        }
    };

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
    let rh = st.runtime.lock().await;
    let _ = rh.cmd_tx.send(runtime::DriverCommand::Interrupt);
    drop(rh);
    let _ = sqlx::query("UPDATE turns SET status='interrupted', completed_at=NOW() WHERE id=$1")
        .bind(turn_id).execute(&st.pool).await;
    Ok(Json(serde_json::json!({ "turn_id": turn_id, "status": "interrupted" })))
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
    // Only cache 2xx.
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
    // Parse JWT manually (avoid consuming the AuthUser extractor before the handler).
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

// ---------- Middleware: stateless auth gate (for protected routes only) ----------
// Routes mounted under /v1 require a Bearer JWT except /v1/auth/login.
async fn require_auth_stateless(req: axum::extract::Request, next: Next) -> Response {
    let path = req.uri().path();
    let is_public = path == "/health" || path == "/v1/auth/login" || path.starts_with("/v1/ws/");
    if is_public {
        return next.run(req).await;
    }
    // Non-public /v1 paths: 401 if no Bearer.
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
