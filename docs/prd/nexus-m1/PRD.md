# Nexus M1 — 需求 PRD

> 需求编号：nexus-m1 · 阶段：② 单租户 MVP（M1 身份 + 骨架）
> 工作分支：`feat/nexus-m1` · 日期：2026-09-06
> 前置：M0 PoC 已交付（三大假设验证通过，merge 6e860ce）

## 1. 背景与目标

M0 PoC 验证三大假设（H1 长会话恢复 / H2 execpolicy / H3 三层沙箱）通过。M1 构建单租户 MVP 的**身份与接入骨架**：身份租户模型 + API Gateway + 认证 + WebSocket 网关 + Web 门户 + CLI + Postgres 主库。为 M2 执行闭环（Runtime 池/任务编排/会话落库）铺路。

**M1 非目标**：多租户 RLS（M5）、Runtime 池调度（M2）、审批 HITL（M3）、计费（M4）。仅搭身份 + 接入骨架。

## 2. 功能点清单

| FP | 功能点 | 对应任务 | 优先级 |
|---|---|---|---|
| FP1 | 身份租户模型（单租户）：Tenant/OrgUnit/User/Role/Membership 表 + RBAC 授权引擎 | T1-1 | P0 |
| FP2 | API Gateway：REST 路由 + Idempotency-Key 幂等 + 租户/用户/IP 三级限流 | T1-2 | P0 |
| FP3 | 认证中间件：JWT 签发 + 校验（OIDC trait 抽象，Keycloak 对接留 M5） | T1-3 | P0 |
| FP4 | WebSocket 网关：权限驱动订阅，权限变更立即断连 | T1-4 | P0 |
| FP5 | Web 门户骨架：会话列表 + 任务时间线 + WS 实时增量 | T1-5 | P0 |
| FP6 | CLI 登录层：企业登录 + 远端 thread 操作 | T1-6 | P1 |
| FP7 | Postgres 主库 schema：26 实体 DDL + 索引（暂不含 RLS） | T1-7 | P0 |

## 3. 功能点详述与验收标准

### FP1 身份租户模型（T1-1）

**需求**：建 5 张身份表（Tenant/OrgUnit/User/Role/Membership）+ RBAC 授权引擎（Role→Permission，User↔Role 经 Membership）。
- 预置单租户 `default` + `admin` 角色（全权限）
- `check_permission(user, resource, action) -> allow/deny` 授权函数

**验收标准**：
- AC1.1 5 表 DDL 建成，外键关系正确
- AC1.2 `check_permission` 对 admin 返回 allow，对无角色用户返回 deny
- AC1.3 预置 `default` 租户 + `admin` 角色可查询

### FP2 API Gateway 路由+限流+幂等（T1-2）

**需求**：axum REST 路由 + tower 中间件链：
- `Idempotency-Key` 头：同 key 重复请求返回首次缓存结果
- 三级限流：租户 / 用户 / IP，超限返回 429

**验收标准**：
- AC2.1 路由命中正确 handler（404 for unknown）
- AC2.2 重复 `Idempotency-Key` 返回首次结果，不重复执行
- AC2.3 超限流返回 429 + Retry-After

### FP3 认证中间件（T1-3，OIDC 简化）

**需求**：用户名 + 密码（bcrypt）登录，签发 JWT；JWT 校验中间件保护受路由。
- OIDC 抽象为 `AuthProvider` trait，本地密码实现 + Keycloak 实现留 M5

> **审查标注**：roadmap T1-3 原 OIDC Keycloak，M1 简化为本地 JWT（Simplicity First——单租户 MVP 不需 OIDC，企业 Keycloak 对接是 M5 多租户需求）。

**验收标准**：
- AC3.1 正确密码登录签发 JWT
- AC3.2 错误密码返回 401
- AC3.3 受保护路由无 JWT / 错 JWT 返回 401

### FP4 WebSocket 网关（T1-4）

**需求**：axum WS 升级，按用户权限订阅 thread 事件流；权限撤销立即断连。

**验收标准**：
- AC4.1 订阅需对 thread 有读权限，否则拒绝
- AC4.2 用户权限被撤销后，WS 连接立即关闭
- AC4.3 事件经 WS 实时推送至授权订阅者

### FP5 Web 门户骨架（T1-5）

**需求**：React + Vite + TS 前端，独立 `web/` 目录：
- 会话列表（GET /threads）
- 任务时间线（thread 事件流渲染）
- WS 实时增量（接 FP4）

**验收标准**：
- AC5.1 会话列表页渲染
- AC5.2 时间线按 item_seq 排序渲染事件
- AC5.3 WS 连接后新事件实时增量更新

### FP6 CLI 登录层（T1-6）

**需求**：`nexus-control` CLI 加 `login`/`threads`/`run` 子命令，远端操作 thread。

**验收标准**：
- AC6.1 `nexus-control login` 存 JWT
- AC6.2 `nexus-control threads` 列出 thread
- AC6.3 `nexus-control run` 提交 turn

### FP7 Postgres 主库 schema（T1-7）

**需求**：26 实体 DDL + 索引（不含 RLS，M5 加）。sqlx migrate 迁移脚本，可重跑。
- 含 M0 events 表迁移（file-backed → Postgres）

**验收标准**：
- AC7.1 26 表 DDL 建成
- AC7.2 关键索引就位（外键 + 查询热路径）
- AC7.3 迁移脚本幂等可重跑（`sqlx migrate run`）

## 4. 测试用例

| 用例ID | 功能点 | 步骤 | 预期 | 对应 AC |
|---|---|---|---|---|
| TC-01 | FP1 | 建表 + 预置 admin | 表 + 角色存在 | AC1.1/1.3 |
| TC-02 | FP1 | check_permission(admin, *) | allow | AC1.2 |
| TC-03 | FP2 | 路由命中/404 | 正确分发 | AC2.1 |
| TC-04 | FP2 | 重复 Idempotency-Key | 返回缓存 | AC2.2 |
| TC-05 | FP2 | 超限流 | 429 | AC2.3 |
| TC-06 | FP3 | 登录签 JWT | JWT 返回 | AC3.1 |
| TC-07 | FP3 | 错密码 | 401 | AC3.2 |
| TC-08 | FP3 | 无 JWT 访问受保护路由 | 401 | AC3.3 |
| TC-09 | FP4 | 无权限订阅 | 拒绝 | AC4.1 |
| TC-10 | FP4 | 权限撤销 | 断连 | AC4.2 |
| TC-11 | FP5 | 会话列表页 | 渲染 | AC5.1 |
| TC-12 | FP5 | WS 增量 | 实时更新 | AC5.3 |
| TC-13 | FP6 | CLI login/threads | 成功 | AC6.1/6.2 |
| TC-14 | FP7 | migrate run ×2 | 幂等 | AC7.3 |

## 5. 约束与非目标

- **不改 codex-rs 内核**：扩展 `codex-rs/nexus-control/` crate
- **控制面继续 Rust**：一致性 + 类型复用（M0 已 Rust）；Web 门户用 TS/React 独立前端
- **OIDC 简化为 JWT**：单租户 MVP 不需 Keycloak，`AuthProvider` trait 预留 M5
- **单租户不含 RLS**：M5 多租户时加
- **不含 Runtime 池/审批/计费**：分别 M2/M3/M4
