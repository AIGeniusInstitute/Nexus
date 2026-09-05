# Nexus M2 — PRD：执行闭环（单租户 MVP）

> 分支：`feat/nexus-m2` · worktree：`.worktrees/feat/nexus-m2`
> 前置：M1（merge 034fad0）身份+骨架已交付
> 日期：2026-09-06

## 1. 背景与目标

M1 交付了身份域 + REST/WS 双网关 + Web 门户 + CLI + Postgres schema，但 `turn_start` 仅写一条 mock system item——**任务不能真正跑**。M2 的目标是打通**执行闭环**：当用户提交一个 turn，系统真正驱动 Codex app-server 执行，把产生的 item 事件流落库并实时推送到前端，断线重连后能看到完整历史。

路线图对 M2 的定义：
- Runtime 池最小版（K8s Job 调度 + 预热池 + 销毁回收）
- Harness 适配层：`start` / `resume` / `interrupt` 三个动作
- 事件落库（thread/turn/item 三表）+ WS 推送
- **验收：任务能跑、消息能存、断线重连后能看到完整历史**

M2 在单租户本地环境落地（K8s 调度/预热池是多租户扩展期 M5+ 的事；M2 用单进程驱动器，序列化执行，最小正确）。

## 2. 范围

### 2.1 In Scope（M2 必做）
- **T2-1 Runtime 适配层**：把 M0 的同步阻塞 `AppServerProcess` 包到专用驱动线程 + tokio channel 后面，暴露 async 的 `thread/start | resume | turn/start | interrupt`，不阻塞 axum runtime。
- **T2-2 事件落库映射**：app-server `item/started`+`item/completed` 通知 → Postgres `items` 行（seq/item_type/content_ref）；全部通知 → `app_server_events`（raw event_json，UNIQUE(thread_id,seq) 幂等）；`turn/completed` + `thread/tokenUsage/updated` → `turns` 状态/token/cost。
- **T2-3 turn_start 真实接线**：`POST /v1/threads/{id}/turns` 不再写 mock item，改为：建 turn 行 → 驱动 runtime → 流式落库 → 返回 turn_id；turn 完成后状态落 `completed`。
- **T2-4 WS 实时推送**：`ws.rs` 从 1s 轮询改为 `tokio::sync::broadcast` 通道即时推送（保留 poll 兜底回放历史）。
- **T2-5 断线重连完整历史**：app-server 进程被 kill → 驱动器自动 respawn → `thread/resume`（codex_thread_id 已落库）→ 新 turn 继续执行，前端经 WS/GET items 看到无缺口的完整 item 序列。
- **T2-6 model_gateway 真实代理**：gateway 支持 upstream passthrough 模式（配置 `UPSTREAM_MODEL_URL`+`UPSTREAM_MODEL_KEY` 时反代真实 OpenAI-compatible 端点，无配置时回退 mock），config.toml 把 app-server 模型流量指向 gateway，解除沙箱网络阻断（M0 T0-4/T0-7 turn Failed 根因）。
- **T2-7 结构化 tracing + 计量**：每 turn 一条 tracing span；turn 完成写 `turns.input_tokens/output_tokens/cost_micros/model`。

### 2.2 Out of Scope（明确不做）
- K8s Job 调度 / 预热池 / 多 Pod（M5+ 多租户扩展）
- 多 app-server 进程并发（M2 单进程序列化，单租户 MVP 可接受）
- 审批中心 / 策略中心（M3）
- 产物上传 / 计量看板 / OTel 全链路（M4，M2 仅 token 落 turns）
- Fork-Rollback 真实快照（M3，M2 仅 thread/resume 复用 codex 内置状态）

## 3. 验收标准（AC）

| AC | 描述 | 验证 |
|---|---|---|
| AC2.1 | turn 真实执行：POST turn → app-server 产生 item 事件流 → `items` 与 `app_server_events` 有行 → `turns.status=completed` | TC: 起 turn，查 PG items 非空、event_json 非空、turn completed |
| AC2.2 | 事件幂等：resume 时 app-server 重放 item 通知，PG 不产生重复行（UNIQUE(thread_id,turn_id,seq) + UNIQUE(thread_id,seq) ON CONFLICT） | TC: kill→respawn→resume，item 行数 = 首次，无 seq 重复 |
| AC2.3 | WS 实时推送：turn 执行期间前端经 broadcast 收到 item frame，延迟 <1s | TC: ws 连接后起 turn，记录首帧到达时间 |
| AC2.4 | 断线重连完整历史：app-server 进程被 kill，新 turn 仍可执行，GET items 返回全部历史 item 无缺口 | TC: kill -9 进程，起新 turn，items 完整连续 |
| AC2.5 | interrupt：POST turn/interrupt → turn 状态 interrupted，驱动器 respawn 就绪下一 turn | TC: 起长 turn，interrupt，查 turn interrupted，再起 turn 成功 |
| AC2.6 | 计量：turn 完成后 `turns.input_tokens/output_tokens/cost_micros/model` 有非零值（来自 tokenUsage/updated） | TC: 查 turns 行 token 字段 |
| AC2.7 | model_gateway 真实代理：配置 upstream 后 gateway 转发请求并计数；无配置回退 mock 仍能跑通闭环 | TC: 双模式各跑一 turn |

## 4. 关键约束

- **不改 codex-rs 内核**：仅扩展 `nexus-control` crate（M0/M1 既定边界）。
- **同步 stdio 适配**：app-server 是行阻塞 stdio JSON-RPC，单进程天然序列化——M2 接受全局单 turn 串行（单租户 MVP），驱动线程独占。
- **Simplicity First**：interrupt MVP 用 kill+respawn+resume（M0 已验证），不引入 select-based 并发 stdin/stdout（复杂度不匹配 MVP）。
- **真实模型可选**：无 API key 时 mock gateway 跑通闭环；有 key 时 passthrough 真实模型。验收闭环不依赖真实模型。
