# Nexus 构建进度跟踪

> 来源：`roadmap.md` 开发路线图 · 实时更新 · 工作目录 `~/Nexus`
> 当前状态：**M0–M19 全部里程碑 + Docker 一键部署 + Web 控制台 全部交付完成** · 2026-09-06

## 总览

| 维度 | 值 |
|---|---|
| 总里程碑 | 20 个（M0 PoC + M1–M19） |
| 已完成 | **20 / 20（100%）** |
| 控制面 crate | `codex-rs/nexus-control/`（Rust，workspace member，23 模块） |
| API 路由 | 44 个 REST/WS 端点 |
| 数据迁移 | 13 个 SQL migration（M1–M19） |
| 单元测试 | 32 个（cargo test 32/32） |
| 系统测试 | 25/25 端到端 PASS（`scripts/sys-test.sh`） |
| Web 控制台 | 11 个能力域页面 + 登录（React+Vite，单 bundle 190KB） |
| Docker 部署 | 一键 `deploy/deploy.sh`（pgvector:pg16 + nexus:8765） |
| 双远端 | `git@gitcode.com:AIGeniusInstitute/Nexus.git` + `git@github.com:AIGeniusInstitute/Nexus.git` |

## 阶段进度

| 阶段 | 里程碑 | 完成 | 状态 |
|---|---|---|---|
| ① PoC | M0 | 8/8 | ✅ 完成（merge 6e860ce） |
| ② 单租户 MVP | M1–M4 | 27/27 | ✅ 完成（身份/执行闭环/审批/计量） |
| ③ 多租户隔离 | M5–M7 | 12/12 | ✅ 完成（并发池/策略自学习/execpolicy 回写） |
| ④ 可靠性治理 | M8–M10 | 14/14 | ✅ 完成（真实模型/function calling/审计 WORM） |
| ⑤ 可观测评测 | M11–M12 | 10/10 | ✅ 完成（全链路 tracing/评测 CI 门禁） |
| ⑥ 知识协作生态 | M13–M19 | 19/19 | ✅ 完成（KB RAG/fork/连接器/skills/协作/MCP） |
| 部署 + Web | — | 2/2 | ✅ 完成（Docker 一键 + Web 控制台 11 页） |

## 里程碑交付清单

| MS | 名称 | 关键产物 | merge commit |
|---|---|---|---|
| M0 | PoC | stdio 集成+事件落库+resume+execpolicy+三层沙箱+gateway | 6e860ce |
| M1 | 身份+骨架 | 5 表 RBAC+axum 6 端点+JWT+WS+Web 门户+CLI | 034fad0 |
| M2 | 执行闭环 | driver 线程桥+事件落库+interrupt+resume+gateway | 4b5b7b6 |
| M3 | 审批与策略 | HITL 审批闭环+park+协议级回写+policy.rs+审批 Web | 8ed3cab |
| M4 | 产物与计量 | usage_records+cost 推导+execpolicy 下发+并发门控+用量 Web | c76c25f |
| M5 | 并发 Turn 池 | DriverPool free-list 调度+RAII guard+AtomicI64+turn_slots 路由 | 0f30580 |
| M6 | 策略自学习 | policy_feedback+learn()保守单调+extract_pattern+自动提升 | 6a01801 |
| M7 | execpolicy 回写 | amendment 协议级回写+单次生效+merge_amendment+安全单调 | 8d7bb57 |
| M8 | 真实模型联调 | gateway Responses↔Chat SSE 双向转换+真实 dashscope tokens | 7d953a8 |
| M9 | function calling | tools/tool_calls 双向转换+扁平↔嵌套+真实审批端到端 | 8299ffd |
| M10 | 审计 WORM | audit_logs+PG trigger 不可篡改+通用审计查询 API | e55b57f |
| M11 | 全链路 tracing | per-turn trace_id 贯穿+timeline 聚合+trace 关联查询 | 251dd92 |
| M12 | 评测+CI 门禁 | eval_cases/runs+断言+eval-gate.sh CI 脚本 | 7b1f174 |
| M13 | 知识库 RAG | pgvector+HNSW+混合召回+权限过滤先于召回+溯源 | d0fb04f |
| M14 | 快照 Fork/Rollback | snapshot→fork(单 imported turn)+rollback(事务删)+ROW_NUMBER 重编 | 25103cf |
| Docker | 一键部署 | deploy.sh+Dockerfile+compose(pgvector+nexus)+健康轮询 | f9e56e2 |
| M15 | Warm Pool | eager spawn+initialize 复用+pool status 端点 | f54a59f |
| M16 | 连接器市场 | connector 目录+tier/status/质量分+invoke_stub | cc7d5ef |
| M17 | Skills 市场 | 版本快照+publish 治理+rollback 不删版本 | ddf6d10 |
| M18 | 多 Agent 协作 | 3 模式(编排者-工作者/对等/批评对抗)+HTTP self-call 零触碰 drain | bd1e873 |
| M19 | MCP Gateway 转发 | 自建 stdio JSON-RPC 客户端+真实 MCP spawn+echo fixture | bd1e873 |
| Web | 控制台 | 11 页企业控制台+ServeDir 静态服务+Docker 集成 | 本批 |

