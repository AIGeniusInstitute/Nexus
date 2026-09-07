# Nexus Agent Studio — 任务执行状态

> 分支：feat/nexus-agent-studio
> PRD：docs/prd/nexus-agent-studio/PRD.md
> 技术方案：docs/tech_solution/nexus-agent-studio/TECH_SOLUTION.md

## 执行进度

### 阶段 1：需求/方案（完成）
- [x] PRD（含 AC-1~AC-4 + 10 测试用例）
- [x] 技术方案（架构定位 + 后端/前端改造 + 关键决策记录 + 改动文件清单）

### 阶段 2：后端编码（完成）

改动文件：
- `migrations/20260907000001_agent_studio.sql`（新）：agent_definitions 表 + threads.agent_def_id
- `src/runtime.rs`：
  - `map_notification` 加 6 个 delta 臂（AgentMessageDelta/ReasoningTextDelta/ReasoningSummaryTextDelta/CommandExecutionOutputDelta/PlanDelta/FileChangeOutputDelta）→ 转发 delta 到 TurnEvent（codex_item_id=item_id, content_ref=delta）
  - `DriverCommand::RunTurn` 加 `system_prompt: Option<String>` 字段
  - driver_loop RunTurn 解构加 system_prompt；thread_start 注入 `base_instructions`（仅新 thread，resume 忽略）
- `src/http_server.rs`：
  - turn_start 查询读 agent_def_id → 查 system_prompt → 传 RunTurn
  - drain 中 delta 跳过 items INSERT（`ends_with("/delta") || ends_with("Delta")`，覆盖 lowercase + camelCase）
  - thread_create 扩展 agent_def_id
  - 5 Agent 路由 + handler（agent_create/list/get/update/delete）+ map_agent_err
- `src/agent_defs.rs`（新）：AgentDefRow + CRUD + delta_item_type_sentinel 单测
- `src/db.rs`：注册 AGENT_STUDIO_MIGRATION_SQL
- `src/lib.rs`：注册 agent_defs 模块
- `src/main.rs`：标题 "Nexus Agent Studio: serve"

验证：
- cargo check 0 error 0 warning
- cargo test 46 passed（45 既有 + 1 新 delta_item_type_sentinel）零回归

### 阶段 3：前端编码（完成）

改动文件：
- `web/src/api.ts`：AgentDef 接口 + StreamFrame 接口 + Agent CRUD（listAgents/createAgent/getAgent/updateAgent/deleteAgent）+ createThread 扩展 agentDefId
- `web/src/ui.tsx`：Markdown（手写极简渲染器，React 元素无 XSS，支持代码块/行内代码/粗体/链接/标题/列表/段落）+ ReasoningBlock（琥珀折叠）+ ToolCard + KV + CodeBlock
- `web/src/pages/Studio.tsx`（新）：
  - StudioPage：会话列表 + Agent 定义管理 Modal
  - StudioChat：CUI 对话框，WS 帧状态机（item/started+completed upsert、delta 按 item_id 聚合、approval/requested 卡片、turn/completed 结束）
  - AssistantBlock：按 ThreadItem kind 分发渲染（reasoning 折叠/agentMessage markdown/commandExecution 卡片/mcpToolCall 卡片/functionCallOutput/fileChange diff/plan）
- `web/src/App.tsx`：studio 路由 + sidebar「Agent Studio」+ 标题更新
- `web/src/theme.css`：studio-chat/md/reasoning/tool-card/kv/approval-card 样式

验证：
- npx tsc --noEmit 0 error
- npx vite build 46 modules（225KB JS / 8KB CSS）

### 阶段 4：e2e 验证（完成）

开发环境：
- PG: nexus-pg-m4:5434（nexus/nexus/nexus）
- 端口: 8766（避开旧 8765 Docker 部署）
- 模型: 智谱 glm-4-flash（NEXUS_UPSTREAM_MODEL_URL + NEXUS_MODEL_KEY，真实模型非 SIMULATE）
- codex 二进制: ~/.local/bin/codex
- 前端: worktree web/dist（NEXUS_WEB_DIR 静态服务）

