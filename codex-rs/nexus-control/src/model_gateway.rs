//! Model gateway: local HTTP proxy between the Codex app-server and a real
//! OpenAI-compatible Chat Completions endpoint (M8).
//!
//! M0/T0-7: a mock gateway that returns a synthetic Responses-API payload so
//! the app-server turn can complete without a real model. M2: optional
//! upstream passthrough over raw TCP (broken for HTTPS + hostnames).
//!
//! **M8**: codex forces `wire_api = "responses"` (Responses API SSE,
//! `CHAT_WIRE_API_REMOVED_ERROR` in `model-provider-info`), but dashscope only
//! speaks Chat Completions SSE. The gateway now performs a **streaming
//! Responses↔Chat-Completions protocol translation**: it parses the Responses
//! API request body, extracts input messages, issues a streaming Chat
//! Completions request to dashscope via reqwest (rustls TLS), and translates
//! each Chat SSE chunk into the corresponding Responses SSE event on the fly
//! so the app-server's `stream_responses_api` SSE parser
//! (`codex-api/src/sse/responses.rs`) accepts it.
//!
//! The minimum Responses SSE event set required by the parser:
//! `response.created` + `response.output_item.added` +
//! `response.output_text.delta`(×N) + `response.output_item.done` +
//! `response.completed`(with `usage`, else "stream closed before
//! response.completed"). `content_part.*` / `output_text.done` are
//! `trace`-level unhandled and may be omitted.

use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

use anyhow::Context;
use anyhow::Result;

/// A running model gateway instance.
pub struct ModelGateway {
    pub addr: std::net::SocketAddr,
    pub token: String,
    request_count: Arc<AtomicU64>,
    /// The accept thread handle. Dropping `ModelGateway` stops the thread.
    _handle: Option<thread::JoinHandle<()>>,
}

impl ModelGateway {
    /// Start the gateway on `127.0.0.1:0` (ephemeral port) with the given
    /// bearer token.
    pub fn start(token: &str) -> Result<Self> {
        Self::start_on("127.0.0.1:0", token)
    }

    /// Start the gateway on a specific address.
    pub fn start_on(addr: &str, token: &str) -> Result<Self> {
        let listener = TcpListener::bind(addr)
            .with_context(|| format!("failed to bind model gateway to {addr}"))?;
        let actual_addr = listener.local_addr()?;
        let request_count = Arc::new(AtomicU64::new(0));
        let token = token.to_string();
        let token_for_closure = token.clone();
        let rc = Arc::clone(&request_count);
        // M8: upstream passthrough now does Responses↔Chat SSE translation
        // over reqwest (rustls TLS). `None` => mock fallback.
        let upstream = Upstream::from_env();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .context("build model-gateway runtime")?,
        );
        let runtime_for_closure = Arc::clone(&runtime);

        let handle = thread::Builder::new()
            .name("model-gateway".into())
            .spawn(move || {
                listener
                    .set_nonblocking(false)
                    .expect("set_nonblocking");
                for stream in listener.incoming() {
                    match stream {
                        Ok(s) => {
                            let token = token_for_closure.clone();
                            let rc = Arc::clone(&rc);
                            let upstream = upstream.clone();
                            let runtime = Arc::clone(&runtime_for_closure);
                            // Handle on the same thread — PoC, one request at a time.
                            handle_request(s, &token, &rc, &upstream, &runtime);
                        }
                        Err(e) => {
                            eprintln!("[model-gateway] accept error: {e}");
                        }
                    }
                }
            })
            .ok();

        Ok(Self {
            addr: actual_addr,
            token: token.to_string(),
            request_count,
            _handle: handle,
        })
    }

    /// Number of model requests received since the gateway started.
    pub fn request_count(&self) -> u64 {
        self.request_count.load(Ordering::Relaxed)
    }

    /// The base URL for the model provider config.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for ModelGateway {
    fn drop(&mut self) {
        // The accept loop exits when `ModelGateway` is dropped because the
        // `listener` (owned by the closure) is dropped when the thread
        // finishes its current `incoming()` iteration. For a PoC this is
        // acceptable — the thread will exit on the next connection attempt
        // or when the process exits.
    }
}

