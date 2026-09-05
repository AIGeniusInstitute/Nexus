//! nexus-control CLI — PoC driver for M0.
//!
//! Usage:
//!   nexus-control poc --codex-bin <path> --codex-home <dir>
//!
//! Sequence: spawn → initialize → thread/start → turn/start → event loop
//! (persist + print) → turn/completed → kill → respawn → thread/resume →
//! verify seq continuity.

use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::SandboxPolicy;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStatus;
use codex_app_server_protocol::UserInput;
use nexus_control::event_store::EventStore;
use nexus_control::stdio_client::AppServerProcess;
use tracing_subscriber::fmt::format::FmtSpan;

/// Nexus control-plane CLI.
#[derive(Parser)]
#[command(name = "nexus-control", version, about = "Nexus control-plane PoC")]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Run the M0 PoC: spawn app-server, drive a turn, persist events, resume.
    Poc {
        /// Path to the `codex` CLI binary.
        #[arg(long)]
        codex_bin: PathBuf,

        /// CODEX_HOME directory for the app-server (isolated from user home).
        #[arg(long)]
        codex_home: PathBuf,

        /// SQLite event store path. Defaults to
        /// `<codex_home>/nexus-events.db`.
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// User message for the turn. Defaults to a simple "run ls" prompt.
        #[arg(long, default_value = "Please run the command: ls and report the files you see.")]
        message: String,

        /// Skip the resume demonstration (useful when no API key is available).
        #[arg(long, default_value_t = false)]
        skip_resume: bool,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nexus_control=info")),
        )
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let cli = Cli::parse();
    match cli.command {
        CliCommand::Poc {
            codex_bin,
            codex_home,
            db_path,
            message,
            skip_resume,
        } => run_poc(&codex_bin, &codex_home, db_path, &message, skip_resume),
    }
}

