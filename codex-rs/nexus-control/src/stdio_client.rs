//! Stdio JSON-RPC client for the Codex app-server.
//!
//! Spawns `codex app-server` as a child process and communicates over
//! newline-delimited JSON (JSONL). The implementation follows the proven pattern
//! from `codex-rs/app-server-test-client` (synchronous std I/O, request/response
//! matching by id, notification buffering during request wait).

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
use std::time::SystemTime;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::ExecPolicyAmendment;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::InitializeResponse;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const GRACEFUL_SHUTDOWN_POLL: Duration = Duration::from_millis(100);

/// Which kind of approval the app-server is asking about (M3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalKind {
    CommandExecution,
    FileChange,
}

/// A simplified, Nexus-side approval decision (M3). Mapped to the appropriate
/// protocol enum (`CommandExecutionApprovalDecision` / `FileChangeApprovalDecision`)
/// when writing the JSON-RPC response back to the app-server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionInput {
    Approve,
    /// M7: approve + apply an execpolicy amendment (allow this command prefix
    /// without prompting in future). `command` is the argv prefix to allow.
    ApproveWithAmendment { command: Vec<String> },
    Deny,
    Cancel,
}

/// A surfaced app-server approval request (M3). The driver parks on this until
/// a `ResolveApproval` command arrives, then writes the JSON-RPC response with
/// the matching `jsonrpc_id`.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// Original JSON-RPC request id (used to match the response).
    pub jsonrpc_id: RequestId,
    pub kind: ApprovalKind,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub reason: Option<String>,
    /// M7: app-server-proposed execpolicy amendment (argv prefix to allow
    /// without prompting), if the approval request carried one.
    pub proposed_amendment: Option<Vec<String>>,
    /// Full params JSON (for audit / display, with secrets stripped upstream).
    pub raw_params: Value,
}

/// What the driver reads from the app-server stream (M3): either a notification
/// to forward, or an approval request that must be surfaced (not auto-accepted).
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Notification(JSONRPCNotification),
    ApprovalRequest(ApprovalRequest),
}

/// A stdio connection to a `codex app-server` child process.
pub struct AppServerProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    pending_notifications: VecDeque<JSONRPCNotification>,
}

impl AppServerProcess {
    /// Spawn `codex app-server` as a child process with the given CODEX_HOME.
    ///
    /// The `codex_bin` should point to the `codex` CLI binary. The child is
    /// launched with `app-server` as the subcommand (stdio is the default
    /// transport — no `--listen` flag needed).
    pub fn spawn(codex_bin: &Path, codex_home: &Path) -> Result<Self> {
        Self::spawn_with_config(codex_bin, codex_home, &[])
    }

    /// Spawn with additional `--config key=value` overrides (same pattern as
    /// `codex-rs/app-server-test-client`). Each entry is passed as
    /// `--config <kv>` before the `app-server` subcommand.
    pub fn spawn_with_config(
        codex_bin: &Path,
        codex_home: &Path,
        config_overrides: &[String],
    ) -> Result<Self> {
        let mut cmd = Command::new(codex_bin);

        // Ensure the codex binary's parent dir is on PATH so it can find
        // subcommands and helpers.
        if let Some(parent) = codex_bin.parent() {
            let mut path = OsString::from(parent.as_os_str());
            if let Some(existing) = std::env::var_os("PATH") {
                path.push(":");
                path.push(existing);
            }
            cmd.env("PATH", path);
        }
        // Set CODEX_HOME so the server uses our isolated directory.
        cmd.env("CODEX_HOME", codex_home);

        // Inject --config overrides before the subcommand (same order as
        // app-server-test-client).
        for override_kv in config_overrides {
            cmd.arg("--config").arg(override_kv);
        }

        let mut child = cmd
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| {
                format!(
                    "failed to start `{}` app-server",
                    codex_bin.display()
                )
            })?;

