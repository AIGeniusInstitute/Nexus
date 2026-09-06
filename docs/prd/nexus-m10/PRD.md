# Nexus M10 PRD — 审计日志 WORM + 审计查询 API

## 背景

Nexus 已完成真实模型联调（M8）+ function calling（M9），核心 Agent 能力链路打通。但缺少企业级合规审计——关键操作（登录、turn 生命周期、审批决策、策略学习、amendment）缺乏不可变审计日志。

路线图 M10 T10-1 要求：审计日志 WORM（只追加 + SIEM 投递 + 应用层无 DELETE）。M10 补齐通用审计日志表（WORM 保证）+ 统一埋点 + 查询 API。

## 非目标（依赖外部资源，留着）

- IM Bot 推送审批卡片（需飞书/钉钉 bot token）
- per-tenant 独占 slot 隔离（需真实多租户场景）
- 多 Pod 分布式 driver 池（需真实多 Pod K8s）
- 多模型路由（glm-5.2 配额超限，缺备模型）
- SIEM 投递（需 SIEM 端点）

## 任务

### T10-1 audit_logs 表 + WORM 保证
- migration `20260906000006_m10_audit.sql`：audit_logs 表（id/tenant_id/actor_uid/action/target_type/target_id/detail(jsonb)/trace_id/created_at）
- WORM 保证：PG trigger `prevent_audit_modification` BEFORE UPDATE OR DELETE → RAISE EXCEPTION
- 索引：tenant+time DESC，action+time DESC

### T10-2 审计埋点（audit.rs + http_server.rs）
- `audit_log()` 函数（audit.rs）：INSERT only，返回 ()
- 埋点（http_server.rs 关键路径）：
  - auth.login（成功/失败，uid）
  - turn.start / turn.complete / turn.interrupt（turn_id）
  - approval.requested / approval.resolved（approval_id + decision）
  - policy.learned / policy.amendment（pattern）
- approval_audit（M3）保留不动（Surgical Changes）

### T10-3 审计查询 API
- GET /v1/audit/logs?action=&since=&limit=（admin 全租户，非 admin 仅本租户）
- GET /v1/audit/logs/{id}（单条详情）

## 验收标准

| AC | 描述 |
|----|------|
| AC10.1 | audit_logs 表 WORM：INSERT 成功，UPDATE/DELETE 报错 |
| AC10.2 | 关键操作埋点落库（login/turn/approval/policy） |
| AC10.3 | GET /v1/audit/logs 按 action/time 过滤，admin 全租户/非 admin 本租户 |
| AC10.4 | GET /v1/audit/logs/{id} 返回详情 |
| AC10.5 | 现有功能零回归（M3 审批/M4 计量/M9 真实模型不退化） |