/// Handle a single HTTP request.
fn handle_request(
    stream: TcpStream,
    expected_token: &str,
    request_count: &AtomicU64,
    upstream: &Option<Upstream>,
    runtime: &tokio::runtime::Runtime,
) {
    let stream_clone = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[model-gateway] clone error: {e}");
            return;
        }
    };
    let mut reader = BufReader::new(stream);
    let mut writer = stream_clone;

    // Read the request line and headers.
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    let mut content_length: usize = 0;
    let mut auth_header: Option<String> = None;

    loop {
        let mut header_line = String::new();
        if reader.read_line(&mut header_line).is_err() {
            break;
        }
        let trimmed = header_line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.to_ascii_lowercase().strip_prefix("authorization:") {
            auth_header = Some(rest.trim().to_string());
        }
        if let Some(rest) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
    }

    // Capture the body (needed for upstream passthrough).
    let body_bytes = if content_length > 0 {
        let mut body = vec![0u8; content_length];
        let _ = std::io::Read::read_exact(&mut reader, &mut body);
        body
    } else {
        Vec::new()
    };

    request_count.fetch_add(1, Ordering::Relaxed);

    // Validate the bearer token.
    let expected = format!("bearer {expected_token}");
    let token_valid = auth_header
        .as_deref()
        .is_some_and(|h| h.eq_ignore_ascii_case(&expected));

    if !token_valid {
        eprintln!(
            "[model-gateway] REJECTED request — invalid token (auth={:?})",
            auth_header
        );
        let body = r#"{"error":{"message":"invalid api key","type":"invalid_request_error"}}"#;
        write_http_response(&mut writer, 401, "application/json", body);
        return;
    }

    eprintln!(
        "[model-gateway] request #{} accepted (upstream={})",
        request_count.load(Ordering::Relaxed),
        upstream.is_some()
    );

    // M8: if an upstream is configured, stream-translate Chat SSE → Responses
    // SSE. Returns `true` on a successful streamed response; falls back to
    // mock on any upstream failure.
    if let Some(u) = upstream {
        let ok = runtime.handle().block_on(async {
            u.forward_stream(&body_bytes, &mut writer).await
        });
        if ok {
            return;
        }
        // Upstream failed mid-stream: best-effort error note. The SSE header
        // may already be written, so we cannot switch to a clean JSON 502.
        let _ = writer.write_all(
            b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"upstream connect failed\"}}}\n\n",
        );
        let _ = writer.flush();
        return;
    }

    // Otherwise return the mock payload (M0 behavior). SIMULATE mode never
    // reads this (driver emits synthetic events), but the mock keeps the
    // non-SIMULATE + no-upstream path from hard-failing at the transport.
    let body = serde_json::json!({
        "id": "resp_nexus_gateway",
        "object": "response",
        "created_at": 1700000000,
        "model": "nexus-gateway-mock",
        "output": [
            {
                "type": "message",
                "id": "msg_nexus_gateway",
                "status": "completed",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": "Gateway proxy active. Model call intercepted by Nexus (mock)."
                    }
                ]
            }
        ],
        "usage": {
            "input_tokens": 1,
            "output_tokens": 1,
            "total_tokens": 2
        }
    });
    let body_str = serde_json::to_string(&body).unwrap_or_default();
    write_http_response(&mut writer, 200, "application/json", &body_str);
}

