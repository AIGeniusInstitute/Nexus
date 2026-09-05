# Nexus M1 — 执行状态

> 分支：`feat/nexus-m1` · worktree：`.worktrees/feat/nexus-m1`
> 技术方案：`docs/tech_solution/nexus-m1/TECH_SOLUTION.md`（审查通过，§7）
> 日期：2026-09-06

## 依赖序与进度

| 任务 | 描述 | 状态 |
|---|---|---|
| T1-1 | 身份租户模型（5 表 + RBAC 引擎） | ✅ 编码完成 |
| T1-7 | Postgres 26 实体 DDL + 迁移 | ✅ 编码完成 |
| T1-2 | API Gateway（REST + 幂等 + 限流） | ✅ 编码完成 |
| T1-3 | 认证（JWT + AuthProvider trait） | ✅ 编码完成 |
| T1-4 | WebSocket 网关（权限驱动订阅） | ✅ 编码完成 |
| T1-5 | Web 门户骨架（React+Vite+TS） | ✅ 编码完成 |
| T1-6 | CLI（serve/migrate/login/threads/run） | ✅ 编码完成 |

图例：🔄 进行中 / ✅ 完成 / ⏳ 待开始

## 执行记录

### 2026-09-06
- 技术方案审查通过（§7 自主审查结论：3 处有意简化符合 Simplicity First，不偏离架构产物）
- 开工 T1-1~T1-7 全量编码：
  - `Cargo.toml`：加 axum/sqlx(jsonwebtoken/tower/tower-http/bcrypt/chrono/async-trait/futures-util/headers/reqwest。sqlx 复用 workspace 0.9.0（+postgres feature；codex-rs 用 sqlite-bundled，libsqlite3-sys 0.37 与 codex-state 共存无冲突）
  - `migrations/20260906000001_initial.sql`：26 实体 + 2 gateway 表 + app_server_events，IF NOT EXISTS/ON CONFLICT 幂等，seed default 租户+admin 角色
  - `src/db.rs`：PgPool + 手动迁移（include_str! 嵌入 SQL + split by `;`，避免 sqlx migrate! 拉 sqlite feature 冲突）+ seed_admin + user_permissions
  - `src/rbac.rs`：check_permission（`*:*`/`resource:*`/`resource:action` 三级通配，4 单测）
  - `src/auth.rs`：AuthProvider trait + LocalProvider（bcrypt+JWT）+ JwtIssuer（HS256）+ AuthUser axum extractor
  - `src/http_server.rs`：axum Router + 6 handlers（health/login/me/threads CRUD/turns start/items list）+ 3 middleware（idempotency POST+Idempotency-Key DB 缓存 / user_rate_limit DB 令牌桶 100/min / ip_rate_limit 200/min）
  - `src/ws.rs`：WS 升级 + JWT 验证 + thread 租户权限 + items poll 推送 + 每 5s 复查 membership 撤销断连
  - `src/main.rs`：+Migrate/Serve/Login/Threads/Run 子命令（tokio current-thread runtime block_on）
  - `web/`：React+Vite+TS 骨架（登录页 + 会话列表 + 时间线 + WS 实时增量 + token localStorage）
- 关键决策：
  - sqlx 复用 workspace 0.9.0（非 0.8 独立）——避免 libsqlite3-sys 双版本 links 冲突
  - 手动迁移执行（include_str! + split）——避免 sqlx migrate! 宏拉 sqlite 后端
  - 去掉 tower-governor，自建 IP 限流 middleware（与 user 限流同 DB 模式，减少外部依赖不确定性）
  - turns start 写 mock system item（真实执行留 M2 runtime 池）
- 编译验证：cargo check -p nexus-control 进行中

### 2026-09-06（验证收尾）

**编译与单测**
- `cargo check -p nexus-control` → 0 error（清理 3 处 warning：http_server 去 Claims import；main.rs run_threads/run_run 参数 server→_server）
- `cargo test -p nexus-control` → 14/14 PASS（rbac 4 + db/auth/http/ws 10）

**端到端验证（Docker postgres:16-alpine 容器 nexus-pg，端口 8765）**
| 用例 | 结果 |
|---|---|
| TC-01 migrate 幂等 29 表 | ✅ |
| TC-02 seed_admin 幂等 | ✅ |
| TC-03 login JWT | ✅ |
| TC-04 错密码 401（修复 Ok(false) 放行） | ✅ |
| TC-05 /auth/me 无 token 401 | ✅ |
| TC-06 threads CRUD | ✅ |
| TC-07 turns start + items | ✅ |
| TC-08 thread 404 | ✅ |
| TC-09 幂等 x-idempotent-replay | ✅ |
| TC-10 用户限流 12×429 | ✅ |
| TC-11 IP 限流 429 | ✅ |
| TC-12 WS 无 token 401 | ✅ |
| TC-13 WS 跨租户 403 + 撤销断连（item frame + revoked + close） | ✅ |
| TC-14 CLI 四子命令 + Web build 31 modules + 3 截图 | ✅ |

**修复的 bug**
1. TC-04：`bcrypt::verify` 返回 `Ok(false)` 被 `?` 放行 → 改 `let ok = verify(...)?; if !ok { return Err(InvalidCredentials); }`
2. TC-09b WS：`require_auth_stateless` 用 Bearer header，WS 用 query token 被拦 401 → public 加 `/v1/ws/` 前缀，ws_handler 自验 token
3. /auth/me + threads create 返空 body：AuthUser 从 extensions 取 JwtIssuer 但 router 未注入 → `.layer(Extension(state.jwt.clone()))`
4. axum 0.8 路由 `:id` panic → `{id}`；axum::http::Request 缺泛型 → `axum::extract::Request`；AuthUser 去 `#[async_trait]`（RPITIT 冲突）；jsonwebtoken 9 `Header::new(Algorithm::HS256)`
5. vite proxy 8080（beego）→ 8765；web npm install 跳 devDeps（NODE_ENV=production）→ `unset NODE_ENV; npm install --include=dev`

**交付物**
- 测试报告：`docs/test_report/nexus-m1/TEST_REPORT.html`（archify 风格双主题 + 14 用例 + AC 矩阵 + curl 证据 + 3 截图 + SVG 架构图）
- 截图：`docs/test_report/nexus-m1/nx-{login,threads,timeline}.png`

## 最终状态

| 任务 | 状态 |
|---|---|
| T1-1 身份租户模型 | ✅ 完成 |
| T1-2 API Gateway | ✅ 完成 |
| T1-3 认证 JWT | ✅ 完成 |
| T1-4 WS 网关 | ✅ 完成 |
| T1-5 Web 门户 | ✅ 完成 |
| T1-6 CLI | ✅ 完成 |
| T1-7 Postgres schema | ✅ 完成 |

M1 全部完成，准备合并 feat/nexus-m1 → main + push 两远端。
