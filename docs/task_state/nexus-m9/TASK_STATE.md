# Nexus M9 Task State — Function Calling 双向协议转换

## 任务清单

| 任务 | 状态 | 说明 |
|------|------|------|
| T9-1 gateway 请求方向 tools 转换 | ✅ 完成 | `model_gateway.rs forward_stream`：扁平 tools（顶层 + input additional_tools 双位置）→ chat 嵌套；function_call item → assistant tool_calls；function_call_output → tool role message。`flatten_tool_to_chat` helper。 |
| T9-2 gateway 响应方向 tool_calls 转换 | ✅ 完成 | `delta.tool_calls` 按 index 累积（`merge_tool_call_delta` helper，首 chunk id+name，后续 arguments 分片）；流结束发 `output_item.added`(function_call,in_progress) + `output_item.done`(完整 item: type/name/arguments 字符串/call_id)。codex 忽略 arguments delta，一次性发。 |
| T9-3 真实审批端到端 e2e | ✅ 完成 | `execpolicy_rules write_config_toml` 加 `approval_policy="untrusted"` + `sandbox_mode="danger-full-access"`，让 codex 对非 allow 命令发真实 CommandExecutionRequestApproval。真实模型→gateway→function_call→codex→审批→resolve→执行→回复。 |

## 执行记录

### 关键证据（实测 2026-09-06）
1. **deepseek-v4-pro 支持 function calling**（dashscope 实测）：流式先 `delta.reasoning_content`（思考），再 `delta.tool_calls`（首 chunk id+name+空 arguments，后续 arguments 分片累积字符串），`finish_reason="tool_calls"`。
2. **codex Responses tools 是扁平结构**（`tool_spec.rs`/`responses_api.rs`）：`{type:"function",name,description,parameters,strict}`，无 `function` 嵌套层；dashscope 要嵌套 `{type:"function",function:{...}}`。
3. **codex 忽略 `function_call_arguments.delta`**（`responses.rs:528-529` unhandled），只靠 `output_item.done` 拿完整 function_call item → gateway 累积后一次性发即可。
4. **审批是 codex 内部生成**（`approvals.rs request_approval`）：模型只产 function_call，codex router→dispatch→approval_policy 决定是否发 CommandExecutionRequestApproval。

### 关键 bug / 配置发现
1. **gateway 直连验证**（T9-1+T9-2）：发 Responses 请求（扁平 tools）到 gateway，返回 SSE 含完整 function_call item（output_index:1, name=get_weather, arguments=`{"city": "Beijing"}`, call_id=call_f104...），真实 usage（input=304/output=74）。证明 tools 转换 + tool_calls 累积正确。
2. **codex 默认 approval_policy=OnRequest 不审批**：turn 28/29 模型调 ls/pwd 自动执行无审批（命令匹配 execpolicy allow 或 OnRequest 不触发）。改 `approval_policy="untrusted"` 后，非 allow 命令（pwd）触发真实审批。
3. **ls 匹配 execpolicy allow 自动执行**：turn 28 item 34 commandExecution `/bin/bash -lc ls`（call_id=call_2835...）——证明 gateway 转换的 function_call 被 codex 正确解析并触发工具执行（AC9.2 间接证据）。

### 关键技术决策
1. **arguments 累积后一次性发**（Simplicity First）：codex 忽略 arguments delta，只靠 output_item.done。不逐片发 delta，比流式转发简单。
2. **tools 双位置兼容**：顶层 `tools` + `input` 内 `additional_tools` item，两处都查，不假设 use_responses_lite 路径。
3. **纯文本路径不变**：无 tool_calls 时走 M8 的 output_text.delta 路径，零回归。
4. **approval_policy=untrusted**：让真实审批可触发（非 SIMULATE）。sandbox=danger-full-access 让命令能执行（不阻断）。
5. **custom_tool_call 不转**：标准 function calling 足够验证审批链路，freeform 工具留后续。

### 坑
1. **login 路由是 /v1/auth/login 不是 /v1/login**（M1 起 axum 路由）。
2. **thread id 是 UUID 字符串非数字**（grep `"id":[0-9]*` 匹配空，改 `"id":"[^"]*"`）。
3. **turn_start 在审批 park 时阻塞**：curl turn 请求 hang 直到 resolve（M3 设计，driver drain event_rx 阻塞到 turn completed/interrupted）。需后台发 turn + 单独查 approvals。
4. **pkill -f "nexus-control serve" 匹配启动器自身命令行→自杀 exit 144**（M5 同坑），改 `fuser -k {port}/tcp` 精确清端口。

## 验证结果

- cargo check：0 error 0 warning
- cargo test：25/25（+4：flatten_tool_flat_to_nested / flatten_tool_already_nested_passthrough / accumulate_tool_calls_from_fragments / function_call_input_items_to_chat_messages）
- 真实 e2e（AC9.1-9.5 全过）：
  - AC9.1 gateway tools 转换 ✅（直连：扁平→嵌套，dashscope 返回 tool_calls）
  - AC9.2 tool_calls→function_call item ✅（codex 解析：turn 28 item 34 commandExecution ls）
  - AC9.3 真实审批触发 ✅（approval 25 pending prompt risk=low command=pwd，非 SIMULATE）
  - AC9.4 resolve→respond→completed ✅（approval 25 approved + turn 30 commandExecution pwd 执行 + agentMessage 回复）
  - AC9.5 纯文本+计量零回归 ✅（turn 31 completed 无审批 + usage 真实 dashscope tokens）
