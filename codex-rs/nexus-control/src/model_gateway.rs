//! Minimal HTTP model gateway proxy for T0-7.
//!
//! Proves that model traffic from the app-server can be routed through a
//! control-plane proxy that validates tokens and records metering. The
//! gateway is a single-threaded `std::net::TcpListener` server that:
//!
//! 1. Accepts `POST /v1/responses` requests.
//! 2. Validates `Authorization: Bearer <expected_token>`.
//! 3. Returns a minimal OpenAI-compatible Responses API payload so the
//!    app-server turn can complete.
//! 4. Records per-token request count for metering.
//!
//! This is PoC-grade — no real model call is made. For production, this
//! would forward to a real provider with per-tenant auth.

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
        // M2: optional upstream passthrough (real model). When
        // NEXUS_UPSTREAM_MODEL_URL + NEXUS_UPSTREAM_MODEL_KEY are set, the
        // gateway forwards requests to a real OpenAI-compatible endpoint and
        // relays the response. Otherwise it falls back to the mock payload.
        let upstream = Upstream::from_env();

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
                            // Handle on the same thread — PoC, one request at a time.
                            handle_request(s, &token, &rc, &upstream);
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
fn handle_request(stream: TcpStream, expected_token: &str, request_count: &AtomicU64, upstream: &Option<Upstream>) {
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

    eprintln!("[model-gateway] request #{} accepted", request_count.load(Ordering::Relaxed));

    // M2: if an upstream is configured, forward + relay (real model).
    if let Some(resp) = upstream.as_ref().and_then(|u| u.forward(&body_bytes)) {
        write_http_response(&mut writer, resp.status, &resp.content_type, &resp.body);
        return;
    }

    // Otherwise return the mock payload (M0 behavior).
    // The app-server expects a `response` with at least an `id`, `model`,
    // and `output` containing message items.
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
                        "text": "Gateway proxy active. Model call intercepted by Nexus T0-7."
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

// ---------------------------------------------------------------------------
// M2: optional upstream passthrough (real model proxy).
// ---------------------------------------------------------------------------

/// Configuration for forwarding model requests to a real upstream endpoint.
/// Populated from env when both `NEXUS_UPSTREAM_MODEL_URL` and
/// `NEXUS_UPSTREAM_MODEL_KEY` are set; `None` otherwise (mock fallback).
#[derive(Clone)]
pub struct Upstream {
    host: String,
    port: u16,
    path: String,
    key: String,
}

/// A relayed upstream response.
struct UpstreamResponse {
    status: u16,
    content_type: String,
    body: String,
}

impl Upstream {
    /// Build from env, or return a sentinel that always falls back to mock.
    fn from_env() -> Option<Self> {
        let url = std::env::var("NEXUS_UPSTREAM_MODEL_URL").ok()?;
        let key = std::env::var("NEXUS_UPSTREAM_MODEL_KEY").ok()?;
        Self::parse_url(&url, key)
    }

    /// Parse `http://host[:port][/path]` into an `Upstream`.
    fn parse_url(url: &str, key: String) -> Option<Self> {
        let rest = url.strip_prefix("http://").or_else(|| url.strip_prefix("https://"))?;
        let (hostport, path) = match rest.split_once('/') {
            Some((h, p)) => (h, format!("/{p}")),
            None => (rest, "/".into()),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().unwrap_or(80)),
            None => (hostport.to_string(), 80),
        };
        Some(Self { host, port, path, key })
    }

    /// Forward the request body to the upstream and relay the response.
    /// Returns `None` on connection failure (caller falls back to mock).
    fn forward(&self, body: &[u8]) -> Option<UpstreamResponse> {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::time::Duration;

        let ip: std::net::IpAddr = self.host.parse().ok()?;
        let mut stream = TcpStream::connect_timeout(
            &std::net::SocketAddr::new(ip, self.port),
            Duration::from_secs(10),
        )
        .ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(60))).ok();
        let request = format!(
            "POST {path} HTTP/1.1\r\n\
             Host: {host}:{port}\r\n\
             Authorization: Bearer {key}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {clen}\r\n\
             Connection: close\r\n\
             \r\n",
            path = self.path, host = self.host, port = self.port,
            key = self.key, clen = body.len()
        );
        stream.write_all(request.as_bytes()).ok()?;
        stream.write_all(body).ok()?;
        stream.flush().ok()?;
        let mut resp = String::new();
        stream.read_to_string(&mut resp).ok()?;
        let status: u16 = resp
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(502);
        let content_type = resp
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-type:"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "application/json".into());
        let body_start = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
        Some(UpstreamResponse {
            status,
            content_type,
            body: resp[body_start..].to_string(),
        })
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
}
