//! Runtime adapter: bridges the synchronous blocking Codex app-server stdio
//! client to the async axum runtime via a dedicated driver thread + tokio
//! channels.
//!
//! M2: a single `std::thread` owns the `AppServerProcess` and performs all
//! blocking stdio I/O. The async side sends `DriverCommand`s via a
//! `std::sync::mpsc::Sender` (Clone-able) and receives `TurnEvent`s via a
//! `tokio::sync::mpsc::UnboundedReceiver`.
//!
//! M3: HITL approval bridge. The driver **surfaces** app-server approval
//! requests (instead of auto-accepting), emits an `approval/requested` event,
//! then **parks** on the command channel until a `ResolveApproval` (or
//! `Interrupt`) arrives. On resolve it writes the JSON-RPC response back and
//! resumes draining. The `RuntimeHandle` is split: `cmd_tx` is `Clone` and
//! stored lock-free in `AppState` (so the approval-resolve handler can send
//! without contending the mutex that `turn_start` holds while draining
//! `event_rx`) — this breaks the deadlock that would otherwise occur.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use anyhow::Context;
use codex_app_server_protocol::{
    AskForApproval, SandboxPolicy, ServerNotification, ThreadResumeParams, ThreadStartParams,
    TurnStartParams, UserInput,
};
use serde_json::Value;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use uuid::Uuid;

use crate::stdio_client::{AppServerProcess, StreamEvent};
// Re-export so handlers can reference `runtime::ApprovalKind` / `runtime::DecisionInput`.
pub use crate::stdio_client::{ApprovalKind, DecisionInput};

/// Commands sent from the async side to the driver thread.
#[derive(Debug)]
pub enum DriverCommand {
    /// Run a turn. `codex_thread_id` is `Some` when resuming an existing
    /// app-server thread; `None` for a fresh `thread/start`. `start_seq` is
    /// the max seq already persisted for this thread.
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
    /// M3: resolve a pending approval. `approval_id` is the Nexus-side ticket
    /// id generated when the approval request was surfaced.
    ResolveApproval {
        approval_id: i64,
        decision: DecisionInput,
    },
    Shutdown,
}

/// A normalized event emitted by the driver and consumed by the async side.
#[derive(Debug, Clone)]
pub struct TurnEvent {
    pub thread_id: Uuid,
    pub turn_db_id: i64,
    pub seq: i64,
    /// Wire method / notification type, e.g. `item/started`, `turn/completed`,
    /// `approval/requested`, `nexus/error`.
    pub item_type: String,
    pub codex_item_id: Option<String>,
    pub content_ref: Option<String>,
    pub raw_json: Value,
    pub usage: Option<Usage>,
    pub codex_thread_id: Option<String>,
    pub is_turn_completed: bool,
    /// M3: present only for `approval/requested` events. Carries the
    /// Nexus-side approval id + the command/cwd/kind for ticket persistence.
    pub approval: Option<ApprovalInfo>,
}

/// M3: approval metadata carried on an `approval/requested` TurnEvent.
#[derive(Debug, Clone)]
pub struct ApprovalInfo {
    pub approval_id: i64,
    pub kind: ApprovalKind,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub reason: Option<String>,
    pub raw_params: Value,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_micros: i64,
    pub model: Option<String>,
}

/// The async-side runtime state for a SINGLE driver (M2/M3). Kept for
/// backward-compat with the M0 PoC CLIs and any single-driver caller.
/// `cmd_tx` is `Clone` and stored lock-free; `event_rx` is consumed by one
/// `turn_start` drain. M5 `spawn_pool` supersedes this for multi-driver
/// concurrency — the per-slot state there is managed by `DriverPool`/`DriverGuard`.
pub struct RuntimeHandle {
    pub cmd_tx: Sender<DriverCommand>,
    pub event_rx: UnboundedReceiver<TurnEvent>,
}

/// Spawn the runtime driver thread and return the async-side handle.
///
/// `start_approval_id` seeds the monotonic approval-id counter (M3). It MUST
/// be `> max(approval_tickets.id)` so the driver-generated ids never collide
/// with existing DB rows (callers typically pass `SELECT max(id)+1`).
pub fn spawn(codex_bin: PathBuf, codex_home: PathBuf, start_approval_id: i64) -> RuntimeHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel::<DriverCommand>();
    let (event_tx, event_rx) = unbounded_channel::<TurnEvent>();
    let approval_counter = Arc::new(AtomicI64::new(start_approval_id.max(1)));
    let _thread = thread::Builder::new()
        .name("nexus-runtime-driver".into())
        .spawn(move || {
            driver_loop(codex_bin, codex_home, cmd_rx, event_tx, approval_counter)
        })
        .expect("spawn nexus runtime driver");
    RuntimeHandle { cmd_tx, event_rx }
}

