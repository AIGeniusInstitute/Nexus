# Nexus M1 — 技术方案

> 需求：nexus-m1 · 分支：`feat/nexus-m1` · worktree：`.worktrees/feat/nexus-m1`
> 日期：2026-09-06 · 关联 PRD：`docs/prd/nexus-m1/PRD.md`

## 1. 架构决策

### 1.1 控制面继续 Rust（扩展 nexus-control crate）

M0 已 Rust（复用 `codex-app-server-protocol` 类型 + codex-rs workspace 构建缓存）。**M1 不换 Go/TS**——换会割裂类型定义、重复协议映射、丢构建缓存。

- `nexus-control` crate 加模块：`http`(axum) / `auth`(JWT) / `db`(sqlx) / `ws`(axum ws) / `rbac`
- Web 门户独立 TS/React（`codex-rs/nexus-control/web/`，前端独立栈）
- CLI 扩展 `nexus-control`（`serve`/`login`/`threads`/`run` 子命令）

> roadmap 团队栏写"Go/TS"是早期估算，与 M0 Rust 实际不符；审查后统一为"Rust 控制面 + TS 前端"。

### 1.2 crate 结构（单 crate 模块化，M2 视膨胀拆分）

```
codex-rs/nexus-control/
├── Cargo.toml          # +axum/sqlx/jsonwebtoken/tower/tower-governor/bcrypt
└── src/
    ├── lib.rs          # +pub mod {auth,rbac,http_server,ws,db}
    ├── main.rs         # CLI: poc*(M0) + serve + login + threads + run
    ├── stdio_client.rs # M0
    ├── event_store.rs  # M0，M1 改 Postgres 后端
    ├── execpolicy_rules.rs / model_gateway.rs  # M0
    ├── auth.rs         # JWT 签发/校验 + AuthProvider trait
    ├── rbac.rs         # check_permission(user, resource, action)
    ├── http_server.rs  # axum 路由 + 中间件链
    ├── ws.rs           # WS 升级 + 权限驱动订阅
    ├── db.rs           # sqlx PgPool + 迁移
    └── migrations/     # sqlx migrate（26 实体 DDL）
web/                    # React+Vite+TS 前端
```

### 1.3 OIDC 简化为 JWT（AuthProvider trait）

M1 单租户：本地用户表 + bcrypt + JWT。OIDC 抽象 trait 预留 M5：
```rust
trait AuthProvider { fn login(&self, user:&str, pw:&str) -> Result<Jwt>; fn verify(&self, jwt:&str) -> Result<Claims>; }
```
`LocalProvider`（M1 密码）+ `OidcProvider`（M5 Keycloak Authorization Code+PKCE）。

> **审查标注**：roadmap T1-3 原 OIDC Keycloak，M1 简化为本地 JWT（Simplicity First——单租户 MVP 不需 OIDC，Keycloak 是 M5 企业对接需求）。有意偏离，M5 补全。

### 1.4 Postgres + sqlx

`sqlx`（编译期 SQL 检查）+ `sqlx migrate` 迁移脚本。26 实体 DDL（见 §3）。M0 file-backed `events` → Postgres `events` 表（`INSERT ... ON CONFLICT DO NOTHING` 幂等，解除 M0 rusqlite 冲突限制）。

### 1.5 限流 + 幂等

- **限流**：`tower-governor`（IP 级）+ 自建租户/用户令牌桶（M1 用 DB 计数简化，M2 迁 Redis）
- **幂等**：`Idempotency-Key` 头 → `idempotency_records(key, response_json, ts)` 表，命中返回缓存结果不重复执行

## 2. 依赖与实现序

```
T1-1 身份模型 ──┬─→ T1-7 Postgres schema ──→ T1-2 Gateway ──→ T1-3 认证 ──→ T1-4 WS ──┬─→ T1-5 Web
                └──────────────────────────────────────────────────────────────────└─→ T1-6 CLI
```
序：T1-1/T1-7（身份+库）→ T1-2（Gateway）→ T1-3（认证）→ T1-4（WS）→ T1-5（Web）→ T1-6（CLI）。

## 3. Postgres Schema（26 实体 DDL，全建；M1 逻辑只接触标★表）

> 来源：架构产物 `03-domain-model-er/domain-model.md`。26 表按域分组，全部建 DDL（schema 一次到位避免 M2-M4 反复迁移），M1 逻辑只读写标★表，其余建空壳待后续填充。

**身份域（T1-1，★M1 用）**：`tenants` / `users` / `roles` / `tenant_memberships` / `workspaces` / `environments`
**会话五原语（★M1 用）**：`threads` / `turns` / `items` / `steps` / `workspace_snapshots`
**治理域（DDL 建，M3-M4 用）**：`approval_tickets` / `usage_records` / `quotas` / `budgets`
**执行面（DDL 建，M2 用）**：`sandbox_pods`
**模型/MCP（DDL 建，M2 用）**：`model_routes` / `model_credentials` / `mcp_servers` / `mcp_credentials`
**知识库/技能/连接器（DDL 建）**：`knowledge_bases` / `skills` / `skill_versions` / `connectors`
**审计（M1 简化普通表）**：`audit_logs`（M1 不分区，M8 加 `PARTITION BY RANGE`）/ `tool_call_logs`
**Gateway（★M1 用，自建补 2 表）**：`idempotency_records` / `rate_limit_buckets`（M1 DB 计数，M2 迁 Redis）

