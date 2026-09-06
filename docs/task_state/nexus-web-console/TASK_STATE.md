# TASK STATE — Nexus Web 控制台

> 需求：用户反馈"首页没有，怎么是企业级 AI Agent 平台" → 深度审查源代码 vs 任务目标 GAP → 补齐全部 GAP
> worktree：`.worktrees/feat/nexus-web-console`（分支 `feat/nexus-web-console`）
> 日期：2026-09-06 · 状态：**已完成**

## GAP 审查结论

| 维度 | 审查前 | 审查后（本批修复） |
|---|---|---|
| 首页 | Docker 8765 访问 `/` 返回 404（nexus-control 从不服务静态文件） | ServeDir fallback，`/` 返回企业控制台首页 |
| 前端覆盖 | M1–M4 时代 3 文件 3 视图（会话/审批/用量），后端 44 路由前端只调 9 | 11 页企业控制台，覆盖全部 44 路由 |
| 里程碑前端 | M6–M19（13 里程碑）零前端 | 全部 13 里程碑有对应页面 |
| Docker 镜像 | 不含前端产物 | 多阶段构建 web-dist 入镜像 |
| roadmap_progress | 停在 M0（8/70） | 更新至 M0–M19 全完成 |

## 执行步骤

### 1. 后端静态服务（外科手术改动）

- `Cargo.toml`：`tower-http` 加 `fs` feature。
- `http_server.rs`：AppState 加 `web_dir: PathBuf` 字段；router `.fallback_service(ServeDir::new(&web_dir).append_index_html_on_directories(true).not_found_service(ServeFile::new(web_dir/index.html)))`。
- `main.rs`：AppState 构造 `web_dir = env::var("NEXUS_WEB_DIR").unwrap_or("/app/web-dist")`；标题 `Nexus Web Console: serve`。

**关键判断**：`.route_layer(require_auth_stateless)` 只作用于显式 `.route()` 注册，不作用于 `fallback_service` → 静态文件无鉴权直接服务，`/v1/*` API 路由优先匹配，零冲突。

验证：`cargo check -p nexus-control` → 0 error 0 warning。

### 2. 前端 11 页企业控制台

技术选型（Simplicity First）：React + Vite，无路由库（hash 路由），无状态管理，无 UI 框架，自写 UI 原语。

| 文件 | 职责 |
|---|---|
| `theme.css` | archify 设计语言：深色 BG#0b141a/PANEL#13242e/ACCENT#35c2b0/GOLD#e8b64c，浅色主题，sidebar/card/table/btn/pill/modal/timeline |
| `api.ts` | 全 44 路由 API 客户端 + WS helper + 类型定义 |
| `ui.tsx` | Card/Table/Button/Pill/Empty/ErrBar/Field/Modal/useAsync/fmtNum/fmtCost/fmtTime |
| `App.tsx` | hash 路由 shell + Login + Sidebar(3 组 11 域) + 主题切换 |
| `pages/Overview.tsx` | 首页：StatGrid + M1–M19 里程碑矩阵 + 三面架构图 |
| `pages/Threads.tsx` | 会话列表 + Timeline(WS 增量) + turn 提交 + snapshot fork/rollback |
| `pages/Approvals.tsx` | 审批列表 + approve/deny/cancel + amendment 模态 |
| `pages/Orchestration.tsx` | 3 模式启动 + 编排详情 + agent 步骤表 |
| `pages/KnowledgeBase.tsx` | KB CRUD + 文档摄入 + 语义/关键词混合召回 |
| `pages/Connectors.tsx` | 连接器 CRUD + publish/offline + 真实 MCP invoke + 调用历史 |
| `pages/Skills.tsx` | 技能 CRUD + 版本发布 + 回滚 |
| `pages/Usage.tsx` | 7 日柱图(input+output) + 统计翻牌 + 明细表 |
| `pages/Policy.tsx` | 策略规则表 + 人决策反馈历史 |
| `pages/Evals.tsx` | 评测用例 + 运行断言 + PASS/FAIL |
| `pages/Audit.tsx` | 审计日志 + action/since 过滤 + WORM 标识 |

验证：`npm run build` → tsc -b 0 error + vite build 44 modules → 190KB JS / 6KB CSS。

### 3. Docker 集成

- `Dockerfile`：加 `COPY web-dist/ /app/web-dist/` + `ENV NEXUS_WEB_DIR=/app/web-dist`。
- `deploy.sh`：加第 4 步 `npm install --include=dev + npm run build → cp dist 到 web-dist/ 上下文`（含 esbuild postinstall 手动补装）。
- `deploy/.gitignore`：加 `web-dist/`。

验证：`deploy.sh` 全流程 → 镜像构建成功 → compose up → postgres+nexus healthy → `curl /` 200 + index.html → login 返回 JWT。

### 4. 截图验证

playwright-core + 系统 `/usr/bin/google-chrome`（`--no-sandbox`）截取 12 张 1440×960：
`01-login / overview / threads / approvals / orchestration / kb / skills / connectors / usage / policy / evals / audit`。

### 5. 零回归系统测试

`scripts/sys-test.sh` 25/25 PASS：M1–M19 全维度端到端零退化。

## 关键决策

| # | 决策 | 理由 |
|---|---|---|
| D1 | ServeDir fallback 而非自定义 handler | tower-http 原生支持，零代码；route_layer 不影响 fallback |
| D2 | hash 路由而非 react-router | 零依赖；SPA 单文件足够；Simplicity First |
| D3 | 自写 UI 原语而非引入 UI 框架 | 企业控制台 11 页用不到 antd/MUI 的复杂度；bundle 190KB |
| D4 | 宿主机构建 web-dist + COPY 进镜像 | 避免容器内装 node 工具链；镜像瘦 |
| D5 | 不改任何 API handler | 外科手术原则；44 路由零触碰 |