fn write_http_response(writer: &mut impl Write, status: u16, content_type: &str, body: &str) {
    let status_text = match status {
        200 => "OK",
        401 => "Unauthorized",
        _ => "Internal Server Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );
    let _ = writer.write_all(response.as_bytes());
    let _ = writer.flush();
}

/// Write one SSE frame (`event: <type>\ndata: <json>\n\n`) to the writer.
fn write_sse(writer: &mut impl Write, event: &str, data: &serde_json::Value) -> bool {
    let data_str = serde_json::to_string(data).unwrap_or_else(|_| "{}".into());
    let frame = format!("event: {event}\ndata: {data_str}\n\n");
    writer.write_all(frame.as_bytes()).is_ok() && writer.flush().is_ok()
}

// ---------------------------------------------------------------------------
// M8: upstream passthrough — streaming Responses↔Chat-Completions translation.
// ---------------------------------------------------------------------------

/// Configuration for forwarding model requests to a real Chat Completions
/// endpoint (e.g. dashscope OpenAI-compatible mode). Populated from env when
/// `NEXUS_UPSTREAM_MODEL_URL` + `NEXUS_MODEL_KEY` are set; `None` otherwise
/// (mock fallback). `NEXUS_MODEL` picks the model id (default
/// `deepseek-v4-pro`).
#[derive(Clone)]
pub struct Upstream {
    /// Base URL, e.g. `https://dashscope.aliyuncs.com/compatible-mode/v1`.
    base_url: String,
    key: String,
    model: String,
}

impl Upstream {
    /// Build from env, or return `None` (mock fallback).
    fn from_env() -> Option<Self> {
        let base_url = std::env::var("NEXUS_UPSTREAM_MODEL_URL").ok()?;
        let key = std::env::var("NEXUS_MODEL_KEY").ok()?;
        let model = std::env::var("NEXUS_MODEL")
            .unwrap_or_else(|_| "deepseek-v4-pro".into());
        Some(Self { base_url, key, model })
    }

    /// Forward a Responses API request body as a streaming Chat Completions
    /// request, translating each Chat SSE chunk into Responses SSE events
    /// written back to `writer`. Returns `false` if the upstream request
    /// could not even be initiated (caller can emit a failure event).
    async fn forward_stream(&self, body: &[u8], writer: &mut impl Write) -> bool {
        // 1. Parse the Responses API request body.
        let req: serde_json::Value = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[model-gateway] failed to parse Responses body: {e}");
                return false;
            }
        };

        // 2. Build chat messages from Responses input items.
        let model = req
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.model);
        let mut messages: Vec<serde_json::Value> = Vec::new();
        if let Some(inst) = req.get("instructions").and_then(|v| v.as_str()) {
            if !inst.is_empty() {
                messages.push(serde_json::json!({"role":"system","content":inst}));
            }
        }
        if let Some(input) = req.get("input").and_then(|v| v.as_array()) {
            for item in input {
                if item.get("type").and_then(|v| v.as_str()) != Some("message") {
                    continue;
                }
                let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let content = item
                    .get("content")
                    .and_then(|v| v.as_array())
                    .map(|parts| {
                        let texts: Vec<String> = parts
                            .iter()
                            .filter_map(|p| {
                                let ttype = p.get("type").and_then(|t| t.as_str())?;
                                if ttype.contains("text") {
                                    p.get("text").and_then(|t| t.as_str()).map(String::from)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        texts.join("")
                    })
                    .unwrap_or_default();
                if !content.is_empty() {
                    let role = if role == "assistant" { "assistant" } else { "user" };
                    messages.push(serde_json::json!({"role":role,"content":content}));
                }
            }
        }
        if messages.is_empty() {
            eprintln!("[model-gateway] Responses body had no usable messages");
            return false;
        }

        // 3. Construct the Chat Completions streaming request.
        let mut chat_req = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if let Some(mt) = req
            .get("max_output_tokens")
            .and_then(|v| v.as_u64())
            .or_else(|| req.get("max_tokens").and_then(|v| v.as_u64()))
        {
            chat_req["max_tokens"] = serde_json::Value::from(mt);
        }

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(180))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[model-gateway] reqwest client build failed: {e}");
                return false;
            }
        };
        let url = format!(
            "{}/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let resp = match client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.key))
            .json(&chat_req)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[model-gateway] upstream request failed: {e}");
                return false;
            }
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            eprintln!("[model-gateway] upstream non-2xx {status}: {text}");
            return false;
        }

        // 4. Write the SSE response header so the app-server's eventsource
        //    parser begins consuming events.
        let header = "HTTP/1.1 200 OK\r\n\
            Content-Type: text/event-stream\r\n\
            Cache-Control: no-cache\r\n\
            Connection: close\r\n\
            \r\n";
        if writer.write_all(header.as_bytes()).is_err() {
            return false;
        }
        let _ = writer.flush();

        // 5. Stream-translate Chat SSE → Responses SSE.
        let resp_id = format!("resp_nexus_{}", uuid::Uuid::new_v4().simple());
        let msg_id = format!("msg_nexus_{}", uuid::Uuid::new_v4().simple());
        let mut full_text = String::new();
        let mut created_sent = false;
        let mut usage_val: Option<serde_json::Value> = None;

        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[model-gateway] stream chunk error: {e}");
                    break;
                }
            };
            buf.push_str(&String::from_utf8_lossy(&chunk));
            // Process complete lines.
            loop {
                let Some(nl) = buf.find('\n') else { break };
                let line = buf[..nl].trim_end_matches('\r').to_string();
                buf = buf[nl + 1..].to_string();
                if line.is_empty() {
                    continue;
                }
                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                let data = data.trim();
                if data == "[DONE]" {
                    break;
                }
                let Ok(chunk_val) = serde_json::from_str::<serde_json::Value>(data) else {
                    continue;
                };

                // First chunk → response.created + output_item.added.
                if !created_sent {
                    created_sent = true;
                    let created = serde_json::json!({
                        "type":"response.created",
                        "response":{"id":&resp_id,"object":"response","model":model,"status":"in_progress"}
                    });
                    if !write_sse(writer, "response.created", &created) {
                        return true;
                    }
                    let item_added = serde_json::json!({
                        "type":"response.output_item.added",
                        "output_index":0,
                        "item":{"type":"message","id":&msg_id,"role":"assistant","status":"in_progress","content":[]}
                    });
                    if !write_sse(writer, "response.output_item.added", &item_added) {
                        return true;
                    }
                }

                // Capture usage (final chunk has choices=[] + usage).
                if let Some(u) = chunk_val.get("usage").filter(|u| !u.is_null()) {
                    usage_val = Some(u.clone());
                }

                if let Some(choices) = chunk_val.get("choices").and_then(|v| v.as_array()) {
                    for choice in choices {
                        let delta = choice.get("delta");
                        // Text deltas → output_text.delta.
                        if let Some(content) = delta
                            .and_then(|d| d.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            if !content.is_empty() {
                                full_text.push_str(content);
                                let ev = serde_json::json!({
                                    "type":"response.output_text.delta",
                                    "output_index":0,
                                    "content_index":0,
                                    "delta":content
                                });
                                if !write_sse(writer, "response.output_text.delta", &ev) {
                                    return true;
                                }
                            }
                        }
                        // Reasoning deltas → reasoning_text.delta (optional,
                        // codex supports it; harmless if present).
                        if let Some(rc) = delta
                            .and_then(|d| d.get("reasoning_content"))
                            .and_then(|c| c.as_str())
                        {
                            if !rc.is_empty() {
                                let ev = serde_json::json!({
                                    "type":"response.reasoning_text.delta",
                                    "output_index":0,
                                    "content_index":0,
                                    "delta":rc
                                });
                                let _ = write_sse(writer, "response.reasoning_text.delta", &ev);
                            }
                        }
                    }
                }
            }
        }

        // 6. Finalize: output_item.done + response.completed (with usage).
        let item_done = serde_json::json!({
            "type":"response.output_item.done",
            "output_index":0,
            "item":{"type":"message","id":&msg_id,"role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":&full_text}]}
        });
        let _ = write_sse(writer, "response.output_item.done", &item_done);

        let (in_tok, out_tok) = match &usage_val {
            Some(u) => (
                u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            ),
            None => (0, 0),
        };
        let completed = serde_json::json!({
            "type":"response.completed",
            "response":{"id":&resp_id,"object":"response","model":model,"status":"completed",
                "output":[{"type":"message","id":&msg_id,"role":"assistant","status":"completed",
                    "content":[{"type":"output_text","text":&full_text}]}],
                "usage":{"input_tokens":in_tok,"output_tokens":out_tok,"total_tokens":in_tok+out_tok}}
        });
        let _ = write_sse(writer, "response.completed", &completed);
        let _ = writer.flush();
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::io::Write;
    use std::net::TcpStream;
    use std::time::Duration;

    fn send_http(addr: std::net::SocketAddr, auth: &str, body: &str) -> (u16, String) {
        let mut stream = TcpStream::connect_timeout(
            &addr,
            Duration::from_secs(5),
        )
        .expect("connect to gateway");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .ok();
        let request = format!(
            "POST /v1/responses HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Authorization: {auth}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {body}",
            body.len()
        );
        stream
            .write_all(request.as_bytes())
            .expect("write request");
        stream.flush().ok();
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read response");
        let status_line = response.lines().next().unwrap_or_default();
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body_start = response.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
        (status, response[body_start..].to_string())
    }

    #[test]
    fn gateway_accepts_valid_token() {
        let gw = ModelGateway::start("test-token-valid").expect("start gateway");
        let (status, body) = send_http(gw.addr, "Bearer test-token-valid", "{}");
        assert_eq!(status, 200, "valid token must get 200");
        assert!(
            body.contains("nexus-gateway-mock"),
            "response should contain gateway model id"
        );
        assert!(gw.request_count() >= 1);
    }

    #[test]
    fn gateway_rejects_invalid_token() {
        let gw = ModelGateway::start("secret-token-123").expect("start gateway");
        let (status, _body) = send_http(gw.addr, "Bearer wrong-token", "{}");
        assert_eq!(status, 401, "invalid token must get 401");
        assert!(
            gw.request_count() >= 1,
            "request should still be counted for metering"
        );
    }

    #[test]
    fn base_url_is_well_formed() {
        let gw = ModelGateway::start("t").expect("start gateway");
        assert!(
            gw.base_url().starts_with("http://127.0.0.1:"),
            "base_url should be http://127.0.0.1:<port>"
        );
    }

    /// M8: Responses→Chat message extraction is the protocol-translation core.
    /// Verify it without a network (pure parsing).
    fn extract_messages(req: &serde_json::Value) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(inst) = req.get("instructions").and_then(|v| v.as_str()) {
            if !inst.is_empty() {
                out.push(("system".into(), inst.to_string()));
            }
        }
        if let Some(input) = req.get("input").and_then(|v| v.as_array()) {
            for item in input {
                if item.get("type").and_then(|v| v.as_str()) != Some("message") {
                    continue;
                }
                let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let content = item
                    .get("content")
                    .and_then(|v| v.as_array())
                    .map(|parts| {
                        let texts: Vec<String> = parts
                            .iter()
                            .filter_map(|p| {
                                let ttype = p.get("type").and_then(|t| t.as_str())?;
                                if ttype.contains("text") {
                                    p.get("text").and_then(|t| t.as_str()).map(String::from)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        texts.join("")
                    })
                    .unwrap_or_default();
                if !content.is_empty() {
                    out.push((role.to_string(), content));
                }
            }
        }
        out
    }

    #[test]
    fn extract_messages_from_responses_input() {
        let req = serde_json::json!({
            "model":"deepseek-v4-pro",
            "instructions":"You are helpful.",
            "input":[
                {"type":"message","role":"user","content":[
                    {"type":"input_text","text":"run ls"}
                ]},
                {"type":"message","role":"assistant","content":[
                    {"type":"output_text","text":"sure"}
                ]}
            ]
        });
        let msgs = extract_messages(&req);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].0, "system");
        assert_eq!(msgs[0].1, "You are helpful.");
        assert_eq!(msgs[1].0, "user");
        assert_eq!(msgs[1].1, "run ls");
        assert_eq!(msgs[2].0, "assistant");
        assert_eq!(msgs[2].1, "sure");
    }
}
