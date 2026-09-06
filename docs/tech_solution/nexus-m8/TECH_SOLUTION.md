# Nexus M8 技术方案 — Responses↔Chat SSE 协议转换 Gateway

## 1. 数据流

```
app-server (wire_api=responses, SSE)
  │ POST /v1/responses  (Responses API 请求: input items + instructions)
  ▼
Nexus model_gateway (127.0.0.1:<port>)  ← 持有 tokio Runtime, std accept thread
  │ 1. 解析 Responses body → 提取 input items text → chat messages
  │ 2. reqwest POST dashscope /v1/chat/completions (stream=true, include_usage=true)
  │ 3. 边收 dashscope SSE 边转 Responses SSE 写回 app-server
  ▼
dashscope (https://dashscope.aliyuncs.com/compatible-mode/v1)
  SSE: delta.content / delta.reasoning_content / usage chunk / [DONE]
```

## 2. 事件映射（dashscope SSE → codex Responses SSE）

| dashscope chunk | Responses SSE 事件（写回 app-server） |
|-----------------|------------------------------------------|
| 首个 chunk（role=assistant） | `event: response.created` + `event: response.output_item.added`(message item) |
| `delta.content` 非空 | `event: response.output_text.delta`(delta=文本) |
| `delta.reasoning_content` 非空 | `event: response.reasoning_text.delta`(delta=推理) [可选，codex 支持] |
| `finish_reason="stop"` | 标记完成 |
| usage chunk（choices=[]） | `event: response.output_item.done`(完整 message) + `event: response.completed`(usage) |
| `data: [DONE]` | 流结束 |

Responses SSE 帧格式：`event: <type>\ndata: <json>\n\n`

## 3. 请求解析（Responses → Chat）

Responses API 请求 body 结构（codex 发）：
```json
{
  "model": "...",
  "input": [{"type":"message","role":"user","content":[{"type":"input_text","text":"..."}]}],
  "instructions": "...",
  "tools": [...],
  "tool_choice": "...",
  ...
}
```

转换：
- `input[]` message → chat `messages[]`：role 取 `user`/`assistant`，content 取 `input_text.text`/`output_text.text`。
- `instructions` → 首条 `system` message。
- `tools` → 暂不转换（M8 验证纯文本回复 turn completed + 计量；function calling 转换留 M9 增量，非本里程碑阻塞——真实审批端到端在 M3 SIMULATE 已验证协议层）。
- `max_tokens`：dashscope 用 `max_tokens`，codex 请求可能带 `max_output_tokens`，映射。

## 4. 修改点

### 4.1 `model_gateway.rs`（重写 Upstream + handle_request）
- `Upstream { base_url: String, key: String, model: String }`
- `Upstream::from_env()`：读 `NEXUS_UPSTREAM_MODEL_URL` + `NEXUS_MODEL_KEY` + `NEXUS_MODEL`(default deepseek-v4-pro)。
- `Upstream::forward_stream(&self, body, writer, runtime) -> bool`：block_on async reqwest streaming + 转换写回。
- `ModelGateway` 持有 `runtime: Arc<tokio::runtime::Runtime>`，`handle_request` 内 `runtime.handle().block_on(...)`。
- mock fallback 保留（无 upstream env）。

### 4.2 `execpolicy_rules.rs::write_config_toml`
- 加 `model: &str` 参数，写 `model = "{model}"` 行。

### 4.3 `main.rs::run_serve`
- 传 `NEXUS_MODEL`（default deepseek-v4-pro）给 `write_config_toml`。
- env 名统一：`NEXUS_UPSTREAM_MODEL_URL` + `NEXUS_MODEL_KEY`（key 不用 `NEXUS_UPSTREAM_MODEL_KEY`，与 dashscope 语义对齐）+ `NEXUS_MODEL`。
- 标题 `=== Nexus M8: serve ===`。

## 5. 安全
- API key 仅从 env 读，不硬编码、不落日志、不入 memory。
- gateway 仍做 bearer token 校验（app-server→gateway 本地 token），upstream key 注入 `Authorization: Bearer` 给 dashscope。

## 6. 风险与回退
- 若 codex 因缺 function calling 而 turn 不 completed → M8 验证文本回复 turn（模型回复文本=turn 结束），如实测 turn failed 则诚实报告并定位。
- glm-5.2 配额恢复后可切，model 通过 env 配置。
