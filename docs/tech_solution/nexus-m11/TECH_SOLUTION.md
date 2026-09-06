# Nexus M11 技术方案 — 全链路 tracing + timeline 聚合查询

## 数据流

```
turn_start
  ├─ INSERT turns (...) RETURNING id, trace_id   ← PG default gen_random_uuid()
  ├─ ... driver 跑 turn ...
  └─ audit_log(..., trace_id=Some(&trace_id))     ← 贯穿 audit_logs.trace_id

GET /v1/threads/{id}/timeline:
  turns(items) ∪ items ∪ approval_tickets  by thread_id → 按 ts 排序

GET /v1/traces/{trace_id}:
  audit_logs WHERE trace_id=$1  +  turns WHERE trace_id=$1
```

## 表结构变更
```sql
ALTER TABLE turns ADD COLUMN IF NOT EXISTS trace_id UUID NOT NULL DEFAULT gen_random_uuid();
CREATE INDEX IF NOT EXISTS idx_turns_trace ON turns (trace_id);
CREATE INDEX IF NOT EXISTS idx_audit_trace ON audit_logs (trace_id) WHERE trace_id IS NOT NULL;
```
audit_logs.trace_id 已由 M10 加。

## 修改点
1. `migrations/20260906000007_m11_tracing.sql`（新）：turns.trace_id + 2 索引
2. `db.rs`：加 M11_MIGRATION_SQL include_str + raw_sql
3. `timeline.rs`（新模块）：`TimelineEntry`(Serialize) + `thread_timeline()` + `trace_lookup()`；thread_timeline 用 3 个独立 query（turns/items/approvals）+ Rust 端合并排序（避免 PG UNION 类型不齐）
4. `http_server.rs`：turn_start INSERT...RETURNING trace_id；4 处 audit 埋点传 trace_id（turn/approval/interrupt，login 无 turn 不传）；2 路由 timeline/trace
5. `main.rs`：标题 "Nexus M11: serve"
6. `lib.rs`：pub mod timeline

## 简化决策
1. trace_id 仅 http_server 层，不经 driver/runtime（Simplicity；M10 audit 埋点已在 http_server）
2. timeline 用 Rust 端合并 3 query（非 SQL UNION——items.content_ref/approvals.command 类型异构，UNION 需 cast，Rust 合并更清晰）
3. timeline 不含 audit_logs（audit 有 M10 独立 API + trace API 关联，职责分离）
4. trace_id 用 PG default gen_random_uuid()，无需应用生成

## 测试
- 单测：timeline.rs 合并排序逻辑（构造 mock 行验证排序）
- e2e：SIMULATE turn → timeline 返回 turn+items+approval → trace API 返回 audit_logs by trace_id
- 零回归：M10 audit WORM + M9 真实模型路径不退化
