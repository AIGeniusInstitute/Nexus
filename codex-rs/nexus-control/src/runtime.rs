//! Runtime adapter (T2-1): bridges the synchronous blocking Codex app-server
//! stdio client to the async axum runtime via a dedicated driver thread +
//! tokio channels.
//!
//! Architecture: a single `std::thread` owns the `AppServerProcess` and
//! performs all blocking stdio I/O. The async side sends `DriverCommand`s via
//! a `std::sync::mpsc::Sender` (Clone-able) and receives `TurnEvent`s via a
//! `tokio::sync::mpsc::UnboundedReceiver`. Commands serialize naturally — a
//! single app-server process serves one turn at a time (acceptable for M2
//! single-tenant MVP; M5+ adds a K8s pool).
//!
//! `interrupt` = kill + respawn + `thread/resume` (the M0-proven path; we do
//! not introduce concurrent stdin/stdout select — Simplicity First).

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use anyhow::Context;
use codex_app_server_protocol::{
    AskForApproval, SandboxPolicy, ServerNotification, ThreadResumeParams,
    ThreadStartParams, TurnStartParams, UserInput,
};
use serde_json::Value;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use uuid::Uuid;

use crate::stdio_client::AppServerProcess;

/// Commands sent from the async side to the driver thread.
#[derive(Debug)]
pub enum DriverCommand {
    /// Run a turn. `codex_thread_id` is `Some` when resuming an existing
    /// app-server thread (read from `threads.codex_thread_id`); `None` for a
    /// fresh `thread/start`. `start_seq` is the max seq already persisted for
    /// this thread (so the driver continues the monotonic thread-level seq).
    RunTurn {
        thread_id: Uuid,
        codex_thread_id: Option<String>,
        turn_db_id: i64,
        input: String,
        start_seq: i64,
    },
    /// Interrupt the in-flight turn: kill the app-server child. The next
    /// `RunTurn` respawns + resumes.
    Interrupt,
    Shutdown,
}

/// A normalized event emitted by the driver and consumed by the async side.
#[derive(Debug, Clone)]
pub struct TurnEvent {
    pub thread_id: Uuid,
    pub turn_db_id: i64,
    /// Monotonic thread-level sequence (continues across turns / resumes).
    pub seq: i64,
    /// Wire method / notification type, e.g. `item/started`, `turn/completed`.
    pub item_type: String,
    /// app-server `ThreadItem.id` (String) — the idempotency key for `items`.
    pub codex_item_id: Option<String>,
    /// Serialized item / notification payload (stored as `items.content_ref`).
    pub content_ref: Option<String>,
    /// Raw notification JSON (stored as `app_server_events.event_json`).
    pub raw_json: Value,
    /// Token usage, populated on `thread/tokenUsage/updated`.
    pub usage: Option<Usage>,
    /// The resolved app-server thread id (populated on the synthetic
    /// `thread/ready` event after thread/start or resume).
    pub codex_thread_id: Option<String>,
    pub is_turn_completed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micros: i64,
    pub model: Option<String>,
}

/// Handle held by the async side (behind `Arc<tokio::sync::Mutex<..>>`).
pub struct RuntimeHandle {
    pub cmd_tx: Sender<DriverCommand>,
    pub event_rx: UnboundedReceiver<TurnEvent>,
}

/// Spawn the runtime driver thread and return the async-side handle.
pub fn spawn(codex_bin: PathBuf, codex_home: PathBuf) -> RuntimeHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel::<DriverCommand>();
    let (event_tx, event_rx) = unbounded_channel::<TurnEvent>();
    let _thread = thread::Builder::new()
        .name("nexus-runtime-driver".into())
        .spawn(move || driver_loop(codex_bin, codex_home, cmd_rx, event_tx))
        .expect("spawn nexus runtime driver");
    RuntimeHandle { cmd_tx, event_rx }
}

