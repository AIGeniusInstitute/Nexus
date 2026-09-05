# Nexus M4 — 产物与计量 · 任务执行状态

> 分支 `feat/nexus-m4` · 里程碑 M4（Artifacts & Metering）· 2026-09-06

## 里程碑目标

补齐"真实产物 + 真实计量"——token 用量落 usage_records + cost 推导 + 用量聚合 API + execpolicy 动态下发(prefix_rule) + 多租户并发上限 + Web 用量看板。

## 任务清单

| 任务 | 主题 | 状态 |
|------|------|------|
| T4-1 | migration: tenants +max_concurrent_turns / approval_tickets +policy_decision / model_pricing | ✅ |
| T4-2 | db.rs 挂载 M4 migration | ✅ |
| T4-3 | metering.rs(新): record_usage + compute_cost + daily_usage 聚合 | ✅ |
| T4-4 | policy.rs: generate_rules 改 prefix_rule 语法 + write_tenant_rules | ✅ |
| T4-5 | runtime.rs: SIMULATE 注入合成 tokenUsage(model=nexus-gateway-mock) | ✅ |
| T4-6 | http_server.rs: 并发门控(429) + usage 落库 + evaluate 标注 + /v1/usage 路由 | ✅ |
| T4-7 | main.rs: run_serve 迁移后写 tenant rules | ✅ |
| T4-8 | web: UsagePage + getUsage + 审批卡 policy/risk 标签 | ✅ |
| T4-9 | 集成验收: e2e usage + cost + 429 + rules | ✅ |

## 验证结果

| 项 | 结果 |
|----|------|
| cargo check | 0 error 0 warning |
| cargo test | 19/19 通过(+2 metering 单测) |
| web tsc --noEmit | exit 0 |
| web vite build | 29 modules 成功 |
| e2e usage 落库 | usage_records: model=nexus-gateway-mock in=10 out=20 ✅ |
| e2e cost 推导 | gpt-4o: 1M in @2.50 + 0.5M out @10.00 = 7,500,000 micros($7.50) ✅ |
| e2e turns.model | nexus-gateway-mock ✅ |
| e2e 聚合 API | GET /v1/usage?days=7 返回 date+in+out+turns+cost ✅ |
| e2e 策略推荐 | ticket policy_decision=deny risk_level=high ✅ |
| e2e 并发 429 | 第二并发 turn → 429 too_many_concurrent_turns: limit=1 ✅ |
| e2e rules 文件 | /tmp/nexus-m4-home/rules/tenant-1.rules prefix_rule 语法 ✅ |

## 关键修复记录

- **execpolicy 语法统一**: M3 `generate_rules()` 用 `forbid()/allow()` 未经验证且与 app-server parser 不符 → 改 M0 验证的 `prefix_rule(pattern=[...], decision=...)` 语法,glob pattern 翻译为 argv 前缀 token 列表。
- **SUM(bigint)→NUMERIC**: PG 的 `SUM(bigint)` 返回 NUMERIC(非 INT8),sqlx 无法 decode 为 i64 → SUM 结果 `::bigint` 转型。
- **make_interval 绑定**: sqlx 对 `make_interval(days => $2::int)` 命名参数类型推断失败 → 改 Rust 算 cutoff 时间戳传入。
- **NaiveDate/DateTime**: chrono feature 已启用,NaiveDate 从 `::date` 解码正常。

## 退出条件

✅ 全部任务完成,e2e 全过,无回归。M4 交付。
