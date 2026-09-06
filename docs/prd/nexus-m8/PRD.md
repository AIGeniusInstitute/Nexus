# Nexus M8 PRD — 真实模型联调

## 1. 背景

M0–M7 全程使用 SIMULATE 测试模式（`NEXUS_SIMULATE_APPROVAL=1`）：driver 注入合成
审批/请求 + 合成 tokenUsage（in=10/out=20/model=nexus-gateway-mock），gateway 返回
mock payload。**真实模型从未接通**。M8 的核心使命：从 SIMULATE 跨越到真实模型联调。

用户已提供 dashscope OpenAI-compatible 凭证（deepseek-v4-pro + glm-5.2）。

## 2. 核心挑战（协议死结）

| 维度 | codex app-server | dashscope |
|------|------------------|-----------|
| wire_api | **强制 `responses`**（`wire_api="chat"` 已移除，见 `model-provider-info/src/lib.rs:58` `CHAT_WIRE_API_REMOVED_ERROR`） | 仅 Chat Completions（`/v1/chat/completions`） |
| 传输 | **SSE 流式**（`stream_responses_api`，`codex-api/src/sse/responses.rs`） | SSE 流式 |
| 端点 | `POST /v1/responses` | `POST /v1/chat/completions` |

codex 只发 Responses API SSE，dashscope 只收 Chat Completions SSE → **gateway 必须做
Responses↔ChatCompletions 双向流式协议转换**。

## 3. 验证依据

- dashscope chat completions 端点 HTTP 200，凭证有效。
- deepseek-v4-pro streaming SSE 格式：`delta.content`(文本增量) / `delta.reasoning_content`(推理增量) / 末尾 usage chunk(`prompt_tokens`/`completion_tokens`/`reasoning_tokens`) / `data: [DONE]`。
- glm-5.2 当前 `insufficient_quota`（配额超限），M8 默认用 deepseek-v4-pro，glm-5.2 留备选。
- codex SSE 解析最小事件集（`codex-api/src/sse/responses.rs:355-549`）：`response.created` + `response.output_item.added` + `response.output_text.delta`(多次) + `response.output_item.done` + `response.completed`(带 usage，否则报 "stream closed before response.completed")。`content_part.added/done`/`output_text.done` 等为 trace 级 unhandled（可忽略）。

## 4. 任务

### T8-1 gateway Responses→Chat SSE 流式协议转换
重写 `model_gateway.rs` 的 `Upstream`：用 reqwest（已在依赖，`rustls-tls`）做 HTTPS streaming。
- `Upstream { base_url, key, model }` from env（`NEXUS_UPSTREAM_MODEL_URL`/`NEXUS_MODEL_KEY`/`NEXUS_MODEL`）。
- `forward_stream(body, writer)`：解析 Responses 请求 body → 提取 input items 文本 → chat messages（instructions→system）→ reqwest POST streaming chat completions → 边收 SSE 边转 Responses SSE 写回 app-server TcpStream。
- 事件映射：首 chunk→`response.created`+`output_item.added`；`delta.content`→`output_text.delta`；usage chunk→`output_item.done`+`response.completed`(usage)；`[DONE]`→结束。
- gateway 持有 `tokio::runtime::Runtime`，std accept thread 内 `handle.block_on(async{...})` 调 async reqwest streaming + 同步写 TcpStream。
- mock fallback 保留（无 upstream env 时）。

### T8-2 config.toml model 配置
`write_config_toml` 加 `model` 参数（env `NEXUS_MODEL`，default `deepseek-v4-pro`）。`wire_api` 保持 `responses`（codex 强制）。`main.rs run_serve` 传 model。

### T8-3 真实 turn e2e 验证
`NEXUS_SIMULATE_APPROVAL` 不设 → app-server 真实调模型经 gateway→dashscope → 真实回复 + 真实 `tokenUsage` 落 `usage_records` + cost 推导（dashscope cost=0，验证落库链路）+ `turns.model` 写真实 model 名。验证 turn completed + 真实 usage 非空。

## 5. 验收标准

- AC8.1 gateway 转发真实模型请求（非 mock）：`gateway.request_count() > 0` + app-server 收到真实回复文本。
- AC8.2 真实 token usage 落库：`usage_records` 行 model=deepseek-v4-pro + input_tokens/output_tokens 来自 dashscope usage chunk（非 10/20 mock）。
- AC8.3 cost 推导：`turns.cost_micros` + `/v1/usage` 聚合返回真实 model 行（cost=0 因 dashscope cost=0，但落库链路通）。
- AC8.4 turn completed：真实模型 turn 状态 completed，无 "stream closed before response.completed"。
- AC8.5 零回归：SIMULATE 模式（`NEXUS_SIMULATE_APPROVAL=1`）仍全过 M3–M7 e2e（审批/学习/amendment/计量/并发）。

## 6. 非目标（留 M9，speculative 风险）

- **IM Bot 推送审批卡片**：无飞书/钉钉 bot token + sender→userId 解析器，凭证缺失。
- **per-tenant 独占 slot 隔离**：单租户下无验证价值（M4 `max_concurrent_turns` DB 门控已覆盖并发上限），需真实多租户场景驱动，speculative。
- **多 Pod 分布式 driver 池（Redis slot 调度）**：需真实多 Pod K8s + Nexus 架构改 Redis turn 分发，单 Pod 无法验证分布式正确性，speculative。

以上三项待真实多租户/多 Pod/bot 凭证环境就绪后，场景驱动开工，避免空写未经验证的框架（Simplicity First）。