        let stdin = child.stdin.take().context("app-server stdin unavailable")?;
        let stdout = child.stdout.take().context("app-server stdout unavailable")?;

        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            pending_notifications: VecDeque::new(),
        })
    }

    /// Send the `initialize` request and `initialized` notification.
    pub fn initialize(&mut self) -> Result<InitializeResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::Initialize {
            request_id: request_id.clone(),
            params: InitializeParams {
                client_info: ClientInfo {
                    name: "nexus-control".to_string(),
                    title: Some("Nexus Control".to_string()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                capabilities: Some(InitializeCapabilities {
                    experimental_api: true,
                    request_attestation: false,
                    opt_out_notification_methods: None,
                    mcp_server_openai_form_elicitation: false,
                    extensions: None,
                }),
            },
        };

        let response: InitializeResponse = self.send_request(request, request_id, "initialize")?;

        // Complete the handshake.
        let initialized = JSONRPCMessage::Notification(JSONRPCNotification {
            method: "initialized".to_string(),
            params: None,
        });
        self.write_jsonrpc_message(initialized)?;

        Ok(response)
    }

    /// Start a new thread.
    pub fn thread_start(&mut self, params: ThreadStartParams) -> Result<ThreadStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadStart {
            request_id: request_id.clone(),
            params,
        };
        self.send_request(request, request_id, "thread/start")
    }

    /// Resume an existing thread by id.
    pub fn thread_resume(&mut self, params: ThreadResumeParams) -> Result<ThreadResumeResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadResume {
            request_id: request_id.clone(),
            params,
        };
        self.send_request(request, request_id, "thread/resume")
    }

    /// Start a turn.
    pub fn turn_start(&mut self, params: TurnStartParams) -> Result<TurnStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::TurnStart {
            request_id: request_id.clone(),
            params,
        };
        self.send_request(request, request_id, "turn/start")
    }

    /// Read the next notification from the server, blocking until one arrives.
    /// Server requests encountered along the way are auto-accepted.
    pub fn next_notification(&mut self) -> Result<JSONRPCNotification> {
        if let Some(n) = self.pending_notifications.pop_front() {
            return Ok(n);
        }

        loop {
            let message = self.read_jsonrpc_message()?;
            match message {
                JSONRPCMessage::Notification(n) => return Ok(n),
                JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => {
                    // No outstanding requests; ignore stray responses.
                    continue;
                }
                JSONRPCMessage::Request(req) => {
                    self.handle_server_request(req)?;
                }
            }
        }
    }

    /// Try to read the next notification as a typed `ServerNotification`.
    pub fn next_server_notification(&mut self) -> Result<Option<ServerNotification>> {
        let notification = self.next_notification()?;
        Ok(ServerNotification::try_from(notification).ok())
    }

    /// M3: read the next stream event. Like `next_notification`, but when the
    /// server sends an approval request (`CommandExecutionRequestApproval` /
    /// `FileChangeRequestApproval`), it is **surfaced** as
    /// `StreamEvent::ApprovalRequest` instead of being auto-accepted. Other
    /// server requests are still auto-handled (M2 behavior).
    pub fn next_event(&mut self) -> Result<StreamEvent> {
        if let Some(n) = self.pending_notifications.pop_front() {
            return Ok(StreamEvent::Notification(n));
        }
        loop {
            let message = self.read_jsonrpc_message()?;
            match message {
                JSONRPCMessage::Notification(n) => return Ok(StreamEvent::Notification(n)),
                JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => continue,
                JSONRPCMessage::Request(req) => {
                    // Classify without consuming `req` on the non-approval path
                    // (handle_server_request needs the original JSONRPCRequest).
                    match ServerRequest::try_from(req.clone()) {
                        Ok(ServerRequest::CommandExecutionRequestApproval { request_id, params }) => {
                            // Capture raw_params BEFORE moving fields out of `params`.
                            let raw_params =
                                serde_json::to_value(&params).unwrap_or(Value::Null);
                            // M7: extract app-server-proposed execpolicy amendment.
                            let proposed_amendment = params
                                .proposed_execpolicy_amendment
                                .as_ref()
                                .map(|a| a.command.clone());
                            return Ok(StreamEvent::ApprovalRequest(ApprovalRequest {
                                jsonrpc_id: request_id,
                                kind: ApprovalKind::CommandExecution,
                                thread_id: params.thread_id,
                                turn_id: params.turn_id,
                                item_id: params.item_id,
                                command: params.command,
                                cwd: params.cwd.map(|p| p.to_string()),
                                reason: params.reason,
                                proposed_amendment,
                                raw_params,
                            }));
                        }
                        Ok(ServerRequest::FileChangeRequestApproval { request_id, params }) => {
                            let raw_params =
                                serde_json::to_value(&params).unwrap_or(Value::Null);
                            return Ok(StreamEvent::ApprovalRequest(ApprovalRequest {
                                jsonrpc_id: request_id,
                                kind: ApprovalKind::FileChange,
                                thread_id: params.thread_id,
                                turn_id: params.turn_id,
                                item_id: params.item_id,
                                command: None,
                                cwd: params.grant_root
                                    .map(|p| p.to_string_lossy().into_owned()),
                                reason: params.reason,
                                proposed_amendment: None,
                                raw_params,
                            }));
                        }
                        // Non-approval server request: auto-handle (M2 path).
                        Ok(_) | Err(_) => {
                            self.handle_server_request(req)?;
                        }
                    }
                }
            }
        }
    }

    /// M3: write back the JSON-RPC response for a surfaced approval request.
    /// `jsonrpc_id` must be the original request id; `decision` is the
    /// Nexus-side decision mapped to the appropriate protocol enum.
    pub fn respond_approval(
        &mut self,
        jsonrpc_id: RequestId,
        kind: ApprovalKind,
        decision: DecisionInput,
    ) -> Result<()> {
        let result: Value = match kind {
            ApprovalKind::CommandExecution => {
                let d = match decision {
                    DecisionInput::Approve => CommandExecutionApprovalDecision::Accept,
                    // M7: approve + apply proposed execpolicy amendment.
                    DecisionInput::ApproveWithAmendment { command } => {
                        CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment {
                            execpolicy_amendment: ExecPolicyAmendment { command },
                        }
                    }
                    DecisionInput::Deny => CommandExecutionApprovalDecision::Decline,
                    DecisionInput::Cancel => CommandExecutionApprovalDecision::Cancel,
                };
                serde_json::to_value(CommandExecutionRequestApprovalResponse { decision: d })?
            }
            ApprovalKind::FileChange => {
                // File-change approvals have no execpolicy amendment; an
                // amendment decision is treated as a plain accept.
                let d = match decision {
                    DecisionInput::Approve | DecisionInput::ApproveWithAmendment { .. } => {
                        FileChangeApprovalDecision::Accept
                    }
                    DecisionInput::Deny => FileChangeApprovalDecision::Decline,
                    DecisionInput::Cancel => FileChangeApprovalDecision::Cancel,
                };
                serde_json::to_value(FileChangeRequestApprovalResponse { decision: d })?
            }
        };
        let message = JSONRPCMessage::Response(JSONRPCResponse {
            id: jsonrpc_id,
            result,
        });
        self.write_jsonrpc_message(message)
    }

    /// Kill the child process (non-graceful).
    pub fn kill(&mut self) {
        self.stdin.take(); // drop stdin to signal EOF
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Best-effort child PID.
    pub fn child_pid(&self) -> Option<u32> {
        Some(self.child.id())
    }

    /// Is the child process still running? (false if it has exited.)
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    // -- internals --

    fn request_id(&self) -> RequestId {
        RequestId::String(Uuid::new_v4().to_string())
    }

    fn send_request<T>(
        &mut self,
        request: ClientRequest,
        request_id: RequestId,
        method: &str,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.write_request(&request)?;
        self.wait_for_response(request_id, method)
    }

    fn write_request(&mut self, request: &ClientRequest) -> Result<()> {
        let request_value = serde_json::to_value(request)?;
        let rpc_request: JSONRPCRequest = serde_json::from_value(request_value)
            .context("client request was not a valid JSON-RPC request")?;
        let json = serde_json::to_string(&rpc_request)?;
        self.write_payload(&json)
    }

    fn wait_for_response<T>(&mut self, request_id: RequestId, method: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        loop {
            let message = self.read_jsonrpc_message()?;
            match message {
                JSONRPCMessage::Response(JSONRPCResponse { id, result }) => {
                    if id == request_id {
                        return serde_json::from_value(result)
                            .with_context(|| format!("{method} response deserialization failed"));
                    }
                }
                JSONRPCMessage::Error(err) => {
                    if err.id == request_id {
                        bail!("{method} failed: {err:?}");
                    }
                }
                JSONRPCMessage::Notification(notification) => {
                    self.pending_notifications.push_back(notification);
                }
                JSONRPCMessage::Request(request) => {
                    self.handle_server_request(request)?;
                }
            }
        }
    }

    fn handle_server_request(&mut self, request: JSONRPCRequest) -> Result<()> {
        let request_id = request.id.clone();
        let server_request = match ServerRequest::try_from(request) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("[nexus-control] skipping unparseable server request: {e}");
                return Ok(());
            }
        };

        let result_value: Value = match &server_request {
            ServerRequest::CommandExecutionRequestApproval { .. } => {
                serde_json::to_value(&CommandExecutionRequestApprovalResponse {
                    decision: CommandExecutionApprovalDecision::Accept,
                })?
            }
            ServerRequest::FileChangeRequestApproval { .. } => {
                serde_json::to_value(&FileChangeRequestApprovalResponse {
                    decision: FileChangeApprovalDecision::Accept,
                })?
            }
            other => {
                eprintln!("[nexus-control] unhandled server request: {other:?}");
                return Ok(());
            }
        };

        let message = JSONRPCMessage::Response(JSONRPCResponse {
            id: request_id,
            result: result_value,
        });
        self.write_jsonrpc_message(message)?;
        Ok(())
    }

    fn write_jsonrpc_message(&mut self, message: JSONRPCMessage) -> Result<()> {
        let payload = serde_json::to_string(&message)?;
        self.write_payload(&payload)
    }

    fn write_payload(&mut self, payload: &str) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("app-server stdin closed")?;
        writeln!(stdin, "{payload}")?;
        stdin.flush().context("failed to flush to app-server")?;
        Ok(())
    }

    fn read_jsonrpc_message(&mut self) -> Result<JSONRPCMessage> {
        loop {
            let raw = self.read_payload()?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parsed: Value = serde_json::from_str(trimmed)
                .context("response was not valid JSON")?;
            let message: JSONRPCMessage = serde_json::from_value(parsed)
                .context("response was not a valid JSON-RPC message")?;
            return Ok(message);
        }
    }

    fn read_payload(&mut self) -> Result<String> {
        let mut line = String::new();
        let bytes = self
            .stdout
            .read_line(&mut line)
            .context("failed to read from app-server")?;
        if bytes == 0 {
            bail!("codex app-server closed stdout");
        }
        Ok(line)
    }
}

impl Drop for AppServerProcess {
    fn drop(&mut self) {
        self.stdin.take();

        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }

        // Try graceful shutdown for a few seconds, then kill.
        let deadline = SystemTime::now() + GRACEFUL_SHUTDOWN_TIMEOUT;
        loop {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            if SystemTime::now() >= deadline {
                break;
            }
            std::thread::sleep(GRACEFUL_SHUTDOWN_POLL);
        }

        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
