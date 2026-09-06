# PRD — Nexus M16 连接器生态市场

## 1. 背景
roadmap T12-2：连接器生态市场，验收标准"社区贡献流程 + 质量分展示"。依赖 T7-2（连接器治理：分级+质量分+上下线）。现有 schema 已铺路——`connectors`/`mcp_servers`/`tool_call_logs` 表已建（initial.sql），但**无任何 Rust handler 或模块**。M16 在此基础上构建连接器目录 + 治理层（CRUD + 分级 + 质量分 + 上下线 + 调用代理骨架），不接真实 MCP 转发（留 T7-1 执行面）。

## 2. 目标
为企业提供连接器目录的元数据 + 治理层：社区贡献（提交→发布）、分级标签（官方/企业/社区）、质量分（基于调用成功率）、上下线状态管理、调用代理骨架（记录调用 intent，真实 MCP 转发 stub）。

## 3. 非目标（外科手术 / 简洁至上）
- 不做真实 MCP server 转发（T7-1 执行面，留置）。
- 不做凭据代理短期令牌（T7-3，需 KMS/Vault，留置）。
- 不触碰 turn_start / drain 循环 / runtime driver。
- 不做 Web 前端（后端 API + 单测 + e2e 验证）。
- 质量分用简单成功率公式，不做多维加权（留扩展）。

## 4. 功能需求
| FR | 描述 |
|---|---|
| FR1 | 连接器 CRUD：创建/列表/查/改/删（本租户隔离） |
| FR2 | 分级标签 tier：official / enterprise / community（默认 community） |
| FR3 | 状态流转 status：draft → published → offline（publish/offline 需 admin `*:*`） |
| FR4 | 质量分 quality_score：基于 tool_call_logs 成功率（success=true/total），可手动触发重算 |
| FR5 | 调用代理骨架 invoke：记录调用 intent 到 tool_call_logs（stub success=true），真实转发留 T7-1 |
| FR6 | 调用历史 calls：按连接器查 tool_call_logs |
| FR7 | 贡献者归属 contributor_user_id |

## 5. 验收标准
| AC | 验收点 |
|---|---|
| AC1 | POST /v1/connectors 创建（tier=community, status=draft），GET 列表本租户隔离 |
| AC2 | PUT 更新 name/config/description/tier，跨租户 404 |
| AC3 | POST publish（admin→published；非 admin 403；draft 才能 publish） |
| AC4 | POST offline（admin→offline；published 才能 offline） |
| AC5 | POST invoke 记录 tool_call_logs（connector_id+success=true），GET calls 返回历史 |
| AC6 | GET quality 返回 success/total 比率（无调用→0.0，2 success/3 total→0.667） |
| AC7 | DELETE 删除（无关联 tool_call_logs 才能删，有则 409 或级联——MVP 拒绝删除有调用的） |
| AC8 | 零回归：M3 审批 / M4 计量 / M13 KB / M14 fork 不退化 |

## 6. 设计要点
- **纯增量**：新模块 `connectors.rs` + migration（ALTER 表加字段）+ 路由，不碰核心路径。
- **租户隔离**：所有查询 `WHERE tenant_id=$X`；admin（`*:*`）仅 publish/offline 治理动作跨租户可见性不放宽（目录仍本租户）。
- **质量分公式**：`SELECT COUNT(*) FILTER (WHERE success) * 1.0 / NULLIF(COUNT(*),0)` — total=0 返回 0.0。
- **stub 调用**：invoke 不接真实 MCP，记 tool_call_logs(success=true, result_ref="stub")，验证调用链路+质量分数据源。
- **删除约束**：有 tool_call_logs 的连接器拒绝删除（保留调用审计 trail），返回 409。
