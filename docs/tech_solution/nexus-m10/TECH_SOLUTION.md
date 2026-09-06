# Nexus M10 技术方案 — 审计日志 WORM + 审计查询 API

## 数据流

```
关键操作（http_server.rs）           audit_logs 表（WORM）         查询 API
  auth.login ─────┐
  turn.start ─────┤                  ┌─ trigger prevent_audit_modification
  turn.complete ──┤── audit_log() ──>│   BEFORE UPDATE/DELETE → RAISE
  approval.* ─────┤   (INSERT only)  │
  policy.* ───────┘                  └─ audit_logs (只追加)
                                                        │
                                          GET /v1/audit/logs?action=&since=&limit=
                                          GET /v1/audit/logs/{id}
```

## WORM 保证方案

PG trigger 阻止修改：
```sql
CREATE OR REPLACE FUNCTION prevent_audit_modification()
RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'audit_logs is append-only (WORM): modification forbidden';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER audit_worm BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION prevent_audit_modification();
```

应用层：`audit_log()` 只 INSERT，从不 UPDATE/DELETE。

## 表结构

```sql
CREATE TABLE audit_logs (
    id BIGSERIAL PRIMARY KEY,
    tenant_id BIGINT NOT NULL,
    actor_uid BIGINT,          -- 操作者（系统操作为 NULL）
    action TEXT NOT NULL,      -- auth.login / turn.start / approval.resolved ...
    target_type TEXT,           -- thread / turn / approval / policy
    target_id TEXT,             -- 对象 id
    detail JSONB,               -- 决策/参数等
    trace_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

## 修改点

1. `migrations/20260906000006_m10_audit.sql`（新）：audit_logs + trigger + 索引
2. `db.rs`：加 M10_MIGRATION_SQL include_str + raw_sql
3. `audit.rs`（新模块）：`audit_log()` + `AuditLogRow`(FromRow) + `list_audit_logs()` + `get_audit_log()`
4. `http_server.rs`：埋点（login/turn/approval/policy 关键路径）+ GET /v1/audit/logs + /v1/audit/logs/{id} 路由
5. `main.rs`：标题 + lib.rs 导出 audit 模块

**不改 codex 内核**（所有改动在 nexus-control crate 内）。**不动 approval_audit**（M3 已有，Surgical）。

## 简化决策

1. **WORM 用 PG trigger**：简单可靠，应用层无需特殊处理，DB 层强制不可变。
2. **audit_logs 通用表 vs approval_audit 专用**：approval_audit 保留（M3 已验证），audit_logs 统一所有审计供查询。审批埋点双写（approval_audit 专用 + audit_logs 通用）——但为避免冗余，审批埋点只写 audit_logs（approval_audit 表保留不删，不再新写）。
   - 实际决策：approval_resolve 仍写 approval_audit（M3 行为不动，Surgical），同时写 audit_logs（新增通用审计）。
3. **trace_id 贯穿**：turn_start 生成 trace_id，后续 turn/approval 埋点带同 trace_id（可选，Simplicity 先埋 trace_id 字段不强求全链路）。

## 测试

- 单测：`prevent_audit_modification`（模拟 INSERT 后 UPDATE 报错）——PG trigger 测试需 DB，放 e2e
- e2e：login → turn → approval → resolve → 查 audit_logs 各事件落库 + WORM（UPDATE 报错）+ API 查询
- 零回归：M9 真实模型 turn + 审批不退化
