# Nexus 企业级 Web 控制台 PRD

## 背景
当前 Web 前端停留在 M1-M4 水平（3 个源文件、3 个视图：会话/审批/用量），后端已交付 M0-M19 共 44 个 API 路由、13 个迁移、23 个模块。后端 M6-M19 共 13 个里程碑的能力在 Web 前端**零覆盖**。用户看到的是一个标题写着 "Nexus M1" 的简陋看板，完全看不出"企业级 AI Agent 平台"的形态。本需求补齐前端 GAP，构建企业级控制台。

## 目标
1. 重构 Web 前端为企业级控制台，覆盖后端全部 11 个能力域
2. 控制面二进制（nexus-control serve）内置静态文件服务，8765 端口直接出首页（不再需要独立 vite dev server）
3. Docker 镜像内置前端构建产物，一键部署即出完整控制台
4. 更新 roadmap_progress.md 至 M0-M19 全部完成状态

## 功能需求（11 个能力域页面）

### F1 概览首页（Overview）
- 系统状态卡片：runtime pool（warmed/in_flight/free）、待审批数、近 7 天 turns/tokens/cost、KB 文档数、连接器数、skills 数
- 里程碑覆盖矩阵（M1-M19 各域状态）
- 品牌头 + 侧栏导航外壳

### F2 会话（Threads）— 增强既有
- 会话列表 + 创建
- 时间线视图（调用 M11 timeline API：turns+items+approvals 聚合）
- 提交 turn + WS 实时增量
- 中断 turn（M2 interrupt API）
- 快照管理（M14：创建/列表/fork/rollback 入口）

### F3 审批（Approvals）— 增强
- pending 列表 + 批准/拒绝/中断
- 显示 policy_decision + risk_level 标签（M3/M4）
- approve_with_amendment（M7）

### F4 用量（Usage）— 保留增强
- 近 7 天 token/cost 柱图
- per-user 用量（M4 /v1/usage/users/{uid}）

### F5 知识库（Knowledge Base）— M13 新增
- KB 列表/创建
- 文档摄入/列表/删除
- 向量搜索（query + keyword + top_k，返回 source/title/snippet/score）

### F6 连接器市场（Connectors）— M16/M19 新增
- 连接器 CRUD + 分级（official/enterprise/community）+ 状态（draft/published/offline）
- 发布/下线
- 真实 MCP 调用（invoke：tool + args，返回 mcp/success/result）
- 质量分 + 调用历史

### F7 技能市场（Skills）— M17 新增
- skill CRUD + 版本发布 + 回滚
- 版本列表

### F8 协作编排（Orchestration）— M18 新增
- 3 模式选择（orchestrator-worker/peer/critic-adversarial）
- 启动编排（prompt + agents 数）
- 编排列表 + 详情（agents 步骤）

### F9 评测（Evals）— M12 新增
- eval case CRUD（input + expected_status + expected_contains）
- 对已完成 turn 运行断言
- runs 列表（passed/fail）

### F10 审计（Audit）— M10 新增
- 审计日志列表（action/since/limit 过滤）
- WORM 只读展示

### F11 策略（Policy）— M6 新增
- 规则列表（pattern/decision/source/priority）
- 反馈历史（feedback：pattern/decision/risk）

## 验收标准

### AC1 企业级首页
- 访问 http://localhost:8765/ 返回完整控制台首页（非 404、非简陋 M1 看板）
- 首页显示品牌头、侧栏导航（11 个域）、概览状态卡片
- 截图：登录页 + 概览页（含状态卡片）

### AC2 全域能力可达
- 左侧导航 11 个域，每个点击进入对应页面
- 每个页面调用对应后端 API 渲染真实数据（非空壳）

### AC3 静态文件服务
- nexus-control serve 内置静态服务，NEXUS_WEB_DIR 指向 dist 即可（fallback index.html SPA 路由）
- /v1/* API 路由与静态文件不冲突
- 静态资源无需认证（route_layer 仅作用于显式路由）

### AC4 构建产物
- `npm run build`（vite build）成功，产出 web/dist/
- Docker 镜像 COPY dist，一键部署后 8765 出完整控制台

### AC5 零回归
- 后端 API 路由全部不变，cargo check 0 error
- 现有 M1-M4 前端功能（会话/审批/用量）在新控制台中保留且工作
- 系统测试 25/25 仍全 PASS

### AC6 进度文档更新
- roadmap_progress.md 反映 M0-M19 全部完成

## 测试用例
- TC1: curl http://localhost:8765/ → 200 + 含 "<div id=\"root\">" + JS bundle
- TC2: 登录页渲染（screenshot）
- TC3: 概览页渲染 + 状态卡片有数据（screenshot）
- TC4: 11 个导航域各点击进入，页面渲染（screenshot 抽样）
- TC5: 连接器页 invoke echo MCP → 显示 result（M19 真实转发）
- TC6: 知识库页搜索 → 显示召回结果（M13 RAG）
- TC7: 协作编排页启动 3 模式 → completed（M18）
- TC8: cargo check 0 error，系统测试 25/25 PASS
