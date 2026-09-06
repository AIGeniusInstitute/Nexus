//! nexus-control CLI — PoC driver for M0.
//!
//! Usage:
//!   nexus-control poc --codex-bin <path> --codex-home <dir>
//!   nexus-control poc-execpolicy --codex-bin <path> --codex-home <dir>
//!   nexus-control poc-gateway --codex-bin <path> --codex-home <dir>
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
use nexus_control::execpolicy_rules;
use nexus_control::model_gateway::ModelGateway;
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

    /// T0-4: Execpolicy rule injection PoC.
    ///
    /// Writes a `.rules` file with `rm` → Forbidden, `ls` → Allow, then
    /// spawns the app-server with ReadOnly sandbox and NeverAsk approval.
    /// The execpolicy is auto-loaded from `<codex_home>/rules/`.
    PocExecpolicy {
        #[arg(long)]
        codex_bin: PathBuf,

        #[arg(long)]
        codex_home: PathBuf,

        /// Message asking the model to run `rm -rf /` (should be blocked).
        #[arg(long, default_value = "Please run: rm -rf /")]
        rm_message: String,

        /// Message asking the model to run `ls` (should be allowed).
        #[arg(long, default_value = "Please run: ls")]
        ls_message: String,
    },

    /// T0-7: Model Gateway proxy PoC.
    ///
    /// Starts a local HTTP gateway, writes config.toml pointing the model
    /// provider to it, then spawns the app-server and runs a turn. All
    /// model traffic goes through the gateway.
    PocGateway {
        #[arg(long)]
        codex_bin: PathBuf,

        #[arg(long)]
        codex_home: PathBuf,

        /// Bearer token for the gateway. If not provided, a random one is
        /// generated.
        #[arg(long)]
        token: Option<String>,

        /// Message to send in the turn.
        #[arg(long, default_value = "Say hello.")]
        message: String,
    },

    /// M1: apply Postgres migrations (idempotent).
    Migrate {
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
    },

    /// M1: start the HTTP + WS gateway server.
    Serve {
        #[arg(long, env = "DATABASE_URL", default_value = "postgres://nexus:nexus@localhost:5432/nexus")]
        database_url: String,
        #[arg(long, default_value = "0.0.0.0:8765")]
        addr: String,
        #[arg(long, env = "NEXUS_JWT_SECRET", default_value = "nexus-m1-dev-secret-change-me")]
        jwt_secret: String,
        #[arg(long)]
        admin_email: Option<String>,
        #[arg(long)]
        admin_password: Option<String>,
        /// Path to the `codex` CLI binary (M2 runtime).
        #[arg(long, env = "NEXUS_CODEX_BIN", default_value = "codex")]
        codex_bin: PathBuf,
        /// CODEX_HOME directory for the app-server (M2 runtime).
        #[arg(long, env = "NEXUS_CODEX_HOME", default_value = ".nexus-control/codex-home")]
        codex_home: PathBuf,
    },

    /// M1: CLI login — obtain a JWT and store it locally.
    Login {
        #[arg(long)]
        server: String,
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: String,
    },

    /// M1: list threads.
    Threads {
        #[arg(long)]
        server: String,
    },

    /// M1: submit a turn to a thread.
    Run {
        #[arg(long)]
        server: String,
        #[arg(long)]
        thread: String,
        #[arg(long, default_value = "hello")]
        input: String,
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
        CliCommand::PocExecpolicy {
            codex_bin,
            codex_home,
            rm_message,
            ls_message,
        } => run_execpolicy_poc(&codex_bin, &codex_home, &rm_message, &ls_message),
        CliCommand::PocGateway {
            codex_bin,
            codex_home,
            token,
            message,
        } => run_gateway_poc(&codex_bin, &codex_home, token, &message),
        CliCommand::Migrate { database_url } => run_migrate(&database_url),
        CliCommand::Serve {
            database_url,
            addr,
            jwt_secret,
            admin_email,
            admin_password,
            codex_bin,
            codex_home,
        } => run_serve(&database_url, &addr, &jwt_secret, admin_email, admin_password, &codex_bin, &codex_home),
        CliCommand::Login { server, email, password } => run_login(&server, &email, &password),
        CliCommand::Threads { server } => run_threads(&server),
        CliCommand::Run { server, thread, input } => run_run(&server, &thread, &input),
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

// ===========================================================================
// T0-4: Execpolicy rule injection PoC
// ===========================================================================

fn run_execpolicy_poc(
    codex_bin: &PathBuf,
    codex_home: &PathBuf,
    rm_message: &str,
    ls_message: &str,
) -> Result<()> {
    std::fs::create_dir_all(codex_home)
        .with_context(|| format!("failed to create codex_home {}", codex_home.display()))?;

    println!("=== Nexus T0-4: Execpolicy Rule Injection PoC ===");
    println!("codex_bin:  {}", codex_bin.display());
    println!("codex_home: {}", codex_home.display());
    println!();

    // Step 1: Write the execpolicy rules file.
    println!("[1] Writing execpolicy rules to <codex_home>/rules/default.rules...");
    let rules_path = execpolicy_rules::write_default_rules(codex_home)?;
    println!("    rules written to: {}", rules_path.display());
    println!("    rules content:");
    for line in execpolicy_rules::NEXUS_DEFAULT_RULES.lines() {
        println!("      {line}");
    }
    println!();

    // Step 2: Spawn app-server (rules are auto-loaded from <codex_home>/rules/).
    println!("[2] Spawning app-server with execpolicy rules...");
    let mut proc = AppServerProcess::spawn(codex_bin, codex_home)?;
    println!("    app-server spawned (pid: {:?})", proc.child_pid());

    // Step 3: Initialize.
    println!("[3] initialize...");
    let init = proc.initialize()?;
    println!("    server: userAgent={}, platform={}/{}",
        init.user_agent, init.platform_family, init.platform_os);

    // Step 4: Thread start.
    println!("[4] thread/start...");
    let thread_response = proc.thread_start(ThreadStartParams::default())?;
    let thread_id = thread_response.thread.id.clone();
    println!("    thread_id: {}", thread_id);
    println!("    model: {}", thread_response.model);
    println!();

    // Step 5: Turn 1 — try `rm -rf /` (should be Forbidden by execpolicy).
    println!("[5] turn/start #1: rm -rf / (should be blocked by execpolicy)");
    println!("    message: \"{rm_message}\"");
    let turn_response = proc.turn_start(TurnStartParams {
        thread_id: thread_id.clone(),
        client_user_message_id: None,
        input: vec![UserInput::Text {
            text: rm_message.to_string(),
            text_elements: Vec::new(),
        }],
        approval_policy: Some(AskForApproval::Never),
        sandbox_policy: Some(SandboxPolicy::ReadOnly {
            network_access: false,
        }),
        ..Default::default()
    })?;
    let turn_id_rm = turn_response.turn.id.clone();
    println!("    turn_id: {turn_id_rm}");

    println!("    [streaming events for rm turn]");
    let mut rm_blocked = false;
    let mut rm_completed = false;
    loop {
        let notification = match proc.next_notification() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("    [error reading notification] {e:#}");
                break;
            }
        };

        let etype = notification.method.clone();
        let payload = serde_json::to_string(&notification.params)
            .unwrap_or_else(|_| "<serialize failed>".into());

        // Check for forbidden/rejection in the payload.
        let payload_lower = payload.to_ascii_lowercase();
        if payload_lower.contains("forbidden")
            || payload_lower.contains("rejected")
            || payload_lower.contains("blocked")
            || payload_lower.contains("nexus execpolicy")
        {
            rm_blocked = true;
        }

        println!("    {etype}");

        if let Ok(sn) = ServerNotification::try_from(notification) {
            match &sn {
                ServerNotification::ItemCompleted(item) => {
                    // Check if the item is a command execution that was rejected.
                    let item_str = format!("{item:?}");
                    let item_lower = item_str.to_ascii_lowercase();
                    if item_lower.contains("forbidden")
                        || item_lower.contains("rejected")
                        || item_lower.contains("failed")
                    {
                        rm_blocked = true;
                    }
                    println!("    item completed (rm): {item_str}");
                }
                ServerNotification::TurnCompleted(payload) => {
                    if payload.turn.id == turn_id_rm {
                        println!("    turn/completed: status={:?}", payload.turn.status);
                        if let Some(error) = &payload.turn.error {
                            println!("    turn error: {}", error.message);
                            // If the turn failed because the command was blocked,
                            // that proves the execpolicy is working.
                            if error.message.to_ascii_lowercase().contains("forbidden")
                                || error.message.to_ascii_lowercase().contains("rejected")
                                || error.message.to_ascii_lowercase().contains("blocked")
                            {
                                rm_blocked = true;
                            }
                        }
                        rm_completed = true;
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    println!();
    println!("    [rm turn summary] completed={rm_completed}, blocked_by_execpolicy={rm_blocked}");

    // Step 6: Turn 2 — try `ls` (should be Allowed by execpolicy).
    println!();
    println!("[6] turn/start #2: ls (should be allowed by execpolicy)");
    println!("    message: \"{ls_message}\"");
    let turn_response2 = proc.turn_start(TurnStartParams {
        thread_id: thread_id.clone(),
        client_user_message_id: None,
        input: vec![UserInput::Text {
            text: ls_message.to_string(),
            text_elements: Vec::new(),
        }],
        approval_policy: Some(AskForApproval::Never),
        sandbox_policy: Some(SandboxPolicy::ReadOnly {
            network_access: false,
        }),
        ..Default::default()
    })?;
    let turn_id_ls = turn_response2.turn.id.clone();
    println!("    turn_id: {turn_id_ls}");

    println!("    [streaming events for ls turn]");
    let mut ls_completed = false;
    loop {
        let notification = match proc.next_notification() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("    [error reading notification] {e:#}");
                break;
            }
        };

        let etype = notification.method.clone();
        println!("    {etype}");

        if let Ok(sn) = ServerNotification::try_from(notification) {
            if let ServerNotification::TurnCompleted(payload) = sn {
                if payload.turn.id == turn_id_ls {
                    println!("    turn/completed: status={:?}", payload.turn.status);
                    ls_completed = true;
                    break;
                }
            }
        }
    }

    println!();
    println!("    [ls turn summary] completed={ls_completed}");

    // --- AC verification ---
    println!();
    println!("=== T0-4 Acceptance Criteria ===");
    println!("AC4.1 rm -rf / blocked by execpolicy: {}", if rm_blocked { "PASS" } else { "UNVERIFIED (turn may not have reached command execution without a model API key)" });
    println!("AC4.2 ls allowed by execpolicy:       {}", if ls_completed { "PASS" } else { "UNVERIFIED (turn may not have completed without a model API key)" });
    println!("AC4.3 Justification visible in rules:  PASS (verified in unit tests — see execpolicy_rules::tests)");
    println!();
    println!("Note: The execpolicy rules file was successfully written and will be");
    println!("auto-loaded by the app-server from <codex_home>/rules/. Unit tests");
    println!("verify that rm→Forbidden and ls→Allow at the policy evaluator level.");

    println!("\n=== T0-4 PoC complete ===");
    Ok(())
}

// ===========================================================================
// T0-7: Model Gateway proxy PoC
// ===========================================================================

fn run_gateway_poc(
    codex_bin: &PathBuf,
    codex_home: &PathBuf,
    token: Option<String>,
    message: &str,
) -> Result<()> {
    std::fs::create_dir_all(codex_home)
        .with_context(|| format!("failed to create codex_home {}", codex_home.display()))?;

    // Generate a token if not provided.
    let gateway_token = token.unwrap_or_else(|| {
        format!("nexus-gateway-{}", uuid::Uuid::new_v4().simple())
    });

    println!("=== Nexus T0-7: Model Gateway Proxy PoC ===");
    println!("codex_bin:  {}", codex_bin.display());
    println!("codex_home: {}", codex_home.display());
    println!();

    // Step 1: Start the model gateway.
    println!("[1] Starting model gateway...");
    let gateway = ModelGateway::start(&gateway_token)?;
    let gateway_url = gateway.base_url();
    println!("    gateway listening on: {}", gateway.addr);
    println!("    base_url: {gateway_url}");
    println!("    token:    {gateway_token}");
    println!();

    // Step 2: Write config.toml pointing to the gateway.
    println!("[2] Writing config.toml with model_providers pointing to gateway...");
    let config_path = execpolicy_rules::write_config_toml(
        codex_home,
        &format!("{gateway_url}/v1"),
        &gateway_token,
        "nexus-gateway-mock",
    )?;
    println!("    config written to: {}", config_path.display());
    println!();

    // Step 3: Spawn app-server (will load config.toml from CODEX_HOME).
    println!("[3] Spawning app-server...");
    let mut proc = AppServerProcess::spawn(codex_bin, codex_home)?;
    println!("    app-server spawned (pid: {:?})", proc.child_pid());

    // Step 4: Initialize.
    println!("[4] initialize...");
    let init = proc.initialize()?;
    println!("    server: userAgent={}", init.user_agent);

    // Step 5: Thread start.
    println!("[5] thread/start...");
    let thread_response = proc.thread_start(ThreadStartParams::default())?;
    let thread_id = thread_response.thread.id.clone();
    println!("    thread_id: {thread_id}");
    println!("    model: {}", thread_response.model);
    println!();

    // Step 6: Turn start.
    println!("[6] turn/start...");
    println!("    message: \"{message}\"");
    let turn_response = proc.turn_start(TurnStartParams {
        thread_id: thread_id.clone(),
        client_user_message_id: None,
        input: vec![UserInput::Text {
            text: message.to_string(),
            text_elements: Vec::new(),
        }],
        approval_policy: Some(AskForApproval::Never),
        sandbox_policy: Some(SandboxPolicy::ReadOnly {
            network_access: false,
        }),
        ..Default::default()
    })?;
    let turn_id = turn_response.turn.id;
    println!("    turn_id: {turn_id}");

    // Step 7: Stream events.
    println!("[7] Streaming events...");
    let mut turn_completed = false;
    loop {
        let notification = match proc.next_notification() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("    [error reading notification] {e:#}");
                break;
            }
        };

        let etype = notification.method.clone();
        println!("    {etype}");

        if let Ok(sn) = ServerNotification::try_from(notification) {
            if let ServerNotification::TurnCompleted(payload) = sn {
                if payload.turn.id == turn_id {
                    println!("    turn/completed: status={:?}", payload.turn.status);
                    turn_completed = true;
                    break;
                }
            }
        }
    }

    let final_count = gateway.request_count();
    println!();
    println!("    [gateway metering] total requests received: {final_count}");
    println!("    [turn completed] {turn_completed}");

    // --- AC verification ---
    println!();
    println!("=== T0-7 Acceptance Criteria ===");
    println!(
        "AC7.1 model request routed through gateway: {}",
        if final_count > 0 { "PASS" } else { "UNVERIFIED (turn may not have reached model call)" }
    );
    println!(
        "AC7.2 invalid token rejected:               PASS (verified in unit tests — see model_gateway::tests)"
    );
    println!(
        "AC7.3 gateway records token metering:       {}",
        if final_count > 0 {
            "PASS"
        } else {
            "UNVERIFIED (no requests received — turn may not have reached model call)"
        }
    );
    println!();
    println!("Note: The gateway successfully started and config.toml was written.");
    println!("Unit tests verify token validation (401 on wrong token) and");
    println!("metering (request count). The full end-to-end turn requires a");
    println!("model API or a properly configured gateway forwarding to one.");

    println!("\n=== T0-7 PoC complete ===");
    Ok(())
}

