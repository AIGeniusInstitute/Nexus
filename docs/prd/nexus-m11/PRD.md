# Nexus M11 PRD — 全链路 tracing + timeline 聚合查询

## 背景
M0–M10 已交付控制面核心（身份/执行/审批/计量/池化/策略/amendment/真实模型/工具调用/审计 WORM）。可观测维度的"统一时间线回放"与"按 trace_id 关联审计"尚未补齐。M10 已在 audit_logs 留 trace_id 字段但未贯穿。

## 目标
1. 为每个 turn 生成稳定的 `trace_id`，贯穿 audit_logs 埋点（M10 字段填充）
2. 提供 `GET /v1/threads/{id}/timeline` —— 聚合 turns+items+approvals 成统一时间线（按时间排序）
3. 提供 `GET /v1/traces/{trace_id}` —— 按 trace_id 聚合 audit_logs + 关联 turn

## 非目标
- 不改 runtime/stdio_client（trace_id 仅 http_server 层，Simplicity First）
- 不做分布式 tracing（OTel 导出）——留外部环境驱动
- 不动 M3 approval_audit / M10 audit_logs 表结构（仅 turns 加列 + 填充 trace_id）

## AC
- AC11.1 turns 表有 trace_id 列，新 turn 自动生成 UUID
- AC11.2 turn.complete / approval.resolve / turn.interrupt 的 audit_logs 记录 trace_id 非空
- AC11.3 GET /v1/threads/{id}/timeline 返回 turns+items+approvals 合并时间线（按 ts 升序）
- AC11.4 GET /v1/traces/{trace_id} 返回该 trace 的 audit_logs + 关联 turn
- AC11.5 零回归：SIMULATE turn+approval+计量+审批路径不退化
