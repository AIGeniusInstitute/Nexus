//! M19 MCP Gateway 真实转发 — roadmap T7-1。
//!
//! 最小 stdio JSON-RPC 2.0 客户端：spawn MCP server 子进程，完成
//! initialize 握手 + tools/call，返回 CallToolResult。不引入 rmcp/codex
//! 重依赖（Simplicity First），仅用 tokio + serde_json（已是依赖）。
//!
//! MCP stdio 传输 = newline-delimited JSON。每请求带自增 id，读响应时
//! 跳过无 id 的 notification（progress 等）直到匹配 id 的 response。

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: AtomicI64,
}

#[derive(Debug, Clone)]
pub struct CallToolResult {
    pub content: Value,        // array of content blocks
    pub is_error: bool,
}

impl McpClient {
    /// 启动 MCP server 子进程（stdio transport）。
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        cwd: Option<&str>,
    ) -> Result<Self> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("spawn mcp server `{command}`: {e}"))?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: AtomicI64::new(1),
        })
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        let line = serde_json::to_string(&req)? + "\n";
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;

        // 读响应：跳过无 id 的 notification，匹配 id 返回
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = self.stdout.read_line(&mut buf).await?;
            if n == 0 {
                return Err(anyhow!("mcp server stdout closed (method={method})"));
            }
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                continue;
            }
            let msg: Value = serde_json::from_str(trimmed)
                .map_err(|e| anyhow!("parse mcp line: {e}; line={trimmed}"))?;
            if msg.get("id").and_then(|v| v.as_i64()) != Some(id) {
                continue; // notification 或别人的响应，跳过
            }
            if let Some(err) = msg.get("error") {
                return Err(anyhow!("mcp error: {err}"));
            }
            return Ok(msg["result"].clone());
        }
    }

    /// JSON-RPC initialize 握手。返回 server 信息。
    pub async fn initialize(&mut self) -> Result<Value> {
        let result = self
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "nexus-control", "version": env!("CARGO_PKG_VERSION")}
                }),
            )
            .await?;
        // 发 notifications/initialized（无 id，无需响应）
        let notif = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        let line = serde_json::to_string(&notif)? + "\n";
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(result)
    }

    /// 调用一个工具。
    pub async fn call_tool(&mut self, name: &str, arguments: Option<Value>) -> Result<CallToolResult> {
        let params = json!({"name": name, "arguments": arguments.unwrap_or(Value::Object(Default::default()))});
        let result = self.send_request("tools/call", params).await?;
        let content = result.get("content").cloned().unwrap_or(Value::Array(vec![]));
        let is_error = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
        Ok(CallToolResult { content, is_error })
    }

    /// 关闭：尝试 shutdown 请求 + kill。
    pub async fn shutdown(&mut self) {
        let _ = self.send_request("shutdown", json!({})).await;
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }

    /// 等待子进程退出（kill_on_drop 兜底）。
    pub fn is_closed(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // kill_on_drop 已设，子进程会被 kill；这里尽力收尸
        let _ = self.child.start_kill();
    }
}

/// 带超时的调用封装（用于 connectors invoke，避免长 hang）。
pub async fn call_tool_with_timeout(
    client: &mut McpClient,
    name: &str,
    args: Option<Value>,
    timeout: Duration,
) -> Result<CallToolResult> {
    tokio::time::timeout(timeout, client.call_tool(name, args))
        .await
        .map_err(|_| anyhow!("mcp call_tool timeout ({:?})", timeout))?
}

#[cfg(test)]
mod tests {
    // 协议正确性靠 e2e（真实 Python MCP server）验证；此处无纯函数可单测。
    // McpClient 的逻辑是 I/O，单测需 spawn 子进程，归入 e2e。
}
