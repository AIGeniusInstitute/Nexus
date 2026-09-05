# Nexus M4 — 产物与计量 · PRD

> 里程碑 M4（Artifacts & Metering）· 2026-09-06 · 分支 `feat/nexus-m4`

## 1. 背景

M0–M3 已交付：骨架、身份租户、执行闭环、审批与策略。但当前 turn 执行依赖 **SIMULATE 模式**（合成审批与 item），未接真实模型；token 用量恒为 0（mock gateway 不返真实 tokenUsage）；策略中心 `generate_rules()` 仅生成 Starlark 文本，未与 app-server 运行时热加载闭环；多租户并发上限未接线（单 driver 串行，无配额门控）。

M4 补齐"真实产物 + 真实计量"——让平台 turn 产出真实内容、采集真实 token 用量、按租户聚合计量、策略动态下发。

## 2. 目标

| # | 目标 | 价值 |
|---|------|------|
| G1 | 真实模型执行 | turn 经 model_gateway 代理真实 upstream，产出真实 item/content |
| G2 | Token 用量计量 | 真实采集 input/output/cached tokens，落 turns，turn 完成聚合 |
| G3 | 用量聚合 API | per-tenant/per-user/per-day 聚合，支撑计费弹性 |
| G4 | execpolicy 动态下发 | generate_rules() → app-server rules/ 热加载 + 审批 amendment 回写 |
| G5 | 多租户并发上限 | tenant 配额 + 运行时并发计数器，超限 429 |
| G6 | 产物持久化 | aggregatedOutput 持久化到 items，可检索 |
| G7 | Web 用量看板 | 前端 per-day token 柱图（CSS SVG，无外部库） |

## 3. MVP 范围

| 范围 | 说明 |
|------|------|
| ✅ M4 | G1–G7 全部 |
| ⏭ M5+ | 真实云集群多 Pod 计量分布式聚合、计费账单生成、SLA 报表 |

## 4. 验收标准

### AC4.1 真实模型执行
- model_gateway 接 `NEXUS_UPSTREAM_MODEL_URL` 真实 upstream，`POST /v1/chat/completions` 转发，透传 stream。
- 非 SIMULATE 模式下，turn start → driver 经 gateway 调真实模型 → 产出真实 item（item/started + item/completed with aggregatedOutput）。
- SIMULATE 模式保留（无 upstream 时回退），零回归。

### AC4.2 Token 用量计量
- driver 从 `thread/tokenUsage/updated` 事件提取 input_tokens/output_tokens/cached_tokens，落 `turns` 表 usage 列。
- 非 SIMULATE 下 usage > 0（来自真实 upstream）；SIMULATE 下 usage = 0（合成）。
- 迁移：`turns` 加 `input_tokens BIGINT DEFAULT 0` / `output_tokens BIGINT DEFAULT 0` / `cached_tokens BIGINT DEFAULT 0` / `model TEXT` / `cost_usd NUMERIC(12,6) DEFAULT 0`。

### AC4.3 用量聚合 API
- `GET /v1/usage?days=7` 返回当前租户近 N 天聚合：`[{date, total_input_tokens, total_output_tokens, total_cached_tokens, total_turns, total_cost_usd}]`。
- `GET /v1/usage/users/{uid}?days=7`（admin）返回 per-user 聚合。
- JWT 鉴权 + 租户隔离（仅本租户；admin 可查任意用户）。

### AC4.4 execpolicy 动态下发
- `policy.rs generate_rules()` 产出 Starlark，写入 `<CODEX_HOME>/rules/{tenant_id}.rules`。
- 审批 resolve 时，若决策含 `AcceptWithExecpolicyAmendment`，将 amendment 合并入该租户 policy + 重新生成 rules 文件。
- driver spawn 时按 tenant 加载对应 rules 文件（app-server 自动加载 `<CODEX_HOME>/rules/`，M0 T0-4 已验证机制）。

### AC4.5 多租户并发上限
- `tenants` 表加 `max_concurrent_turns INT DEFAULT 1`。
- turn_start 检查：该租户 running turns 数 >= max_concurrent_turns → `429 Too Many Turns`。
- turn 终态（completed/failed/interrupted）时释放计数。
- 单 driver 串行不变（M5+ 池化），但门控在 HTTP 层提前拒绝超限请求。

### AC4.6 产物持久化
- item/completed 的 `aggregatedOutput` 落 `items.content_ref`（大对象经 `data/trace-io/` 落盘 + ref 引用，M0 已有机制沿用）。
- `GET /v1/threads/{id}/items?since=N` 返回含产物的 item 列表。

### AC4.7 Web 用量看板
- 前端新增"用量"页：CSS/SVG 柱图展示近 7 天 input/output tokens + 翻牌总量 + turn 数。
- nav 三栏：会话 / 审批 / 用量。

## 5. 非目标

- 真实计费账单生成（M5+）。
- 多 Pod 分布式用量聚合（M5+，当前单进程 DB 聚合）。
- 模型路由策略（多模型负载均衡，M5+）。
- 产物全文检索（FTS/pgvector，已在 DeepThink 主仓实现，Nexus 后续接）。

## 6. 风险

| 风险 | 缓解 |
|------|------|
| 真实 upstream 不可用（无 API key） | SIMULATE 回退 + 文档说明；e2e 用 SIMULATE 验证骨架，真实联调标注"需 upstream" |
| gateway stream 转发阻塞 driver | driver 专用线程已串行化 stdio，gateway 用 TcpStream 非阻塞桥接（M2 模式） |
| execpolicy amendment 与 app-server 加载时机 | rules 文件原子写（tmp + rename），app-server 每-turn 读 rules 目录（已验证） |
| 并发计数器与单 driver 串行冲突 | 计数器在 HTTP 层（turn_start 前置门控），不进 driver；单 driver 串行保证不超 1 真实并发，门控防积压 |
