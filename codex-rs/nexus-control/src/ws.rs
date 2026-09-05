//! WebSocket gateway: permission-driven subscription + revocation disconnect (T1-4).
//!
//! Client connects: `WS /v1/ws/threads/:id/events?token=<jwt>`. The server
//! verifies the JWT, checks thread access (owner or same-tenant), then polls
//! `items` for new rows and pushes JSON frames. Every few seconds it re-checks
//! the user's live membership; revocation closes the socket (AC4.2).

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::sink::SinkExt;
use serde::Deserialize;
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
    tracing::info!(uid = claims.uid, thread_id = %thread_id, "ws attached");

    loop {
        // Push any new items since last_seq.
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
                last_seq = seq;
            }
        }

        // AC4.2: every ~5s re-check live membership; revocation closes the socket.
        perm_tick = perm_tick.saturating_add(1);
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

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
