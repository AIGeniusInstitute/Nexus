# Nexus M4 — 产物与计量 · 技术方案

> 里程碑 M4（Artifacts & Metering）· 2026-09-06 · 分支 `feat/nexus-m4`

## 1. 现状与缺口（探查实证）

| 领域 | 现状 | 缺口 |
|------|------|------|
| model_gateway | upstream 透传已实现（raw TCP），mock 返固定 2 token；gateway 在真实 turn 中确实被命中 | 无 per-tenant 计量，无 token 提取 |
| token 用量 | `thread/tokenUsage/updated` 已捕获到 `turns.input_tokens/output_tokens`；`cost_micros=0` 硬编码；`model` 列从不写；`usage_records` 表已建但**从不写入** | cost 推导、model 捕获、usage_records 写入、聚合 |
| execpolicy/rules | M0 `write_default_rules` 用 `prefix_rule(...)` 语法（**T0-4 已验证** rm→Forbidden/ls→Allow）；M3 `generate_rules()` 用 `forbid()/allow()`（**未验证、与 parser 不符**）；serve 模式**从不写 .rules 文件**；`evaluate()` 从未在 handler 调用 | 语法统一为 `prefix_rule`、serve 写 per-tenant rules、evaluate 接线为审批推荐 |
| 并发 | 全局 mutex 串行所有 turn；无租户门控；`quotas`/`budgets` 表闲置 | tenant `max_concurrent_turns` + turn_start 前置 429 门控 |

## 2. 设计决策

### D1 — usage_records 写入 + cost 推导 + model 捕获
**Simplicity First**：`usage_records` 表已就绪（tenant_id/user_id/thread_id/turn_id/model/input_tokens/output_tokens/cost_micros/recorded_at）。在 http_server.rs turn_start drain 收到 `ThreadTokenUsageUpdated` 时，**除**更新 turns（现有）外，**额外** INSERT usage_records。新增 `model_pricing` 表（model PK, input_rate_per_mtok NUMERIC, output_rate_per_mtok NUMERIC, currency TEXT）+ seed `nexus-gateway-mock`/`gpt-4o`/`claude-sonnet` 三行。`compute_cost(model, input, output) → cost_micros`：`((input/1e6)*input_rate + (output/1e6)*output_rate) * 1e6`，未知 model → 0。model 名从 tokenUsage 事件取（若无则 `"nexus-gateway"`）。

### D2 — 用量聚合 API
`GET /v1/usage?days=7` → `SELECT date_trunc('day', recorded_at) d, SUM(input_tokens), SUM(output_tokens), SUM(cost_micros), COUNT(*) FROM usage_records WHERE tenant_id=$1 AND recorded_at >= NOW()-interval 'N days' GROUP BY d ORDER BY d`。`GET /v1/usage/users/{uid}?days=7`（admin 鉴权）加 `user_id=$uid`。JWT 租户隔离。

### D3 — execpolicy 语法统一为 prefix_rule（**关键修复**）
M3 `generate_rules()` 的 `forbid()/allow()` 语法**未经验证且与 M0 验证过的 `prefix_rule(...)` 不符**。**修复**：重写 `generate_rules()` 输出 M0 验证语法：
```python
prefix_rule(pattern=["rm -rf*"], decision="forbidden", justification="deny: rm -rf")
prefix_rule(pattern=["ls*"], decision="allowed", justification="allow: ls")
```
新增 `write_tenant_rules(pool, tenant_id, codex_home)`：调 `generate_rules(pool, tenant_id)` → 原子写（tmp+rename）`{codex_home}/rules/tenant-{tenant_id}.rules`。main.rs run_serve 迁移后调用（默认 tenant_id=1）。app-server 每-turn 读 rules 目录（M0 已验证机制），无需重启。