测试矩阵（全过）：
- [x] TC-1: map_notification delta 单测（cargo test 46/46，含 delta_item_type_sentinel）
- [x] TC-2: Agent 定义 CRUD（e2e create id=5/list/get/update 全过）
- [x] TC-3: thread 绑定 Agent（agent_def_id 持久化 = 5）
- [x] TC-4: system_prompt 注入（turn completed 间接验证 base_instructions 被接受）
- [x] TC-5: 零回归（cargo test 46/46，既有 45 + 新 1，无退化）
- [x] TC-6: 真实模型流式（3293 delta 行落 app_server_events / 0 delta 入 items / 2 item 持久化 / WS 53 delta 帧 broadcast）
- [x] TC-7: Studio CUI 渲染（user-bubble + assistant-msg Markdown + tool-card 命令执行卡片，0 📦 fallback）
- [x] TC-8: 历史回放（loadHistory 从 items 表 item/completed 还原，断线重连靠 ws.rs re-subscribe）
- [x] TC-9: 审批卡片（M3 审批闭环不退化；ls 命中 execpolicy allow 自动执行无需审批，approval-card 渲染分支保留）
- [x] TC-10: cargo test 零回归（46/46）

### 阶段 5：测试报告 + 合并（完成）
- 测试报告：docs/test_report/nexus-agent-studio/TEST_REPORT.html
- 合并 main + push origin/github

## 关键决策记录

1. **delta 不入 items 表**：items 幂等键是 codex_item_id（item 级），delta 同 item_id 多条会冲突；delta 是瞬态流式数据，落 app_server_events 已够审计
2. **map_notification 不改签名**：driver_loop 真实 drain 已用 `method` 作为 item_type，map_notification 只需返回 (item_id, delta, None, false) 4 元组
3. **delta sentinel 双匹配**：codex delta method 有两种风格——`*/delta`（lowercase: agentMessage/delta, plan/delta）和 `*Delta`（camelCase: textDelta, outputDelta, summaryTextDelta）。drain 用 `ends_with("/delta") || ends_with("Delta")` 覆盖（单测验证）
4. **手写 markdown 渲染器**：不引入 marked（worktree 无 node_modules，避免依赖问题），手写极简渲染器生成 React 元素（无 innerHTML → 无 XSS）
5. **fire-and-forget startTurn**：turn_start HTTP 同步阻塞到 completed，await 会卡 UI；WS 是流式通道，提交后立即靠 WS 渲染增量（deepthink 同模式）
6. **system_prompt 走 base_instructions**：ThreadStartParams.base_instructions 字段，thread 级固化；resume 不重传
7. **WS 中途订阅修复（关键 bug）**：ws.rs 在连接时订阅 broadcast channel，但新 thread 首次 turn 前 channel 不存在 → rx=None → 整个会话恒 None，delta 帧全丢（只收到 items 表回放的 item/started+completed）。修复：run() 循环每轮若 broadcast_rx 为 None 则重新从 map 获取（channel 被 turn_start 创建后即拾取）。这是 M2 时代遗留 bug，Agent Studio 首次依赖 live delta 才暴露。
8. **userMessage 渲染为用户气泡**：codex 回传 userMessage item（用户输入回显），初版落 📦 JSON fallback（丑陋）。修复：AssistantBlock 加 userMessage 分支，从 raw.content[].text 提取文本渲染为右对齐用户气泡（.user-bubble CSS）。
9. **迁移幂等**：migrations 用 CREATE TABLE IF NOT EXISTS + ALTER ADD COLUMN IF NOT EXISTS，重启安全（非 CREATE TABLE，二次启动会报 "already exists" 崩）。