// ===========================================================================
// M5: Driver pool — N independent driver threads, each owning its own
// app-server process + event channel. Breaks the global-mutex serialization
// of M3/M4 (one `event_rx` behind an `Arc<Mutex<..>>` held for the whole
// turn). turn_start `acquire()`s a free slot, drains that slot's `event_rx`
// exclusively (no shared mutex), and `release()`s on turn end via
// `DriverGuard`'s `Drop`.
// ===========================================================================

/// One pool slot. `event_rx` is `Some` when the slot is free, taken out
/// (`None`) while a turn is draining it.
struct DriverSlot {
    event_rx: Option<UnboundedReceiver<TurnEvent>>,
}

/// A pool of N independent driver threads.
pub struct DriverPool {
    slots: Vec<std::sync::Mutex<DriverSlot>>,
    /// Clone-able command senders, indexed by slot. Resolve/interrupt
    /// handlers route to the slot holding the in-flight turn via `turn_slots`.
    cmd_txs: Vec<Sender<DriverCommand>>,
    /// Free-slot queue. `acquire` dequeues an idx; `DriverGuard::drop`
    /// enqueues it back. Unbounded (bounded by pool_size in practice).
    free_tx: tokio::sync::mpsc::UnboundedSender<usize>,
    free_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<usize>>,
}

/// RAII guard over an acquired pool slot. Drains `event_rx` exclusively;
/// on drop, returns the receiver to its slot and enqueues the slot back to
/// the free-list.
pub struct DriverGuard {
    pub idx: usize,
    event_rx: Option<UnboundedReceiver<TurnEvent>>,
    pool: Arc<DriverPool>,
}

impl DriverGuard {
    /// Clone the command sender for this slot (for the initial `RunTurn`
    /// dispatch from `turn_start`).
    pub fn cmd_tx(&self) -> Sender<DriverCommand> {
        self.pool.cmd_txs[self.idx].clone()
    }

    /// Exclusive mutable access to this slot's event stream (drain it in
    /// `turn_start`). The receiver is returned to its slot on `Drop`.
    pub fn event_rx_mut(&mut self) -> &mut UnboundedReceiver<TurnEvent> {
        self.event_rx.as_mut().expect("guard event_rx taken before drain")
    }
}

impl Drop for DriverGuard {
    fn drop(&mut self) {
        // Return the event receiver to its slot.
        if let Some(rx) = self.event_rx.take() {
            if let Ok(mut slot) = self.pool.slots[self.idx].lock() {
                slot.event_rx = Some(rx);
            }
        }
        // Mark this slot free again.
        let _ = self.pool.free_tx.send(self.idx);
    }
}

impl DriverPool {
    /// Acquire a free driver slot, waiting until one is available. Returns
    /// `None` only if the free-list channel is closed (pool dropped).
    pub async fn acquire(self: &Arc<Self>) -> Option<DriverGuard> {
        let idx = self.free_rx.lock().await.recv().await?;
        let event_rx = {
            let mut slot = self.slots[idx].lock().unwrap();
            slot.event_rx.take()
        };
        Some(DriverGuard { idx, event_rx, pool: Arc::clone(self) })
    }

    /// Look up the command sender for slot `idx` (for resolve/interrupt
    /// routing). Returns `None` for an out-of-range idx.
    pub fn cmd_tx(&self, idx: usize) -> Option<Sender<DriverCommand>> {
        self.cmd_txs.get(idx).cloned()
    }
}

