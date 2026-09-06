# Nexus M11 任务状态 — 全链路 tracing + timeline 聚合查询

## 里程碑
M11 = per-turn trace_id 贯穿 + timeline 聚合 + trace 查询 API。
分支：`feat/nexus-m11`，base 含 M10 merge `e55b57f`。

## 任务清单

| 任务 | 状态 | 说明 |
|---|---|---|
| T11-1 migration turns.trace_id | ✅ | ALTER turns ADD trace_id UUID DEFAULT gen_random_uuid() + idx_turns_trace + idx_audit_trace |
| T11-2 turn_start 读 trace_id + 贯穿埋点 | ✅ | INSERT...RETURNING id,trace_id；turn.complete/approval.resolve/turn.interrupt 传 trace_id（approval/interrupt 查 turns.trace_id） |
| T11-3 timeline.rs 模块 | ✅ | `TimelineEntry`(Serialize) + `thread_timeline()`(3 query+Rust 合并排序) + `trace_lookup()`(turn+audit by trace_id) |
| T11-4 路由 | ✅ | GET /v1/threads/{id}/timeline + GET /v1/traces/{trace_id}（admin 跨租户/非 admin 本租户）|
| T11-5 main.rs 标题 | ✅ | "Nexus M11: serve" |

## 关键决策

1. **trace_id 仅 http_server 层**：不经 driver/runtime/stdio_client（Simplicity First；M10 audit 埋点已在 http_server，trace_id 在此层贯穿即可）
2. **timeline 用 Rust 合并 3 query**：turns/items/approval_tickets 行异构（content_ref vs command vs status），SQL UNION 需 cast 脆弱；Rust 端合并 + sort_by(ts) 清晰
3. **timeline 不含 audit_logs**：audit 有 M10 独立查询 API + M11 trace API 关联，职责分离（timeline=结构回放，trace=审计关联）
4. **trace_id 用 PG default**：gen_random_uuid()，应用不生成
5. **audit_logs.trace_id 是 TEXT**（M10 建），存 UUID 字符串；trace_lookup bind trace_id.to_string()（非 Uuid，避免类型不匹配）

## 坑

1. **`input_tokens`/`output_tokens` 是 INT4 非 INT8**：trace_lookup turn query 初版声明 `Option<i64>`→ColumnDecode "Rust i64 (INT8) not compatible with INT4"。改为 i32（列 integer NOT NULL default 0）。
2. **audit_logs.trace_id 是 TEXT**：trace_lookup audit query bind Uuid→类型错（"trace: audit" 500）。改 bind `trace_id.to_string()`（turn.trace_id 是 UUID bind Uuid OK；audit_logs.trace_id TEXT bind String）。
3. **trace turn query 子查询**：初版 `thread_id IN (SELECT id FROM threads WHERE tenant_id=$2)` 子查询内 $2 无 cast 类型推断失败→改 `EXISTS(SELECT 1 FROM threads h WHERE h.id=t.thread_id AND h.tenant_id=$2)`（M10 audit list 同 NULL-or-equal 模式验证过）。

## 验证

- cargo check：0 error 0 warning
- cargo test：27/27（M10 26 + timeline 1）零回归
- e2e（PG nexus-pg-m4:5434 + POOL=2 + SIMULATE_APPROVAL=1）AC11.1-11.5 全过：
  - AC11.1 turns.trace_id 自动生成 UUID（45d8e6e1.../5ed6ea65...）
  - AC11.2 audit_logs.trace_id 非空（turn.complete/approval.resolve 同 trace_id）
  - AC11.3 GET /v1/threads/{id}/timeline 返回 turns+items+approvals 合并时间线（按 ts 升序，count=3：turn/approval/item）
  - AC11.4 GET /v1/traces/{trace_id} 返回 turn(id=35,completed,mock,tokens 10/20)+2 audit(approval.resolve+turn.complete)
  - AC11.5 零回归：SIMULATE turn completed + approval resolved + 计量落库（mock 10/20）+ M10 audit WORM 路径不退化
- 不改 codex 内核（全部 nexus-control crate）；不动 M3/M10 表结构（仅 turns 加列）
