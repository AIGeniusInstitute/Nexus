# Nexus M10 任务状态 — 审计日志 WORM + 审计查询 API

## 里程碑
M10 = 审计日志 WORM（Write-Once-Read-Many）+ 通用审计查询 API。
分支：`feat/nexus-m10`，base 含 M9 merge `8299ffd`。

## 任务清单

| 任务 | 状态 | 说明 |
|---|---|---|
| T10-1 migration audit_logs WORM | ✅ | ALTER ADD COLUMN target_type/target_id/trace_id + WORM trigger `prevent_audit_modification` BEFORE UPDATE/DELETE + 2 索引 |
| T10-2 audit.rs 模块 | ✅ | `audit_log()` INSERT-only + `AuditLogRow`(FromRow+Serialize) + `list_audit_logs()`/`get_audit_log()` NULL-or-equal 单查询全组合 |
| T10-3 http_server 埋点 | ✅ | login→auth.login / turn 完成→turn.complete / turn_interrupt→turn.interrupt / approval_resolve→approval.resolve |
| T10-4 审计查询路由 | ✅ | GET /v1/audit/logs?action=&since=&limit= + GET /v1/audit/logs/{id}（admin 全租户 / 非 admin 本租户）|
| T10-5 main.rs 标题 | ✅ | "Nexus M10: serve" |

## 关键决策

1. **适配现有 audit_logs 表**：发现 audit_logs 表已存在（早期里程碑建，列 actor_user_id(FK users)/resource/detail_json(NOT NULL default '{}')）。`CREATE TABLE IF NOT EXISTS` 会跳过→改为 `ALTER TABLE ADD COLUMN IF NOT EXISTS` 补 target_type/target_id/trace_id，保留现有列。Surgical：不重建表、不动现有数据/WORM trigger。
2. **detail_json NOT NULL**：audit_log() 内 `detail.cloned().unwrap_or(json!({}))` 保证 None→'{}' 不违反 NOT NULL。
3. **WORM 用 PG trigger**：BEFORE UPDATE OR DELETE RAISE EXCEPTION，DB 层强制不可变，应用层只 INSERT（best-effort，失败仅 log 不阻塞业务路径）。
4. **NULL-or-equal 单查询**：`($2::bigint IS NULL OR tenant_id=$2)` + `($3::text IS NULL OR action=$3)`，一个 query 覆盖 admin/非 admin × action 过滤全组合，避免 4 分支。
5. **审计 best-effort 不阻塞**：audit_log() 返回 ()，内部 catch error→tracing::error，绝不 propagate 给业务调用（审计失败不能让 login/turn 失败）。
6. **admin 全租户**：`*:*` 权限→tenant_filter=None（跨租户），非 admin→Some(tid) 本租户隔离。

## 坑

1. **audit_logs 表已存在旧 schema**（最关键）：初版 migration `CREATE TABLE IF NOT EXISTS` 因表已存在跳过→列 actor_uid 不存在→INSERT 报 42703 `column "actor_uid" does not exist`。修复：migration 改 ALTER ADD COLUMN，audit.rs 列名 actor_user_id/detail_json 适配现有表。
2. **admin_email 无 env**：main.rs `admin_email` 字段只有 `#[arg(long)]` 无 env，NEXUS_ADMIN_EMAIL 不生效→用 CLI flag `--admin-email`。
3. **PG 密码**：nexus-pg-m4 容器 POSTGRES_USER/PASSWORD=nexus/nexus（非 deepthink/deepthink123）。

## 验证

- cargo check：0 error 0 warning
- cargo test：26/26（M9 25 + audit 1）零回归
- e2e（PG nexus-pg-m4:5434 + NEXUS_POOL_SIZE=2 + NEXUS_SIMULATE_APPROVAL=1）7 AC 全过：
  - AC10.1 login→auth.login 落库（actor_user_id=4, target user/4）
  - AC10.2 SIMULATE turn→approval 触发（approval 27 pending）
  - AC10.3 resolve approve→approval approved + turn 33 completed
  - AC10.4 audit_logs 3 条：auth.login / approval.resolve(approved) / turn.complete(completed)
  - AC10.5 GET /v1/audit/logs/{id} 单条查询
  - AC10.6 WORM UPDATE→ERROR "append-only (WORM): modification forbidden"
  - AC10.7 WORM DELETE→ERROR 同
- 零回归：SIMULATE turn completed + approval resolved + driver pool 正常

## 不改 codex 内核
所有改动在 codex-rs/nexus-control/ crate 内。approval_audit（M3 专用表）不动，audit_logs（通用查询表）双轨并存。