/// Spawn a pool of `pool_size` independent driver threads and return the
/// async-side pool handle.
///
/// `start_approval_id` seeds the GLOBAL monotonic approval-id counter (M5):
/// unlike M3's per-driver counter, this counter is shared via `AtomicI64` so
/// ids generated by different drivers never collide. Callers typically pass
/// `SELECT max(id)+1 FROM approval_tickets`.
pub fn spawn_pool(
    codex_bin: PathBuf,
    codex_home: PathBuf,
    pool_size: usize,
    start_approval_id: i64,
) -> Arc<DriverPool> {
    let pool_size = pool_size.max(1);
    let (free_tx, free_rx) = tokio::sync::mpsc::unbounded_channel::<usize>();
    let approval_counter = Arc::new(AtomicI64::new(start_approval_id.max(1)));
    let mut slots = Vec::with_capacity(pool_size);
    let mut cmd_txs = Vec::with_capacity(pool_size);
    for i in 0..pool_size {
        let (cmd_tx, cmd_rx) = mpsc::channel::<DriverCommand>();
        let (event_tx, event_rx) = unbounded_channel::<TurnEvent>();
        let counter = Arc::clone(&approval_counter);
        // Clone per-iteration so each closure owns its own copy (the loop
        // body runs pool_size times; a bare `move ||` would capture the
        // outer PathBuf by move on the first iteration).
        let bin = codex_bin.clone();
        let home = codex_home.clone();
        let _thread = thread::Builder::new()
            .name(format!("nexus-driver-{i}"))
            .spawn(move || {
                driver_loop(bin, home, cmd_rx, event_tx, counter)
            })
            .expect("spawn nexus driver");
        slots.push(std::sync::Mutex::new(DriverSlot { event_rx: Some(event_rx) }));
        cmd_txs.push(cmd_tx);
        let _ = free_tx.send(i); // initially all slots free
    }
    Arc::new(DriverPool {
        slots,
        cmd_txs,
        free_tx,
        free_rx: tokio::sync::Mutex::new(free_rx),
    })
}

