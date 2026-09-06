# Nexus M8 Task State — 真实模型联调

## 任务清单

| 任务 | 状态 | 说明 |
|------|------|------|
| T8-1 gateway Responses→Chat SSE 流式协议转换 | ✅ 完成 | `model_gateway.rs` 重写 `Upstream`：reqwest+rustls streaming，解析 Responses input→chat messages，dashscope SSE→Responses SSE 事件映射（created/output_item.added/output_text.delta/reasoning_text.delta/output_item.done/completed）。multi_thread runtime 解 block_on hang。mock fallback 保留。 |
| T8-2 config.toml model 配置 | ✅ 完成 | `write_config_toml` 加 `model` 参数；`main.rs run_serve` 从 `NEXUS_MODEL` env 读（default deepseek-v4-pro）；`wire_api` 保持 `responses`（codex 强制）。 |
| T8-3 真实 turn e2e 验证 | ✅ 完成 | `NEXUS_SIMULATE_APPROVAL` 不设 → app-server 真实调模型经 gateway→dashscope → 真实回复 + 真实 tokenUsage 落库。`http_server.rs` model fallback 改用 `NEXUS_MODEL` env（真实 model 名落库）。 |

## 执行记录

### 关键 bug 修复
1. **gateway forward() 不可用**（M2 遗留）：`host.parse::<IpAddr>()` 不解析域名 + 无 TLS + port 默认 80。重写为 reqwest+rustls HTTPS streaming。
2. **current_thread runtime block_on hang**：model-gateway std thread 内调 `handle().block_on()` 对 current_thread runtime hang（非 own thread）。改 `new_multi_thread().worker_threads(2)`，worker threads 可跨线程 drive。
3. **model 名未落库**：runtime `ThreadTokenUsageUpdated` 的 token_usage 不带 model → `Usage.model=None` → http_server fallback "nexus-gateway"。改 fallback 用 `NEXUS_MODEL` env，真实 model（deepseek-v4-pro）落库。

### 关键技术决策
1. **协议转换层**：codex 强制 `wire_api="responses"`（`CHAT_WIRE_API_REMOVED_ERROR`），dashscope 仅 Chat Completions → gateway 做 Responses↔Chat SSE 双向流式转换（非可选，是唯一路径）。
2. **SSE 最小事件集**：基于 `codex-api/src/sse/responses.rs:355-549` 分析，`content_part.*/output_text.done` 是 trace 级 unhandled，最小集 = created+output_item.added+output_text.delta+output_item.done+response.completed(usage)。
3. **reasoning_content 转发**：deepseek-v4-pro 是 reasoning 模型，`delta.reasoning_content`→`response.reasoning_text.delta`（codex 支持，无害）。
4. **tools 不转换**（Simplicity First）：M8 验证纯文本回复 turn completed + 计量；function calling 转换（真实审批端到端）留 M9 增量，非本里程碑阻塞（审批协议层 M3 SIMULATE 已验证）。

### 非目标（留 M9，speculative）
- IM Bot 推送审批卡片：无飞书/钉钉 bot token + sender→userId 解析器。
- per-tenant 独占 slot 隔离：单租户无验证价值（M4 max_concurrent_turns DB 门控已覆盖），需真实多租户场景。
- 多 Pod 分布式 driver 池（Redis slot 调度）：需真实多 Pod K8s + Nexus 架构改 Redis turn 分发，单 Pod 无法验证分布式。

## 验证结果

- cargo check：0 error 0 warning
- cargo test：21/21（+1 extract_messages_from_responses_input）
- 真实 e2e：turn 25/26 completed，真实 tokens（input=12625/12616, output=29/17）落 usage_records + turns.model=deepseek-v4-pro
- SIMULATE 零回归：turn 27 mock 10/20 + approval 24 approved（M3 审批 + M4 计量不退化）