### D4 — evaluate() 接线为审批推荐（非侵入）
**Surgical Changes**：不改 M3 的 surface-always 行为。M4 migration 给 `approval_tickets` 加 `policy_decision TEXT`/`risk_level TEXT` 两列。turn_start 收到 approval/requested 时，调 `policy::evaluate(pool, tenant_id, role, "command", &command)` → 将结果（Allow/Prompt/Deny）+ `risk_of(command)` 写入 ticket 的 policy_decision/risk_level。Web 审批抽屉展示"策略推荐: deny(高风险)"标签。**人仍最终决策**，evaluate 仅作推荐标注。

### D5 — 多租户并发门控
tenants 表加 `max_concurrent_turns INT NOT NULL DEFAULT 1`。turn_start **在锁 mutex 前**：`SELECT COUNT(*) FROM turns t JOIN threads th ON th.id=t.thread_id WHERE th.tenant_id=$1 AND t.status='running'`，若 `>= max_concurrent_turns` → `429 {"error":"too_many_concurrent_turns","limit":N}`。因全局 mutex 串行，真实并发恒 ≤1，门控语义=防同租户请求积压。无并发计数器同步问题（纯 DB count）。

### D6 — SIMULATE 注入 tokenUsage
SIMULATE 模式（无真实 upstream）下，driver 在 turn 完成前合成一条 `ThreadTokenUsageUpdated`（input=10/output=20/model="nexus-gateway-mock"），使 usage_records e2e 可验证 cost 推导 + 聚合，无需真实模型。镜像 M3 SIMULATE 策略。

### D7 — Web 用量看板
`api.ts` 加 `getUsage(days)` → `Usage[]`。`App.tsx` 新增 `UsagePage`：CSS 柱图（近 7 天 input/output tokens，纯 div+height%）+ 翻牌总量 + turn 数。nav 三栏：会话/审批/用量。

## 3. 任务分解

| 任务 | 文件 | 内容 |
|------|------|------|
| T4-1 | `migrations/20260906000004_m4_metering.sql` | tenants +max_concurrent_turns；approval_tickets +policy_decision/risk_level；CREATE model_pricing + seed 3 行 |
| T4-2 | `db.rs` | 挂载 M4_MIGRATION_SQL |
| T4-3 | `metering.rs`（新） | `compute_cost(model,input,output)→i64`；`record_usage(pool,tenant,uid,thread,turn,model,in,out)` INSERT usage_records+compute_cost；3 单测 |
| T4-4 | `policy.rs` | 重写 `generate_rules()` 为 prefix_rule 语法；新增 `write_tenant_rules(pool,tid,codex_home)`；修单测 |
| T4-5 | `runtime.rs` | SIMULATE 分支注入合成 tokenUsage；real 分支透传（现有）；Usage 加 model 捕获 |
| T4-6 | `http_server.rs` | turn_start 前置 tenant 并发门控(429)；drain 中 ThreadTokenUsageUpdated→record_usage；approval/requested→evaluate 写 policy_decision/risk_level；新增 `/v1/usage`、`/v1/usage/users/{uid}` |
| T4-7 | `main.rs` | run_serve 迁移后调 `write_tenant_rules(pool, 1, codex_home)` |
| T4-8 | `web/api.ts`+`App.tsx` | UsagePage + getUsage；审批卡显示 policy_decision/risk 标签 |
| T4-9 | 集成验收 | e2e：usage 聚合 + cost 推导 + 并发 429 + rules 文件生成 |

## 4. 关键代码骨架

### metering.rs
```rust
pub async fn record_usage(pool, tenant_id, user_id, thread_id, turn_id,
    model: &str, input: i64, output: i64) -> Result<i64> {
    let cost = compute_cost(model, input, output);
    sqlx::query(
      "INSERT INTO usage_records(tenant_id,user_id,thread_id,turn_id,model,
       input_tokens,output_tokens,cost_micros,recorded_at)
       VALUES($1,$2,$3,$4,$5,$6,$7,$8,NOW())")
      .bind(tenant_id).bind(user_id).bind(thread_id).bind(turn_id)
      .bind(model).bind(input).bind(output).bind(cost).execute(pool).await?;
    Ok(cost)
}
pub fn compute_cost(model: &str, input: i64, output: i64) -> i64 {
    // 查 model_pricing；未知→0。input_rate_per_mtok * input/1e6 * 1e6 → micros
    ... // 简化：硬编码 rate map 或查 DB
}
```