/// The driver thread main loop.
fn driver_loop(
    codex_bin: PathBuf,
    codex_home: PathBuf,
    cmd_rx: Receiver<DriverCommand>,
    event_tx: UnboundedSender<TurnEvent>,
    approval_counter: Arc<AtomicI64>,
) {
    let mut proc: Option<AppServerProcess> = None;
    // M5: global monotonic approval id shared across all drivers (AtomicI64),
    // seeded from `start_approval_id` (DB max+1) so it never collides with
    // existing rows OR with ids generated by sibling drivers.
    let simulate_approval = std::env::var("NEXUS_SIMULATE_APPROVAL").is_ok();

    for cmd in cmd_rx.iter() {
        match cmd {
            DriverCommand::Shutdown => break,

            DriverCommand::Interrupt => {
                if let Some(mut p) = proc.take() {
                    p.kill();
                }
            }

            DriverCommand::ResolveApproval { .. } => {
                // A resolve with no parked approval is a no-op (e.g. the turn
                // already completed / was interrupted). Drop it.
                tracing::warn!("runtime: ResolveApproval with no parked approval — drop");
            }

            DriverCommand::RunTurn {
                thread_id,
                codex_thread_id,
                turn_db_id,
                input,
                start_seq,
            } => {
                // Lazily spawn + initialize on first use, after interrupt, or
                // when the previous child has died.
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

                // Resolve the app-server thread id (resume if known).
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
                    approval: None,
                });

                let turn_resp = match p.turn_start(TurnStartParams {
                    thread_id: resolved.clone(),
                    client_user_message_id: None,
                    input: vec![UserInput::Text {
                        text: input,
                        text_elements: Vec::new(),
                    }],
                    approval_policy: Some(AskForApproval::OnRequest),
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

                let mut seq = start_seq;

                // M3: SIMULATE mode — a self-contained mini-flow that exercises
                // the full HITL bridge (emit approval/requested → park →
                // resolve → emit turn/completed) WITHOUT a real model. Used
                // only in tests (`NEXUS_SIMULATE_APPROVAL=1`). Does NOT enter
                // the real notification drain (the mock model emits nothing).
                if simulate_approval {
                    let approval_id = approval_counter.fetch_add(1, Ordering::SeqCst);
                    // M6: SIMULATE 命令可配（default rm -rf /tmp/nexus-sim）。
                    // 设为 prompt 类命令（如 npm install nexus-sim）可演示
                    // 策略自学习（3 次 deny → 学习 deny 规则）。
                    let sim_command = std::env::var("NEXUS_SIMULATE_COMMAND")
                        .unwrap_or_else(|_| "rm -rf /tmp/nexus-sim".into());
                    seq += 1;
                    let _ = event_tx.send(TurnEvent {
                        thread_id,
                        turn_db_id,
                        seq,
                        item_type: "approval/requested".into(),
                        codex_item_id: Some(format!("sim-item-{approval_id}")),
                        content_ref: Some(sim_command.clone()),
                        raw_json: serde_json::json!({
                            "command": sim_command,
                            "cwd": "/tmp",
                            "kind": "command_execution",
                        }),
                        usage: None,
                        codex_thread_id: None,
                        is_turn_completed: false,
                        approval: Some(ApprovalInfo {
                            approval_id,
                            kind: ApprovalKind::CommandExecution,
                            command: Some(sim_command.clone()),
                            cwd: Some("/tmp".into()),
                            reason: None,
                            raw_params: serde_json::json!({
                                "command": sim_command,
                                "simulated": true,
                            }),
                        }),
                    });
                    // Park until resolve / interrupt.
                    match park_for_decision(&cmd_rx, approval_id) {
                        Some(decision) => {
                            tracing::info!(?decision, "simulated approval resolved");
                            // M4: 注入合成 tokenUsage（input=10/output=20/
                            // model=nexus-gateway-mock），使 usage_records 落库
                            // + cost 推导可端到端验证，无需真实模型。
                            let sim_usage = Usage {
                                input_tokens: 10,
                                output_tokens: 20,
                                cost_micros: 0, // http_server 落库前调 metering::compute_cost 重算
                                model: Some("nexus-gateway-mock".into()),
                            };
                            // Emit a synthetic item + turn/completed so the
                            // async side finalizes the turn.
                            seq += 1;
                            let _ = event_tx.send(TurnEvent {
                                thread_id,
                                turn_db_id,
                                seq,
                                item_type: "item/completed".into(),
                                codex_item_id: Some(format!("sim-item-{approval_id}")),
                                content_ref: Some(format!("approved: {decision:?}")),
                                raw_json: serde_json::json!({"simulated": true}),
                                usage: Some(sim_usage.clone()),
                                codex_thread_id: None,
                                is_turn_completed: false,
                                approval: None,
                            });
                            seq += 1;
                            let _ = event_tx.send(TurnEvent {
                                thread_id,
                                turn_db_id,
                                seq,
                                item_type: "turn/completed".into(),
                                codex_item_id: None,
                                content_ref: None,
                                raw_json: Value::Null,
                                usage: Some(sim_usage),
                                codex_thread_id: None,
                                is_turn_completed: true,
                                approval: None,
                            });
                        }
                        None => {
                            // Interrupted / shutdown.
                            let _ = event_tx.send(TurnEvent {
                                thread_id,
                                turn_db_id,
                                seq: seq + 1,
                                item_type: "approval/interrupted".into(),
                                codex_item_id: None,
                                content_ref: None,
                                raw_json: Value::Null,
                                usage: None,
                                codex_thread_id: None,
                                is_turn_completed: true,
                                approval: None,
                            });
                        }
                    }
                    continue; // turn done; next command
                }

                // Real drain: notifications until turn/completed, surfacing approvals.
                let mut parked: Option<ParkedApproval> = None;
                'turn_drain: loop {
                    // If we are parked on an approval, wait for resolve/interrupt
                    // instead of reading more from the stream.
                    if let Some(pa) = parked.take() {
                        match park_real_approval(
                            &cmd_rx,
                            &event_tx,
                            thread_id,
                            turn_db_id,
                            pa,
                            p,
                        ) {
                            ParkOutcome::Resolved => {
                                // response written; resume draining.
                                continue 'turn_drain;
                            }
                            ParkOutcome::Interrupted => break 'turn_drain,
                            ParkOutcome::Shutdown => {
                                if let Some(mut dead) = proc.take() {
                                    dead.kill();
                                }
                                return;
                            }
                        }
                    }

                    let ev = match p.next_event() {
                        Ok(StreamEvent::Notification(n)) => n,
                        Ok(StreamEvent::ApprovalRequest(ar)) => {
                            let approval_id = approval_counter.fetch_add(1, Ordering::SeqCst);
                            seq += 1;
                            let _ = event_tx.send(TurnEvent {
                                thread_id,
                                turn_db_id,
                                seq,
                                item_type: "approval/requested".into(),
                                codex_item_id: Some(ar.item_id.clone()),
                                content_ref: ar.command.clone(),
                                raw_json: serde_json::to_value(&ar.raw_params)
                                    .unwrap_or(Value::Null),
                                usage: None,
                                codex_thread_id: None,
                                is_turn_completed: false,
                                approval: Some(ApprovalInfo {
                                    approval_id,
                                    kind: ar.kind.clone(),
                                    command: ar.command.clone(),
                                    cwd: ar.cwd.clone(),
                                    reason: ar.reason.clone(),
                                    raw_params: ar.raw_params.clone(),
                                }),
                            });
                            // Park: remember the jsonrpc_id + kind to write back.
                            parked = Some(ParkedApproval {
                                approval_id,
                                jsonrpc_id: ar.jsonrpc_id,
                                kind: ar.kind,
                            });
                            continue 'turn_drain;
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "runtime: stream ended");
                            emit_error(&event_tx, thread_id, turn_db_id, seq, &e);
                            break 'turn_drain;
                        }
                    };

                    let method = ev.method.clone();
                    let raw = serde_json::to_value(&ev).unwrap_or(Value::Null);
                    seq += 1;

                    let (codex_item_id, content_ref, usage, is_completed) =
                        match ServerNotification::try_from(ev) {
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
                        approval: None,
                    });

                    if is_completed {
                        break 'turn_drain;
                    }
                }
            }
        }
    }
    if let Some(mut p) = proc.take() {
        p.kill();
    }
}