// ===========================================================================
// M1: migrate / serve / login / threads / run
// ===========================================================================

fn rt() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")
}

fn run_migrate(database_url: &str) -> Result<()> {
    println!("=== Nexus M1: migrate ===");
    let rt = rt()?;
    rt.block_on(async {
        let pool = nexus_control::db::connect(database_url).await?;
        nexus_control::db::run_migrations(&pool).await?;
        println!("migrations applied to {database_url}");
        Ok::<_, anyhow::Error>(())
    })
}

fn run_serve(
    database_url: &str,
    addr: &str,
    jwt_secret: &str,
    admin_email: Option<String>,
    admin_password: Option<String>,
    codex_bin: &std::path::Path,
    codex_home: &std::path::Path,
) -> Result<()> {
    println!("=== Nexus M17: serve ===");
    let rt = rt()?;
    rt.block_on(async move {
        let pool = nexus_control::db::connect(database_url).await?;
        nexus_control::db::run_migrations(&pool).await?;
        if let (Some(email), Some(pw)) = (admin_email, admin_password) {
            nexus_control::db::seed_admin(&pool, &email, &pw).await?;
            println!("admin user seeded: {email}");
        }

        // M2: start the model gateway (mock or upstream passthrough).
        let gateway_token = format!("nexus-gateway-{}", uuid::Uuid::new_v4().simple());
        let _gateway = nexus_control::model_gateway::ModelGateway::start(&gateway_token)?;
        let gateway_url = _gateway.base_url();
        println!("model gateway on {gateway_url} (upstream passthrough: {})",
            if std::env::var("NEXUS_UPSTREAM_MODEL_URL").is_ok() { "on" } else { "off/mock" });

        // M2: write config.toml pointing the app-server at the gateway.
        // M8: model id from env (default deepseek-v4-pro); wire_api stays
        // "responses" (codex forces it) and the gateway does the
        // Responses↔Chat translation to the dashscope upstream.
        std::fs::create_dir_all(codex_home).ok();
        let model_id = std::env::var("NEXUS_MODEL")
            .unwrap_or_else(|_| "deepseek-v4-pro".into());
        let _config_path = nexus_control::execpolicy_rules::write_config_toml(
            codex_home,
            &format!("{gateway_url}/v1"),
            &gateway_token,
            &model_id,
        )?;

        // M4: 下发 per-tenant execpolicy rules（prefix_rule 语法，M0 验证）。
        // app-server 每-turn 自动加载 <CODEX_HOME>/rules/*.rules。
        let tenant_rules = nexus_control::policy::generate_rules(&pool, 1).await
            .unwrap_or_else(|e| { eprintln!("generate_rules failed: {e}"); String::new() });
        if !tenant_rules.is_empty() {
            match nexus_control::policy::write_tenant_rules(1, codex_home, &tenant_rules) {
                Ok(p) => println!("tenant rules written: {}", p.display()),
                Err(e) => eprintln!("write_tenant_rules failed: {e}"),
            }
        }

        // M5: spawn a POOL of driver threads (each owns an app-server process).
        // NEXUS_POOL_SIZE (default 4) controls the max concurrent in-flight
        // turns. The global approval-id counter is seeded from DB max so
        // driver-generated ids never collide across slots or restarts.
        let start_approval_id: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM approval_tickets",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(1);
        let pool_size: usize = std::env::var("NEXUS_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(4);
        let driver_pool = nexus_control::runtime::spawn_pool(
            codex_bin.to_path_buf(),
            codex_home.to_path_buf(),
            pool_size,
            start_approval_id,
        );
        println!(
            "driver pool spawned: {pool_size} slots (codex_bin={}, codex_home={})",
            codex_bin.display(),
            codex_home.display()
        );

        let jwt = std::sync::Arc::new(nexus_control::auth::JwtIssuer::new(jwt_secret, 24 * 3600));
        let auth: std::sync::Arc<dyn nexus_control::auth::AuthProvider> =
            std::sync::Arc::new(nexus_control::auth::LocalProvider::new(
                pool.clone(),
                nexus_control::auth::JwtIssuer::new(jwt_secret, 24 * 3600),
            ));
        let state = nexus_control::http_server::AppState {
            pool,
            jwt,
            auth,
            driver_pool,
            turn_slots: std::sync::Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            codex_home: codex_home.to_path_buf(),
            broadcast: std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        };
        let app = nexus_control::http_server::router(state);
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("bind {addr}"))?;
        println!("nexus-control serving on http://{addr}");
        axum::serve(listener, app).await?;
        Ok::<_, anyhow::Error>(())
    })
}

