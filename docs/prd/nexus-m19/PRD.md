# M19 MCP Gateway 真实转发 PRD

## 背景
M16 连接器市场 invoke_stub 只记录调用 intent，不真实转发（留 T7-1）。用户要求完成真实 MCP 转发，并自建真实 MCP server 测试。

## 目标
将 connector invoke 从 stub 升级为真实 MCP 调用：spawn MCP server 子进程，JSON-RPC 2.0 over stdio 完成 initialize → tools/call，结果落 tool_call_logs，重算质量分。

## 功能需求
1. 最小 stdio JSON-RPC 2.0 客户端（自建，不引入 rmcp 重依赖）
2. connector.config_json 存 `{command,args,env,cwd}`（零 schema 改动）
3. invoke_mcp：spawn → initialize → call_tool → 落库 tool_call_logs(success=!is_error) → compute_quality
4. 旧 connector（无 command 字段）回退 stub 语义（向后兼容）
5. 自建真实 Python MCP echo server 做测试 fixture（含 Docker 镜像）

## 验收标准
- AC1 connector config_json 含 command → invoke 真实 spawn MCP server
- AC2 echo 工具调用返回 `echo:<message>`（证明真实 MCP 非 stub）
- AC3 success=true 落 tool_call_logs，result_ref 含 MCP content
- AC4 未知工具 → is_error=true，success=false，质量分下降
- AC5 调用后 quality_score 重算
- AC6 旧 connector（无 command）→ stub 回退，mcp:false
- AC7 零回归（M16 connectors CRUD/质量分不退化）