## Web 控制台（本批交付）

| 能力域 | 路由 hash | 后端 API | 里程碑 |
|---|---|---|---|
| 登录 | `/#`（无 token） | POST /v1/auth/login | M1 |
| 总览 | `#overview` | poolStatus+approvals+usage+kbs+connectors+skills | M0–M19 |
| 会话时间线 | `#threads` | threads CRUD+turns+items(WS)+snapshots+fork/rollback | M1/M2/M14 |
| 审批中心 | `#approvals` | listApprovals+resolve(approve/deny/amendment) | M3/M7 |
| 协作编排 | `#orchestration` | startOrchestration(3 mode)+list+detail | M18 |
| 知识库 | `#kb` | kbs CRUD+documents ingest/delete+search | M13 |
| 技能市场 | `#skills` | skills CRUD+versions publish+rollback | M17 |
| 连接器市场 | `#connectors` | connectors CRUD+publish/offline+MCP invoke+calls | M16/M19 |
| 用量计量 | `#usage` | getUsage(7d)+聚合 | M4 |
| 策略中心 | `#policy` | policyRules+policyFeedback | M6/M7 |
| 评测中心 | `#evals` | evalCases+evalRuns+createCase+runEval | M12 |
| 审计日志 | `#audit` | auditLogs(action/since filter)+WORM | M10 |

## 系统全量验收

`scripts/sys-test.sh` — 25 用例 Docker 端到端全 PASS：

```
M1-login/auth-me/thread-create/list · M3-approval-loop · M4-usage ·
M6-policy-rules/feedback · M10-audit · M11-timeline · M12-evals ·
M13-kbs · M14-snapshots · M15-pool · M16-connector-create/list ·
M17-skill-create/publish/rollback · M18-orch-worker/peer/critic/list ·
M19-mcp-echo/quality
```

## 更新日志

- 2026-09-06：M0 PoC 全部 8 任务完成，三大假设验证（H1 长会话可恢复 / H2 execpolicy 可下发 / H3 三层沙箱生效）。
- 2026-09-06：M1–M4 单租户 MVP 交付（身份+骨架/执行闭环/审批/计量），merge 034fad0→4b5b7b6→8ed3cab→c76c25f。
- 2026-09-06：M5–M7 多租户隔离交付（并发池/策略自学习/execpolicy 回写），merge 0f30580→6a01801→8d7bb57。
- 2026-09-06：M8–M10 可靠性治理交付（真实模型/function calling/审计 WORM），merge 7d953a8→8299ffd→e55b57f。
- 2026-09-06：M11–M12 可观测评测交付（全链路 tracing/评测 CI 门禁），merge 251dd92→7b1f174。
- 2026-09-06：M13–M14 知识/fork 交付（KB RAG pgvector/快照 fork rollback），merge d0fb04f→25103cf。
- 2026-09-06：Docker 一键部署交付（deploy.sh + Dockerfile + compose），merge f9e56e2。
- 2026-09-06：M15–M17 池化/连接器/skills 交付（warm pool/连接器市场/skills 市场），merge f54a59f→cc7d5ef→ddf6d10。
- 2026-09-06：M18–M19 协作/MCP 交付（3 模式编排/MCP Gateway 真实转发），merge bd1e873。25/25 系统测试 PASS。
- 2026-09-06：**Web 控制台交付** — 后端 ServeDir 静态服务（`tower-http fs`，fallback_service 不受 auth route_layer 影响）+ 前端 11 页企业控制台（React+Vite，hash 路由，自写 UI 原语，零重依赖）+ Docker 镜像构建 web-dist + roadmap_progress 更新。

## 剩余外部环境依赖项（留置）

按"需要外部资源，无法自主搭建的先留着"原则留置：

| 依赖项 | 里程碑 | 所需外部资源 |
|---|---|---|
| IM Bot 推送审批 | M4 遗留 | 飞书/钉钉 bot token + sender→userId 解析器 |
| per-tenant 独占 slot | M6 遗留 | 真实多租户场景驱动 |
| 多 Pod 分布式 driver 池 | M5 遗留 | 真实多 Pod K8s + Redis 跨 Pod slot 调度 |
| 多模型路由 | M8 遗留 | glm-5.2 insufficient_quota（需多模型配额） |
| 私有化部署 | roadmap T12-1 | vLLM/Ollama 本地模型服务 |
| 100+ 并发稳定性 | roadmap T11-5 | K8s HPA 真实集群 |
| custom_tool_call 转换 | M9 扩展 | 真实 custom_tool 场景 |

**自主可完成维度已穷尽，core 系统（20 里程碑 + Docker + Web）全部构建完成。**