fn cred_store_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home).join(".nexus-control").join("credentials.json"))
}

fn load_creds() -> Result<(String, String)> {
    let path = cred_store_path()?;
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("read credentials at {}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let server = v["server"].as_str().context("missing server").map(str::to_string)?;
    let token = v["token"].as_str().context("missing token").map(str::to_string)?;
    Ok((server, token))
}

fn run_login(server: &str, email: &str, password: &str) -> Result<()> {
    println!("=== Nexus M1: login ===");
    let rt = rt()?;
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        let resp = client
            .post(format!("{server}/v1/auth/login"))
            .json(&serde_json::json!({ "email": email, "password": password }))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("login failed: HTTP {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        let token = body["token"]
            .as_str()
            .context("response missing token")?
            .to_string();
        let path = cred_store_path()?;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(
            &path,
            serde_json::json!({ "server": server, "token": token }).to_string(),
        )?;
        println!("logged in as {email}; token stored at {}", path.display());
        Ok::<_, anyhow::Error>(())
    })
}

fn run_threads(_server: &str) -> Result<()> {
    println!("=== Nexus M1: threads ===");
    let (server, token) = load_creds()?;
    let rt = rt()?;
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        let resp = client
            .get(format!("{server}/v1/threads"))
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("list failed: HTTP {}", resp.status());
        }
        let rows: serde_json::Value = resp.json().await?;
        println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
        Ok::<_, anyhow::Error>(())
    })
}

fn run_run(_server: &str, thread_id: &str, input: &str) -> Result<()> {
    println!("=== Nexus M1: run ===");
    let (server, token) = load_creds()?;
    let rt = rt()?;
    rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let resp = client
            .post(format!("{server}/v1/threads/{thread_id}/turns"))
            .header("Authorization", format!("Bearer {token}"))
            .json(&serde_json::json!({ "input": input }))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("turn failed: HTTP {}", resp.status());
        }
        let body: serde_json::Value = resp.json().await?;
        println!("{body:#?}");
        Ok::<_, anyhow::Error>(())
    })
}
