//! WebSocket gateway: permission-driven subscription + revocation disconnect (T1-4).
//!
//! M2: live push via per-thread `tokio::sync::broadcast` channel (instant,
//! <1s) with a periodic poll fallback to refill any gaps. Every ~5s the
//! user's live membership is re-checked; revocation closes the socket (AC4.2).

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::sink::SinkExt;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::auth::Claims;
use crate::http_server::AppState;

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: String,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(st): State<AppState>,
    Path(thread_id): Path<Uuid>,
    Query(q): Query<WsQuery>,
) -> impl IntoResponse {
    let claims = match st.jwt.verify(&q.token) {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };
    // AC4.1: thread must belong to the user's tenant.
    let accessible: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM threads WHERE id=$1 AND tenant_id=$2",
    )
    .bind(thread_id)
    .bind(claims.tid)
    .fetch_optional(&st.pool)
    .await
    .ok()
    .flatten();
    if accessible.is_none() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |socket| run(socket, st, thread_id, claims))
}

async fn run(mut socket: WebSocket, st: AppState, thread_id: Uuid, claims: Claims) {
    let mut last_seq: i64 = 0;
    let mut perm_tick = 0u32;
    // Subscribe to the per-thread broadcast channel (live push).
    let rx: Option<broadcast::Receiver<Value>> = {
        let map = st.broadcast.lock().await;
        map.get(&thread_id).map(|t| t.subscribe())
    };
    tracing::info!(uid = claims.uid, thread_id = %thread_id, "ws attached");

    let mut broadcast_rx = rx;
    let mut gap_tick = 0u32;

    loop {
        // 1. Replay any persisted items since last_seq (catches up on connect
        //    + refills gaps if broadcast lagged/dropped).
        if let Ok(rows) = sqlx::query_as::<_, (i64, String, Option<String>)>(
            "SELECT seq, item_type, content_ref FROM items WHERE thread_id=$1 AND seq>$2 ORDER BY seq ASC LIMIT 100",
        )
        .bind(thread_id)
        .bind(last_seq)
        .fetch_all(&st.pool)
        .await
        {
            for (seq, kind, content) in rows {
                let frame = serde_json::json!({
                    "thread_id": thread_id, "seq": seq, "type": kind, "content": content,
                });
                if socket.send(Message::Text(frame.to_string().into())).await.is_err() {
                    return;
                }
                last_seq = seq.max(last_seq);
            }
        }

        // 2. Live push via broadcast (non-blocking try_recv for ~1s window).
        if let Some(brx) = broadcast_rx.as_mut() {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
            loop {
                tokio::select! {
                    Ok(frame) = brx.recv(), if tokio::time::Instant::now() < deadline => {
                        if let Some(seq) = frame.get("seq").and_then(|v| v.as_i64()) {
                            if seq > last_seq { last_seq = seq; }
                        }
                        if socket.send(Message::Text(frame.to_string().into())).await.is_err() {
                            return;
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => break,
                    else => break,
                }
            }
        } else {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        // 3. Periodic membership re-check (AC4.2) + gap backfill tick.
        perm_tick = perm_tick.saturating_add(1);
        gap_tick = gap_tick.saturating_add(1);
        if perm_tick >= 5 {
            perm_tick = 0;
            let still: Option<(i64,)> = sqlx::query_as(
                "SELECT u.id FROM users u JOIN tenant_memberships m ON m.user_id=u.id
                 WHERE u.id=$1 AND u.status='active'",
            )
            .bind(claims.uid)
            .fetch_optional(&st.pool)
            .await
            .ok()
            .flatten();
            if still.is_none() {
                let _ = socket
                    .send(Message::Text(r#"{"event":"revoked"}"#.into()))
                    .await;
                let _ = socket.close().await;
                tracing::info!(uid = claims.uid, "ws closed: permission revoked");
                return;
            }
        }
    }
}
