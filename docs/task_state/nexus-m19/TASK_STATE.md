# M19 MCP Gateway 真实转发 任务状态

## 已完成（T19-1~T19-5）
- T19-1 `mcp.rs`（~190 行）：最小 stdio JSON-RPC 2.0 客户端
  - McpClient::spawn(command,args,env,cwd) → tokio::process::Child + BufReader stdout
  - send_request：newline-delimited JSON，自增 id，跳过 notification 匹配 id
  - initialize（握手 + notifications/initialized）+ call_tool + shutdown + Drop kill_on_drop
  - call_tool_with_timeout（30s 超时）
- T19-2 connectors.rs invoke_mcp：解析 config_json{command,args,env,cwd} → spawn → initialize → call_tool → 落 tool_call_logs(success=!is_error) → compute_quality；旧 connector 无 command → stub 回退
- T19-3 http_server.rs connector_invoke：调 invoke_mcp，返回 {call_id,mcp,success,result}
- T19-4 测试 fixture `tests/mcp_echo_server.py`：真实 stdio MCP server（initialize/tools/list/tools/call echo 工具）
- T19-5 Docker：Dockerfile 加 python3 + COPY mcp_echo_server.py；deploy.sh 准备脚本到 bin/

## 关键决策
- 自写 stdio JSON-RPC 客户端，不引入 rmcp/codex-rmcp-client 重依赖（Simplicity First，协议最小子集）
- config_json 存 command（零 schema ALTER，复用 JSONB）
- 每 invoke spawn+shutdown（简单，后续可优化连接池）
- 旧 connector 向后兼容（无 command → stub 回退 mcp:false）

## e2e 验证（Docker 8765，容器内 python3 + /app/mcp_echo_server.py）
- AC1 connector config_json 含 command → 真实 spawn MCP server
- AC2 echo 工具 → result `echo:hello-nexus`（真实 MCP 非 stub，mcp:true）
- AC3 success=true 落 tool_call_logs
- AC4 未知工具 → is_error=true success=false（"unknown tool: nonexistent"）
- AC5 quality_score=0.5（2 调用 1 成功 1 失败）
- AC6 calls 历史 2 条（echo success + nonexistent fail）
- AC7 零回归（M16 connectors CRUD 不退化）

## 验证：cargo check 0 error 0 warning，cargo test 32/32 零回归