关键约束：
- `items`: `UNIQUE(thread_id, turn_id, seq)` + `INSERT ... ON CONFLICT DO NOTHING`（幂等，解 M0 file-backed 限制）
- `threads.permission_snapshot_hash`: M1 允许 NULL（M5 多租户启用权限快照防漂移）
- `audit_logs` / `usage_records` / `tool_call_logs`: 只追加（无 UPDATE/DELETE），M1 普通表
- `rollout_object_key` / `content_ref`: M1 用本地路径占位（M2 接对象存储）

> 注：roadmap 提"OrgUnit"，架构清单以 `workspaces`+`environments`+`tenant_memberships.scope_json` 表达组织单元，无独立 `org_units` 表；M1 以架构 26 表为准。

## 4. API 端点（M1 实现骨架子集；全量 60+ 留 M2-M4）

> 来源：架构产物 `05-api-spec/api-spec.md`。M1 只实现下标★端点（身份 + 会话骨架 + WS + 健康检查），其余端点 DDL/路由占位留后续里程碑。

**★M1 实现**：
- 认证（FP3）：`POST /v1/auth/login`（bcrypt 校验 → 签 JWT）/ `GET /v1/auth/me`（JWT → 当前用户）
- 会话（FP1/FP2/FP4）：`GET /v1/threads`（列表）/ `POST /v1/threads`（创建）/ `POST /v1/threads/:id/turns`（启 turn，桥接 app-server 子进程）/ `GET /v1/threads/:id/items`（事件流按 seq）
- 实时（FP4）：`WS /v1/ws/threads/:id/events`（权限驱动订阅，SSE 降级 `GET /v1/threads/:id/events?stream=sse`）
- 运维：`GET /health`（liveness）

**留 M2-M4（DDL/路由占位，不实现 handler）**：
- 租户/用户/角色 CRUD（`/v1/tenants` `/v1/users` `/v1/roles`）
- Workspaces 全套（CRUD + members + settings）
- Threads 高级（`/resume` `/fork` `/interrupt` `/steer` `/compact` `/search` `/shell` `/messages` `/timeline`）
- Approvals（`/v1/approvals/{pending,decide,rules}`）
- Connectors/MCP（`tools` `/call` `oauth`）
- 计量/审计/成本看板（`/v1/usage` `/v1/audit` `/v1/cost-dashboard`）
- KB/RAG（`/v1/kb/search` ACL `/v1/kb/embeddings`）
- Webhooks（`/v1/webhooks`）
- API Keys（`/v1/auth/api-keys`）

## 5. 风险与缓解

| 风险 | 缓解 |
|---|---|
| 单 crate 膨胀（>500LoC） | M1 模块化，M2 拆 `nexus-http`/`nexus-auth` 子 crate |
| OIDC 简化偏离 roadmap | 有意为之，`AuthProvider` trait 预留 M5 |
| Postgres 环境需起 | 本地 docker `postgres:16-alpine`（M0 Docker 已就绪） |
| Web 门户独立构建 | `web/` 独立 npm，不耦合 Rust 构建 |

## 6. 构建与运行

```shell
# 起本地 Postgres（M0 Docker 已就绪）
docker run -d --name nexus-pg -e POSTGRES_PASSWORD=nexus -e POSTGRES_DB=nexus -p 5432:5432 postgres:16-alpine
# 迁移
cargo run -p nexus-control -- migrate
# 起服务
cargo run -p nexus-control -- serve
# Web
cd codex-rs/nexus-control/web && npm install && npm run dev
```

## 7. 自主审查结论（开工前）

| # | 审查项 | 决策 |
|---|---|---|
| 1 | OIDC→JWT 简化偏离 roadmap T1-3 | 有意（Simplicity First；`AuthProvider` trait 预留 M5 Keycloak）— OK |
| 2 | 26 表全建但 M1 只用 ~10 | schema 一次到位避免 M2-M4 反复迁移 — OK |
| 3 | API 60+ 端点 M1 只实现 ~6 | 骨架子集，其余 DDL/路由占位留后续 — OK |
| 4 | `audit_logs` 不分区 | M1 普通表，M8 加 `PARTITION BY RANGE` — OK |
| 5 | `permission_snapshot_hash` NULL | M1 单租户不需，M5 多租户启用 — OK |
| 6 | 单 crate 加 6 模块 | 各 <500LoC，M2 视膨胀拆 `nexus-http`/`nexus-auth` — OK |
| 7 | 控制面 Rust（非 roadmap 团队栏"Go/TS"） | M0 已 Rust，一致性 + 类型复用优先 — OK |

**结论：方案 OK。3 处有意简化（#1/#4/#5）均符合 Simplicity First，标注清楚，不偏离架构产物。开工 M1。**

> 进度核对：任务一深度调研报告 + 任务二 8 维度架构产物已全交付（`docs/architecture/` 9 子目录 62 文件/7.3MB），M0 PoC 已交付（merge 6e860ce）。M1 是架构设计的实施阶段，方向一致，不偏离原始目标。