### generate_rules（修复后）
```rust
pub async fn generate_rules(pool, tenant_id) -> Result<String> {
    let rows = sqlx::query_as::<_,PolicyRow>(
      "SELECT pattern, decision FROM policies WHERE tenant_id=$1 ORDER BY priority DESC")
      .bind(tenant_id).fetch_all(pool).await?;
    let mut s = String::new();
    for r in rows {
        let (dec, just) = match r.decision.as_str() {
            "deny" => ("forbidden", "deny"),
            "allow" => ("allowed", "allow"),
            _ => continue, // prompt 不生成 execpolicy 规则（走 HITL）
        };
        s += &format!("prefix_rule(pattern=[\"{}\"], decision=\"{}\", justification=\"{}: {}\")\n",
            r.pattern, dec, just, r.pattern);
    }
    Ok(s)
}
```

### turn_start 前置门控
```rust
async fn turn_start(State(st), Path(tid), Json(body)) -> impl IntoResponse {
    let claims = ...;
    // M4: tenant 并发门控（锁 mutex 前）
    let running: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM turns t JOIN threads th ON th.id=t.thread_id
       WHERE th.tenant_id=$1 AND t.status='running'").bind(claims.tid).fetch_one(&st.pool).await?;
    let limit: i32 = sqlx::query_scalar("SELECT max_concurrent_turns FROM tenants WHERE id=$1")
      .bind(claims.tid).fetch_one(&st.pool).await?;
    if running >= limit as i64 {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error":"too_many_concurrent_turns","limit":limit}))).into_response();
    }
    // ... 既有 M3 逻辑（lock runtime_events, send RunTurn, drain）
}
```

## 5. 验证策略

| 验证 | 方法 |
|------|------|
| cargo check/test | 0 error 0 warning；单测含 compute_cost + generate_rules(prefix_rule) + evaluate |
| e2e usage | SIMULATE turn → usage_records 有行（input=10/output=20）+ cost>0（nexus-gateway-mock rate） |
| e2e 聚合 | GET /v1/usage?days=7 返回当日聚合行 |
| e2e 并发 | turn1 运行中 → turn2（同租户）→ 429 |
| e2e rules | serve 启动后 `cat /tmp/nexus-m4-home/rules/tenant-1.rules` 含 prefix_rule |
| web | tsc + vite build；UsagePage 渲染柱图 |

## 6. 风险与回退

| 风险 | 缓解 |
|------|------|
| app-server 不认 prefix_rule 多 pattern 语法 | M0 T0-4 已验证单 pattern；多 pattern 改单条多行（每 pattern 一条 prefix_rule） |
| 真实 tokenUsage 事件无 model 字段 | Usage.model 默认 "nexus-gateway"；SIMULATE 注入含 model |
| 并发门控误拒（mutex 串行下 running 恒 0/1） | 默认 limit=1：running=1 时拒第二个，语义正确（防积压）；用户可调 limit |
| cost 推导精度 | micros 整数运算，未知 model→0（不误计费） |

## 7. 自审清单

- [x] 不改 codex-rs 内核（全在 nexus-control/）
- [x] 不改 M3 surface-always 审批行为（evaluate 仅标注推荐）
- [x] 向后兼容：无 upstream 时 SIMULATE 回退；无 max_concurrent_turns 时 default 1
- [x] Simplicity First：usage_records 表已存在直接用；并发门控纯 DB count 无计数器；cost 硬编码 rate map
- [x] Surgical Changes：只动必要文件；M3 既有逻辑不重构
- [x] 退出条件：cargo check/test + e2e 5 例 + web tsc/build

## 8. 方案自确认

✅ 方案 OK。缺口实证清晰、设计最小化、向后兼容、风险可控。开工 T4-1 → T4-9。
