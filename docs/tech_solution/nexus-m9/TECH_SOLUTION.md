# Nexus M9 技术方案 — Function Calling 双向协议转换

## 数据流

```
codex app-server                 Nexus Gateway                  dashscope
(Responses API)                   (协议转换)                    (Chat Completions)
     │                                │                              │
     │  POST /v1/responses            │                              │
     │  {model,input,instructions,     │                              │
     │   tools(扁平 or additional_tools),│                            │
     │   tool_choice:"auto"}           │                              │
     │ ─────────────────────────────► │                              │
     │                                │  POST /chat/completions       │
     │                                │  {model,messages,             │
     │                                │   tools(嵌套),stream:true}    │
     │                                │ ────────────────────────────► │
     │                                │                              │
     │                                │  ◄── SSE: reasoning_content  │
     │                                │  → response.reasoning_text.delta│
     │                                │  ◄── SSE: tool_calls[id+name]│
     │                                │  ◄── SSE: tool_calls[args分片]│
     │                                │      (累积，不发 delta)        │
     │                                │  ◄── SSE: finish_reason=tool_calls│
     │                                │  ◄── SSE: usage              │
     │                                │                              │
     │  ◄── SSE: response.created     │                              │
     │  ◄── SSE: output_item.added    │  (type=message, 纯文本路径)  │
     │       (type=function_call,     │  或                          │
     │        status=in_progress)      │                              │
     │  ◄── SSE: output_text.delta ×N │  (纯文本)                    │
     │  ◄── SSE: output_item.done     │                              │
     │       (完整 function_call:     │                              │
     │        type,name,arguments,    │                              │
     │        call_id)                 │                              │
     │  ◄── SSE: response.completed  │                              │
     │       (usage)                  │                              │
     │                                │                              │
     ▼ codex 解析 function_call       │                              │
     → router.build_tool_call         │                              │
     → dispatch (shell/exec)          │                              │
     → approvals.request_approval     │                              │
     → CommandExecutionRequestApproval│                              │
     │ ──────► driver park            │                              │
     │ ◄────── respond_approval ◄────┤ 人 resolve (POST /v1/approvals/{id}/resolve)
     ▼ turn completed/interrupted     │                              │
```

## 事件映射表

### 请求方向（Responses → Chat）

| Responses 请求字段 | Chat 请求字段 | 转换 |
|---------------------|---------------|------|
| 顶层 `tools[]`（扁平） | `tools[]`（嵌套） | `{type:"function",name,desc,params,strict}` → `{type:"function",function:{name,desc,params}}` |
| `input[].type=="additional_tools"` 的 `tools[]` | `tools[]` | 同上（responses-lite 路径） |
| `tool_choice:"auto"` | `tool_choice:"auto"` | 直传 |
| `input[].type=="function_call"` | assistant message `tool_calls` | `{name,arguments,call_id}` → `{id:call_id,type:"function",function:{name,arguments}}` |
| `input[].type=="function_call_output"` | `{role:"tool",tool_call_id,content}` | `{call_id,output}` → tool message |

### 响应方向（Chat → Responses）

| dashscope SSE | Responses SSE（写回 app-server） |
|---------------|-----------------------------------|
| 首个 chunk（role=assistant） | `response.created` + `output_item.added`(message) |
| `delta.content` | `response.output_text.delta` |
| `delta.reasoning_content` | `response.reasoning_text.delta` |
| `delta.tool_calls`（id+name 首 chunk） | 累积，不发 |
| `delta.tool_calls`（arguments 分片） | 累积，不发 |
| 流结束 + 有 tool_calls | `output_item.added`(function_call,in_progress) + `output_item.done`(完整 function_call item) |
| usage chunk | `response.completed`(usage) |

## 修改点

仅改 `codex-rs/nexus-control/src/model_gateway.rs`（gateway 内的转换逻辑）：

1. `forward_stream` 请求构造：新增 tools 转换（扁平→嵌套，双位置）+ function_call/function_call_output item → messages
2. `forward_stream` 响应流：新增 `delta.tool_calls` 累积逻辑 + 流结束发 function_call output_item.added/done
3. 纯文本路径（无 tool_calls）保持 M8 行为不变

**不改 codex 内核**（所有改动在 nexus-control crate 内）。**无 schema 变更**（复用 M3 approval_tickets + M4 usage_records）。

## 简化决策（Simplicity First）

1. **arguments 累积后一次性发**：codex 忽略 `function_call_arguments.delta`（responses.rs:528-529），只靠 `output_item.done` 拿完整 item。不逐片发 delta，比流式转发简单。
2. **tools 双位置兼容**：顶层 `tools` + `input` 内 `additional_tools` item，两处都查，不假设 use_responses_lite 路径。
3. **纯文本路径不变**：无 tool_calls 时走 M8 的 output_text.delta 路径，零回归。
4. **custom_tool_call 不转**：标准 function calling 足够验证审批链路，freeform 工具留后续。

## 风险

- **codex 是否发 tools**：deepseek-v4-pro 走自定义 gateway provider，use_responses_lite 未定。双位置兼容覆盖两种情况。
- **模型是否触发 function_call**：e2e 需 prompt 引导模型调用 shell（如"列出当前目录文件"）。若模型不调工具，turn 走纯文本路径（M8 已验证）。
- **多轮 function_call_output**：M9 验证单轮（首次调用→审批）即可，多轮工具结果回传可后续。

## 测试

- 单测：`flatten_tools_to_chat`（扁平→嵌套）+ `accumulate_tool_calls`（分片累积成 function_call item），纯解析无网络
- e2e：真实 deepseek-v4-pro + gateway，模型触发 function_call → 真实审批 resolve → turn completed/interrupted
- 零回归：SIMULATE 审批 + M8 纯文本 turn
