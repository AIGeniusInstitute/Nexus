# 任务状态 — Nexus M16 连接器生态市场

## 里程碑
M16 = 连接器生态市场（roadmap T12-2），治理层（分级+质量分+上下线+调用代理骨架）。

## 任务清单
| ID | 任务 | 状态 |
|---|---|---|
| T16-1 | migration（connectors 加 tier/status/quality_score/contributor/description/updated_at；tool_call_logs 加 success/connector_id） | ✅ 完成 |
| T16-2 | connectors.rs（CRUD + set_status + compute_quality + invoke_stub + list_calls + 单测） | ✅ 完成 |
| T16-3 | http_server.rs 9 路由 + map_conn_err 错误映射 | ✅ 完成 |
| T16-4 | lib.rs 注册模块 + main.rs 标题 + db.rs migration 接线 | ✅ 完成 |

## 验证结果
- cargo check：0 error 0 warning
- cargo test：32/32（M15 31 + connectors 1 quality_formula_cases）
- e2e AC1-8：全过
  - AC1 create（draft/community/quality=0.0）+ list 本租户
  - AC2 update（desc/tier）+ 跨租户隔离（代码 WHERE tenant_id）
  - AC3 publish（admin→published；非 admin 403；invalid transition 400）
  - AC4 offline（published→offline）
  - AC5 invoke x3（2 success 1 fail）+ calls=3 历史
  - AC6 quality=0.6667（2/3）
  - AC7 delete 有 calls→409 in_use / 无 calls→200
  - AC8 零回归：M3 审批闭环（approval→approve→completed）+ M4 计量 + M15 warm pool + M14 fork + connectors

## 改动文件
- migrations/20260906000011_m16_connector_market.sql（新）
- src/connectors.rs（新）
- src/db.rs（接线 M16 migration）
- src/lib.rs（注册 connectors 模块）
- src/http_server.rs（9 路由 + 10 handler + map_conn_err）
- src/main.rs（标题 M16）
- docs/{prd,tech_solution,task_state,test_report}/nexus-m16/

## 关键决策
- 纯增量（不碰 turn_start/drain/runtime/gateway）
- 删除约束（有 tool_call_logs→409，保留审计 trail）
- 质量分 success/total（total=0→0.0，PG COUNT FILTER）
- publish/offline 需 admin（*:*），CRUD 本租户
- invoke stub 记 tool_call_logs（真实 MCP 转发留 T7-1）

## 状态
全量完成，待合并 main + push 两远端。