/// The driver thread main loop. Owns the app-server process; blocking stdio
/// lives here, never on the async runtime.
fn driver_loop(
    codex_bin: PathBuf,
    codex_home: PathBuf,
    cmd_rx: Receiver<DriverCommand>,
    event_tx: UnboundedSender<TurnEvent>,
) {
    let mut proc: Option<AppServerProcess> = None;

    for cmd in cmd_rx.iter() {
        match cmd {
            DriverCommand::Shutdown => break,

            DriverCommand::Interrupt => {
                if let Some(mut p) = proc.take() {
                    p.kill();
                }
            }

            DriverCommand::RunTurn {
                thread_id,
                codex_thread_id,
                turn_db_id,
                input,
                start_seq,
            } => {
                // Lazily spawn + initialize on first use, after interrupt, or
                // when the previous child has died (e.g. killed externally).
                if !proc.as_mut().is_some_and(|p| p.is_alive()) {
                    if let Some(mut dead) = proc.take() {
                        dead.kill();
                    }
                    match AppServerProcess::spawn(&codex_bin, &codex_home)
                        .and_then(|mut p| {
                            p.initialize().context("app-server initialize")?;
                            Ok(p)
                        }) {
                        Ok(p) => proc = Some(p),
                        Err(e) => {
                            tracing::error!(error = %e, "runtime: spawn app-server failed");
                            emit_error(&event_tx, thread_id, turn_db_id, start_seq, &e);
                            continue;
                        }
                    }
                }
                let p = proc.as_mut().unwrap();

                // Resolve the app-server thread id: resume if known (codex
                // persists thread state to CODEX_HOME, so a fresh process can
                // resume an existing thread), else start a new one.
                let resolved = if let Some(cid) = codex_thread_id.clone() {
                    match p.thread_resume(ThreadResumeParams {
                        thread_id: cid.clone(),
                        ..Default::default()
                    }) {
                        Ok(_) => cid,
                        Err(e) => {
                            tracing::error!(error = %e, "thread/resume failed");
                            emit_error(&event_tx, thread_id, turn_db_id, start_seq, &e);
                            continue;
                        }
                    }
                } else {
                    match p.thread_start(ThreadStartParams::default()) {
                        Ok(r) => r.thread.id,
                        Err(e) => {
                            emit_error(&event_tx, thread_id, turn_db_id, start_seq, &e);
                            continue;
                        }
                    }
                };

                // Synthetic "thread/ready" carries the resolved codex_thread_id
                // so the async side can persist it on the threads row.
                let _ = event_tx.send(TurnEvent {
                    thread_id,
                    turn_db_id,
                    seq: 0,
                    item_type: "thread/ready".into(),
                    codex_item_id: None,
                    content_ref: None,
                    raw_json: serde_json::json!({ "thread_id": resolved }),
                    codex_thread_id: Some(resolved.clone()),
                    usage: None,
                    is_turn_completed: false,
                });

                // turn/start.
                let turn_resp = match p.turn_start(TurnStartParams {
                    thread_id: resolved.clone(),
                    client_user_message_id: None,
                    input: vec![UserInput::Text {
                        text: input,
                        text_elements: Vec::new(),
                    }],
                    approval_policy: Some(AskForApproval::Never),
                    sandbox_policy: Some(SandboxPolicy::ReadOnly {
                        network_access: true,
                    }),
                    ..Default::default()
                }) {
                    Ok(r) => r,
                    Err(e) => {
                        emit_error(&event_tx, thread_id, turn_db_id, start_seq, &e);
                        continue;
                    }
                };
                let _codex_turn_id = turn_resp.turn.id;

                // Drain notifications until turn/completed.
                let mut seq = start_seq;
                loop {
                    let notif = match p.next_notification() {
                        Ok(n) => n,
                        Err(e) => {
                            tracing::error!(error = %e, "runtime: notification stream ended");
                            emit_error(&event_tx, thread_id, turn_db_id, seq, &e);
                            break;
                        }
                    };
                    let method = notif.method.clone();
                    let raw = serde_json::to_value(&notif).unwrap_or(Value::Null);
                    seq += 1;

                    let (codex_item_id, content_ref, usage, is_completed) =
                        match ServerNotification::try_from(notif) {
                            Ok(sn) => map_notification(&sn),
                            Err(e) => {
                                tracing::warn!(error = %e, %method, "runtime: notification parse failed");
                                (None, None, None, method == "turn/completed")
                            }
                        };

                    let _ = event_tx.send(TurnEvent {
                        thread_id,
                        turn_db_id,
                        seq,
                        item_type: method,
                        codex_item_id,
                        content_ref,
                        raw_json: raw,
                        usage,
                        codex_thread_id: None,
                        is_turn_completed: is_completed,
                    });

                    if is_completed {
                        break;
                    }
                }
            }
        }
    }
    // On loop exit, kill any lingering child.
    if let Some(mut p) = proc.take() {
        p.kill();
    }
}

/// Map a `ServerNotification` to (codex_item_id, content_ref, usage, is_completed).
fn map_notification(
    n: &ServerNotification,
) -> (Option<String>, Option<String>, Option<Usage>, bool) {
    match n {
        ServerNotification::ItemStarted(p) => {
            let id = item_id(&p.item);
            let content = serde_json::to_string(&p.item).ok();
            (id, content, None, false)
        }
        ServerNotification::ItemCompleted(p) => {
            let id = item_id(&p.item);
            let content = serde_json::to_string(&p.item).ok();
            (id, content, None, false)
        }
        ServerNotification::TurnCompleted(_) => (None, None, None, true),
        ServerNotification::ThreadTokenUsageUpdated(p) => {
            let t = &p.token_usage.total;
            (
                None,
                None,
                Some(Usage {
                    input_tokens: t.input_tokens,
                    output_tokens: t.output_tokens,
                    cost_micros: 0, // cost derivation is M4
                    model: None,
                }),
                false,
            )
        }
        _ => (None, None, None, false),
    }
}

/// Extract the `id` field from a `ThreadItem` (all variants carry `id: String`).
fn item_id(item: &codex_app_server_protocol::ThreadItem) -> Option<String> {
    let val = serde_json::to_value(item).ok()?;
    val.get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Emit a synthetic error event so the async side can fail the turn gracefully.
fn emit_error(
    tx: &UnboundedSender<TurnEvent>,
    thread_id: Uuid,
    turn_db_id: i64,
    seq: i64,
    e: &anyhow::Error,
) {
    let _ = tx.send(TurnEvent {
        thread_id,
        turn_db_id,
        seq: seq + 1,
        item_type: "nexus/error".into(),
        codex_item_id: None,
        content_ref: Some(e.to_string()),
        raw_json: serde_json::json!({ "error": e.to_string() }),
        usage: None,
        codex_thread_id: None,
        is_turn_completed: true, // treat error as turn termination
    });
}
