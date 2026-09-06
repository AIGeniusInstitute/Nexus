# Nexus M9 PRD — Function Calling 双向协议转换

## 背景

M8 将 Nexus 跨越到真实模型联调（Responses↔Chat SSE 纯文本转换，turn completed + 真实计量）。但 gateway 当前**不转换 tools/function calling**——模型无法在真实回复中触发工具调用，因此真实审批端到端（非 SIMULATE）无法验证。

M3 已用 SIMULATE 验证审批协议层（driver park → respond_approval → 落库）。M9 补齐 gateway 的 function calling 双向转换，使"真实模型 → 真实工具调用 → 真实审批"完整链路可用。

## 协议死结（M8 已解，M9 延续）

codex 强制 `wire_api="responses"`，dashscope 仅 Chat Completions。gateway 必须做双向流式转换。M8 处理纯文本；M9 处理 tools + tool_calls。

## 证据基线（实测 2026-09-06）

deepseek-v4-pro 经 dashscope 实测确认支持 function calling：
- 流式先输出 `delta.reasoning_content`（思考），再输出 `delta.tool_calls`
- 首个 tool_calls chunk：`{index:0, id:"call_xxx", type:"function", function:{name:"get_weather", arguments:""}}`
- 后续 chunk：`{index:0, id:"", type:"function", function:{name:"", arguments:"<分片>"}}`（arguments 字符串分片累积）
- codex 侧（`codex-api/src/sse/responses.rs:528-529`）：忽略 `response.function_call_arguments.delta/done`，只靠 `response.output_item.done` 拿完整 function_call item
- codex tools 是**扁平**结构 `{type:"function",name,description,parameters,strict}`；dashscope 是**嵌套** `{type:"function",function:{name,description,parameters}}`

## 任务

### T9-1 请求方向：Responses → Chat tools 转换
- tools 位置兼容两种：顶层 `tools` 字段（标准路径）+ `input` 内 `type=="additional_tools"` item 的 `tools`（responses-lite 路径）
- 扁平→嵌套：`{type:"function",name,description,parameters,strict}` → `{type:"function",function:{name,description,parameters}}`（丢 strict/defer_loading，dashscope 不需要）
- `tool_choice` "auto" 直传
- input 的 `function_call` item → assistant message 的 `tool_calls`（多轮：模型之前的工具调用）
- input 的 `function_call_output` item → `{role:"tool",tool_call_id,content}` message

### T9-2 响应方向：Chat tool_calls → Responses function_call
- 累积 `delta.tool_calls` 按 index 分组：首个 chunk 拿 id+name，后续 chunk 拼接 arguments 分片
- 等流结束（finish_reason="tool_calls" 或 [DONE]）后，合成：
  - `response.output_item.added`（item type=function_call, status=in_progress）
  - `response.output_item.done`（完整 function_call item: `{type:"function_call",name,arguments(字符串),call_id}`）
- codex 忽略 arguments delta，一次性发即可（Simplicity First，不逐片发）
- 纯文本 turn（无 tool_calls）走 M8 路径不变

### T9-3 真实审批端到端 e2e
- 不设 NEXUS_SIMULATE_APPROVAL → app-server 真实调模型经 gateway
- 模型决定调用 shell 工具 → 返回 function_call → codex 发 CommandExecutionRequestApproval
- driver park → 人 resolve → driver respond_approval → turn completed/interrupted
- 验证真实模型驱动的审批闭环（非 SIMULATE 注入）

## 验收标准

| AC | 描述 |
|----|------|
| AC9.1 | gateway 请求方向 tools 正确转换（扁平→嵌套），dashscope 接受并返回 tool_calls |
| AC9.2 | gateway 响应方向 tool_calls 累积成 function_call item，codex 解析成功 |
| AC9.3 | 真实模型触发 function_call → codex 发 CommandExecutionRequestApproval（非 SIMULATE） |
| AC9.4 | 审批 resolve → driver respond_approval → turn completed/denied/interrupted |
| AC9.5 | SIMULATE + 纯文本 turn 零回归（M3/M4/M8 不退化） |

## 非目标（留后续，speculative 不空写）

- IM Bot 推送审批卡片（需飞书/钉钉 bot token + sender→userId 解析器）
- per-tenant 独占 slot 隔离（需真实多租户场景）
- 多 Pod 分布式 driver 池（需真实多 Pod K8s + Redis turn 分发）
- custom_tool_call（freeform 工具）转换——标准 function calling 足够验证审批链路