/// A parked (in-flight) approval awaiting a human decision.
struct ParkedApproval {
    approval_id: i64,
    jsonrpc_id: codex_app_server_protocol::RequestId,
    kind: ApprovalKind,
}

/// Outcome of parking on a real (non-simulated) approval.
enum ParkOutcome {
    /// Human decided; response written back to app-server.
    Resolved,
    /// Interrupt received; turn should abort.
    Interrupted,
    /// Shutdown received; driver should exit.
    Shutdown,
}

/// Park the driver on the command channel for a REAL approval request, writing
/// the JSON-RPC response back when a `ResolveApproval` (matching
/// `approval_id`) or `Interrupt` arrives.
fn park_real_approval(
    cmd_rx: &Receiver<DriverCommand>,
    event_tx: &UnboundedSender<TurnEvent>,
    thread_id: Uuid,
    turn_db_id: i64,
    pa: ParkedApproval,
    p: &mut AppServerProcess,
) -> ParkOutcome {
    loop {
        match cmd_rx.recv() {
            Ok(DriverCommand::ResolveApproval { approval_id, decision })
                if approval_id == pa.approval_id =>
            {
                match p.respond_approval(pa.jsonrpc_id.clone(), pa.kind.clone(), decision) {
                    Ok(()) => return ParkOutcome::Resolved,
                    Err(e) => {
                        tracing::error!(error = %e, "respond_approval failed");
                        emit_error(event_tx, thread_id, turn_db_id, 0, &e);
                        return ParkOutcome::Interrupted;
                    }
                }
            }
            Ok(DriverCommand::ResolveApproval { approval_id, .. }) => {
                tracing::warn!(approval_id, "stale ResolveApproval (not parked) — ignore");
                continue;
            }
            Ok(DriverCommand::Interrupt) => {
                // Best-effort Cancel response so the server tears down cleanly.
                let _ = p.respond_approval(
                    pa.jsonrpc_id.clone(),
                    pa.kind.clone(),
                    DecisionInput::Cancel,
                );
                let _ = event_tx.send(TurnEvent {
                    thread_id,
                    turn_db_id,
                    seq: 0,
                    item_type: "approval/interrupted".into(),
                    codex_item_id: None,
                    content_ref: None,
                    raw_json: Value::Null,
                    usage: None,
                    codex_thread_id: None,
                    is_turn_completed: true,
                    approval: None,
                });
                return ParkOutcome::Interrupted;
            }
            Ok(DriverCommand::Shutdown) => {
                let _ = p.respond_approval(
                    pa.jsonrpc_id.clone(),
                    pa.kind.clone(),
                    DecisionInput::Cancel,
                );
                return ParkOutcome::Shutdown;
            }
            Ok(other) => {
                tracing::warn!(?other, "unexpected command while parked — queue ignored");
                continue;
            }
            Err(_) => return ParkOutcome::Shutdown,
        }
    }
}

/// Park for a SIMULATED approval (no real jsonrpc_id to write back). Returns
/// `Some(decision)` if resolved, `None` if interrupted/shutdown.
fn park_for_decision(cmd_rx: &Receiver<DriverCommand>, approval_id: i64) -> Option<DecisionInput> {
    loop {
        match cmd_rx.recv() {
            Ok(DriverCommand::ResolveApproval { approval_id: aid, decision })
                if aid == approval_id =>
            {
                return Some(decision);
            }
            Ok(DriverCommand::Interrupt) => return None,
            Ok(DriverCommand::Shutdown) => return None,
            Ok(_) => continue,
            Err(_) => return None,
        }
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

/// Extract the `id` field from a `ThreadItem`.
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
        is_turn_completed: true,
        approval: None,
    });
}