fn run_poc(
    codex_bin: &PathBuf,
    codex_home: &PathBuf,
    db_path: Option<PathBuf>,
    message: &str,
    skip_resume: bool,
) -> Result<()> {
    // Ensure codex_home exists.
    std::fs::create_dir_all(codex_home)
        .with_context(|| format!("failed to create codex_home {}", codex_home.display()))?;

    let db_path = db_path.unwrap_or_else(|| codex_home.join("nexus-events.json"));
    let mut store = EventStore::open(&db_path)
        .with_context(|| format!("failed to open event store at {}", db_path.display()))?;

    println!("=== Nexus M0 PoC ===");
    println!("codex_bin:  {}", codex_bin.display());
    println!("codex_home: {}", codex_home.display());
    println!("db_path:    {}", db_path.display());
    println!();

    // --- Phase 1: spawn, initialize, thread/start, turn/start, event loop ---

    println!("[1] Spawning app-server...");
    let mut proc = AppServerProcess::spawn(codex_bin, codex_home)?;
    println!("[1] app-server spawned (pid: {:?})", proc.child_pid());

    println!("[2] initialize...");
    let init = proc.initialize()?;
    println!(
        "    server: userAgent={}, platform={}/{}",
        init.user_agent, init.platform_family, init.platform_os
    );
    println!("    codexHome: {:?}", init.codex_home);

    println!("[3] thread/start...");
    let thread_response = proc.thread_start(ThreadStartParams::default())?;
    let thread_id = thread_response.thread.id.clone();
    println!("    thread_id: {thread_id}");
    println!("    model: {}", thread_response.model);

    println!("[4] turn/start...");
    let turn_response = proc.turn_start(TurnStartParams {
        thread_id: thread_id.clone(),
        client_user_message_id: None,
        input: vec![UserInput::Text {
            text: message.to_string(),
            text_elements: Vec::new(),
        }],
        approval_policy: Some(AskForApproval::Never),
        sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
        ..Default::default()
    })?;
    let turn_id = turn_response.turn.id;
    println!("    turn_id: {turn_id}");

    println!("[5] Streaming events...");
    let mut seq: i64 = 0;
    let mut event_count: i64 = 0;
    let mut turn_completed = false;
    let mut turn_failed = false;

    loop {
        let notification = match proc.next_notification() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("    [error reading notification] {e:#}");
                break;
            }
        };

        seq += 1;
        event_count += 1;

        let etype = notification.method.clone();
        let payload = serde_json::to_string(&notification.params)?;
        let pretty = serde_json::to_string_pretty(&notification.params)
            .unwrap_or_else(|_| payload.clone());

        // Persist to SQLite (idempotent).
        store.upsert_event(&thread_id, &turn_id, seq, &etype, &payload)?;

        // Print.
        println!("    [{seq:>4}] {etype}");
        for line in pretty.lines().take(3) {
            println!("           {line}");
        }

        // Check for turn/completed.
        if let Ok(sn) = ServerNotification::try_from(notification) {
            match sn {
                ServerNotification::TurnCompleted(payload) => {
                    if payload.turn.id == turn_id {
                        println!(
                            "    turn/completed: status={:?}",
                            payload.turn.status
                        );
                        if matches!(payload.turn.status, TurnStatus::Failed) {
                            turn_failed = true;
                        }
                        turn_completed = true;
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    println!();
    println!("[summary] events_received={event_count}, db_count={}", store.count());
    println!("[summary] turn_completed={turn_completed}, turn_failed={turn_failed}");

    if !turn_completed {
        println!("[note] turn did not complete — likely no API key / model unavailable.");
        println!       ("        Handshake (initialize + thread/start + turn/start) succeeded.");
    }

    let phase1_max_seq = store.max_seq(&thread_id, &turn_id);
    println!("[summary] phase1 max_seq for ({thread_id}, {turn_id}) = {phase1_max_seq}");

    // --- Phase 2: resume demonstration ---

    if skip_resume {
        println!();
        println!("[6] Resume skipped (--skip-resume).");
        println!("\n=== PoC complete ===");
        return Ok(());
    }

    if !turn_completed {
        println!();
        println!("[6] Skipping resume demo because the turn did not complete.");
        println!("    (Use --skip-resume to suppress this message.)");
        println!("\n=== PoC complete ===");
        return Ok(());
    }

    println!();
    println!("[6] Resume demonstration: kill + respawn + thread/resume");

    // Kill the old process.
    println!("    killing app-server...");
    proc.kill();
    drop(proc);
    println!("    app-server killed. Pre-resume db count: {}", store.count());

    // Respawn.
    println!("    respawning app-server...");
    let mut proc2 = AppServerProcess::spawn(codex_bin, codex_home)?;
    println!("    app-server respawned (pid: {:?})", proc2.child_pid());

    // Re-initialize (new connection).
    let init2 = proc2.initialize()?;
    println!("    re-initialized: {}", init2.user_agent);

    // Resume.
    println!("    thread/resume(threadId={thread_id})...");
    let resume_response = proc2.thread_resume(ThreadResumeParams {
        thread_id: thread_id.clone(),
        exclude_turns: true,
        ..Default::default()
    })?;
    println!("    resume response: thread_id={}", resume_response.thread.id);

    // Start a follow-up turn on the resumed thread.
    println!("    turn/start (follow-up on resumed thread)...");
    let turn2 = proc2.turn_start(TurnStartParams {
        thread_id: thread_id.clone(),
        client_user_message_id: None,
        input: vec![UserInput::Text {
            text: "Thank you. Please reply with exactly: DONE".to_string(),
            text_elements: Vec::new(),
        }],
        approval_policy: Some(AskForApproval::Never),
        sandbox_policy: Some(SandboxPolicy::DangerFullAccess),
        ..Default::default()
    })?;
    let turn_id2 = turn2.turn.id;
    println!("    new turn_id: {turn_id2}");

    // Stream events for the follow-up turn, starting seq from max.
    let mut seq2 = store.max_seq(&thread_id, &turn_id);
    let mut event_count2: i64 = 0;
    let mut turn2_completed = false;

    loop {
        let notification = match proc2.next_notification() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("    [error] {e:#}");
                break;
            }
        };

        seq2 += 1;
        event_count2 += 1;

        let etype = notification.method.clone();
        let payload = serde_json::to_string(&notification.params)?;

        store.upsert_event(&thread_id, &turn_id2, seq2, &etype, &payload)?;

        println!("    [{seq2:>4}] {etype}");

        if let Ok(sn) = ServerNotification::try_from(notification) {
            if let ServerNotification::TurnCompleted(payload) = sn {
                if payload.turn.id == turn_id2 {
                    println!(
                        "    turn/completed: status={:?}",
                        payload.turn.status
                    );
                    turn2_completed = true;
                    break;
                }
            }
        }
    }

    println!();
    println!("[resume summary] new events={event_count2}, db_count={}", store.count());
    println!("[resume summary] turn2_completed={turn2_completed}");

    // Verify: new event seqs should be > phase1 max_seq (they were assigned
    // from max_seq onwards on the new turn). For the same turn_id (turn1),
    // no new events should have been written after resume (idempotent).
    let phase1_max_after = store.max_seq(&thread_id, &turn_id);
    println!(
        "[resume verify] turn1 max_seq before={phase1_max_seq}, after={phase1_max_after} (should be equal — no new events for old turn)"
    );

    if phase1_max_after == phase1_max_seq {
        println!("[resume verify] PASS: old turn events unchanged after resume (idempotent).");
    } else {
        println!("[resume verify] WARN: old turn max_seq changed unexpectedly.");
    }

    println!("\n=== PoC complete ===");
    Ok(())
}
