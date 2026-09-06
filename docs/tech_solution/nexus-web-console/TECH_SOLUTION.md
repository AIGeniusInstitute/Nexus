# Nexus 企业级 Web 控制台 技术方案

## 设计原则
- **Simplicity First**：保持 React + Vite 技术栈，不引入路由库/状态管理/UI 框架。用 hash 路由 + React state，自写轻量 UI 原语。
- **Surgical Changes**：后端仅加静态文件 fallback（一处 router 改动 + 一个 AppState 字段），不动任何 API handler。前端从 3 文件扩展为多文件，但 api.ts 兼容既有方法签名。

## 一、后端：静态文件服务接入

### 1.1 依赖
`Cargo.toml` 的 `tower-http` 加 `fs` feature：
```toml
tower-http = { version = "0.6", features = ["trace", "cors", "fs"] }
```

### 1.2 AppState
新增字段 `pub web_dir: std::path::PathBuf`（默认 `/app/web-dist`，由 `NEXUS_WEB_DIR` env 覆盖）。

### 1.3 router fallback
在 `router()` 末尾 `.with_state(state)` 前加：
```rust
.fallback_service(
    tower_http::services::ServeDir::new(&state.web_dir)
        .append_index_html_on_directories(true)
        .not_found_service(tower_http::services::ServeFile::new(
            state.web_dir.join("index.html"),
        )),
)
```
**关键**：`.fallback_service` 不受 `.route_layer(require_auth_stateless)` 影响（route_layer 仅作用于显式 `.route()` 注册的路由）。/v1/* API 与 /health 显式注册优先匹配，静态资源走 fallback 免认证。零冲突。

### 1.4 main.rs
`run_serve` 读 `NEXUS_WEB_DIR` env（默认 `/app/web-dist`），注入 AppState。无需存在性校验（不存在时 ServeDir 返回 404，API 仍工作）。

## 二、前端：企业级控制台

### 2.1 文件结构
```
web/src/
  api.ts          # 扩展：覆盖全部 44 路由对应的 ~30 个方法 + TS 类型
  ui.tsx           # UI 原语：Card/Table/Button/Pill/Empty/ErrBar/Modal/Field
  theme.css        # 设计系统：CSS 变量、深/浅双主题、布局
  main.tsx         # React 挂载
  App.tsx          # 外壳：hash 路由 + 侧栏 + 顶栏
  pages/
    Overview.tsx       # 概览首页
    Threads.tsx        # 会话+timeline+snapshots
    Approvals.tsx      # 审批
    Usage.tsx          # 用量
    KnowledgeBase.tsx  # 知识库 RAG
    Connectors.tsx     # 连接器市场
    Skills.tsx         # 技能市场
    Orchestration.tsx  # 协作编排
    Evals.tsx          # 评测
    Audit.tsx          # 审计
    Policy.tsx         # 策略
```

### 2.2 设计系统（theme.css）
- CSS 变量：--bg/--panel/--accent/--gold/--grn/--red/--txt/--mut/--bd
- `data-theme="dark|light"` 双主题，localStorage 记忆
- 布局：左侧固定侧栏（240px）+ 右侧主内容区，顶栏品牌 + 主题切换
- 企业级色板（与 archify 深色主题对齐：BG #0b141a / PANEL #13242e / ACCENT #35c2b0 / GOLD #e8b64c）

### 2.3 路由
无路由库。App.tsx 用 `window.location.hash`（`#/threads` 等）+ `useState` 切换页面。hash 变化监听 `hashchange`。简单、无依赖、刷新可恢复。

### 2.4 api.ts 扩展
保留既有方法（login/listThreads/startTurn/listItems/listApprovals/resolveApproval/getUsage/openThreadStream），新增：
- threads: timeline, snapshots (create/list/fork/rollback), turn interrupt
- approvals: listByThread, resolve with amendment
- usage: perUser
- policy: rules, feedback
- audit: logs (with filters), logGet
- evals: caseCreate, casesList, runEval, runsList
- kbs: create, list, docIngest, docList, docDelete, search
- snapshots: create, list, fork, rollback
- pool: status
- connectors: create, list, get, update, delete, publish, offline, quality, invoke, calls
- skills: create, list, get, delete, publishVersion, versions, rollback
- orchestrations: start, list, get

### 2.5 UI 原语（ui.tsx）
- `<Card title>` 卡片容器
- `<Table cols rows>` 数据表格（支持 render 函数列）
- `<Button onClick variant>` 按钮（primary/ghost/danger）
- `<Pill tone>` 标签（success/warn/danger/info）
- `<Empty>` 空状态
- `<ErrBar>` 错误条
- `<Field label>` 表单字段
- `<Modal>` 弹窗（create 表单用）

## 三、Docker 集成

### 3.1 Dockerfile
- 新增 Stage：`node:20-alpine` 构建 web（`npm ci && npm run build` → /dist）
- 主阶段 COPY web/dist → /app/web-dist
- ENV NEXUS_WEB_DIR=/app/web-dist
- 不再需要单独 serve 前端

### 3.2 deploy.sh
- 新增 web 构建步骤（宿主机 `npm run build` 或 Docker 多阶段构建）
- 优先用 Docker 多阶段构建（自包含，不依赖宿主 node 环境）

## 四、进度文档
重写 `docs/architecture/09-roadmap/roadmap_progress.md`：M0-M19 全部完成（70/70 任务映射，实际 20 里程碑全交付）。

## 五、验证计划
1. `cd web && npm run build` → dist/ 产出
2. `cargo check -p nexus-control` 0 error
3. Docker 重建部署 → 8765 出首页
4. playwright 截图：登录页 + 概览页 + 各域抽样
5. `scripts/sys-test.sh` 25/25 零回归
