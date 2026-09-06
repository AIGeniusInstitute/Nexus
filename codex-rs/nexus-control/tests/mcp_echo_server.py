#!/usr/bin/env python3
"""最小 stdio MCP server（JSON-RPC 2.0 over stdio），用于 M19 MCP Gateway 真实转发测试。

实现 initialize / notifications/initialized / tools/list / tools/call。
暴露一个 echo 工具：原样回传 arguments，加前缀确认是真实 MCP 调用（非 stub）。
"""
import json, sys

def read_msg():
    data = b""
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        data += line
        try:
            return json.loads(data.decode())
        except json.JSONDecodeError:
            continue

def write_msg(msg):
    sys.stdout.write(json.dumps(msg, ensure_ascii=False) + "\n")
    sys.stdout.flush()

def main():
    server_info = {"name": "nexus-echo-mcp", "version": "1.0.0"}
    capabilities = {"tools": {}}
    while True:
        msg = read_msg()
        if msg is None:
            break
        if "id" not in msg:
            continue  # notification
        method = msg.get("method", "")
        params = msg.get("params", {}) or {}
        if method == "initialize":
            write_msg({
                "jsonrpc": "2.0", "id": msg["id"],
                "result": {
                    "protocolVersion": "2024-11-05",
                    "serverInfo": server_info,
                    "capabilities": capabilities,
                }
            })
        elif method == "notifications/initialized":
            continue
        elif method == "tools/list":
            write_msg({
                "jsonrpc": "2.0", "id": msg["id"],
                "result": {
                    "tools": [{
                        "name": "echo",
                        "description": "Echo back the arguments (real MCP, not stub).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"message": {"type": "string"}},
                            "required": ["message"],
                        },
                    }]
                }
            })
        elif method == "tools/call":
            name = params.get("name", "")
            args = params.get("arguments", {}) or {}
            if name == "echo":
                text = args.get("message", "")
                write_msg({
                    "jsonrpc": "2.0", "id": msg["id"],
                    "result": {
                        "content": [{"type": "text", "text": f"echo:{text}"}],
                        "isError": False,
                    }
                })
            else:
                write_msg({
                    "jsonrpc": "2.0", "id": msg["id"],
                    "result": {
                        "content": [{"type": "text", "text": f"unknown tool: {name}"}],
                        "isError": True,
                    }
                })
        elif method == "shutdown":
            write_msg({"jsonrpc": "2.0", "id": msg["id"], "result": {}})
            break
        else:
            write_msg({"jsonrpc": "2.0", "id": msg["id"],
                       "error": {"code": -32601, "message": f"unknown method: {method}"}})

if __name__ == "__main__":
    main()
