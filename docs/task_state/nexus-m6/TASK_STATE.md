# Nexus M6 策略自学习闭环 — 任务状态

| 字段 | 值 |
|---|---|
| 分支 | `feat/nexus-m6` |
| 里程碑 | M6 = 策略自学习闭环 / Policy Learning Loop |
| 任务 | T6-1 ~ T6-5 |
| 起始基线 | M5（merge 0f30580） |
| 状态 | ✅ 全部完成，e2e + 零回归全过 |

## 目标

将"人 resolve 审批"从一次性动作升级为**自学习闭环**：记录人决策 → 累计 N 次
一致且与当前策略矛盾 → 自动提升（prompt→deny / prompt→allow）→ UPSERT
policies 表（source=learned）→ 刷新 tenant `.rules` 文件 → 下一 turn app-server
自动加载。保守、安全单调：deny 永不回退，高危命令（risk=high）即使人反复
approve 也不自动 allow。

## 任务清单

| 任务 | 内容 | 状态 |
|---|---|---|
| T6-1 | migration `20260906000005_m6_policy_learning.sql`：policies +source/learned_from；policy_feedback 表 + 2 索引。db.rs 接线。 | ✅ |
| T6-2 | policy.rs：`extract_pattern`（argv 前2 token + 末尾 `*`）、`record_feedback`、`learn`（阈值 N、一致判定、矛盾比对、保守提升、UPSERT）、`list_feedback`、`list_rules`、`FeedbackRow`/`LearnedRule`/`PolicyRow`。单测 `extract_pattern_prefix`。 | ✅ |
| T6-3 | http_server.rs：AppState +`codex_home`；approval_resolve 扩展取 `(status,turn_id,command,policy_decision,risk_level)` → DB update+audit 后调 `record_feedback`+`learn`+`generate_rules`+`write_tenant_rules`；新增 `GET /v1/policy/feedback`、`GET /v1/policy/rules`。 | ✅ |
| T6-4 | runtime.rs：SIMULATE 命令经 `NEXUS_SIMULATE_COMMAND` env 配置（默认 `rm -rf /tmp/nexus-sim`），使学习 e2e 可用 prompt 类命令（`npm install nexus-sim`）演示。 | ✅ |
| T6-5 | main.rs：标题 `=== Nexus M6: serve ===`；AppState 注入 `codex_home`。 | ✅ |

## 关键决策

1. **保守提升规则**：仅 `prompt→deny` 或 `prompt→allow(if risk≠high)`；`deny` 不
   回退、`allow` 不变、高危不自动 allow。学到的规则 priority=50（介于种子
   deny 80~100 与 allow 10 之间）。
2. **pattern 提取**：argv 前 2 token，token 数 >2 则末尾加 `*`（前缀匹配，与
   policies 表 glob 风格一致）。`npm install nexus-sim`→`npm install*`。
3. **SIMULATE 命令可配置**：默认 deny 种子 `rm -rf*` 与人 deny 一致不触发学习；
   改 `NEXUS_SIMULATE_COMMAND="npm install nexus-sim"`（当前无种子→prompt）→
   3 次 deny 触发学习，可端到端验证闭环。
4. **零侵入 runtime 调度**：M6 不改 DriverPool/DriverGuard/slot 路由核心，仅在
   approval_resolve 路径叠加 feedback+learn，M5 并发零回归。

## 坑

- `extract_pattern` format 串 bug：`"{}* {}"` 产出 `rm* -rf`（token 错位）→
  `"{} {}*"` 产出 `rm -rf*`。单测捕获。
- `policies` 唯一索引 `(tenant_id,role,action_kind,pattern)` 已存（M3 建），
  UPSERT `ON CONFLICT ... DO UPDATE` 覆盖同 pattern 旧 prompt 种子。
- TaskStop 强杀后台 bash 脚本会致 driver 半状态（running turn + pending
  approval 残留，resolve 不解锁因 driver 已 break 出 park 循环）→ 重启服务 +
  `UPDATE turns/approval_tickets SET status='interrupted' WHERE running/pending`
  清理。非代码 bug。

## 验证

- `cargo check -p nexus-control`：0 error 0 warning。
- `cargo test -p nexus-control`：20/20（+1 `extract_pattern_prefix`）。
- e2e（NEXUS_POOL_SIZE=2 + NEXUS_SIMULATE_APPROVAL=1 +
  NEXUS_SIMULATE_COMMAND="npm install nexus-sim"）：见
  `docs/test_report/nexus-m6/TEST_REPORT.html`。
- 零回归：M3 interrupt / M4 计量 / M5 并发 全 PASS。

## 下一步

M7：真实模型经 gateway 联调审批（非 SIMULATE）+ 多 Pod 分布式 driver 池
（Redis 跨 Pod slot 调度）+ per-tenant 独占 slot 隔离 + IM Bot 推送审批卡片
（飞书/钉钉，需 IM sender→userId 解析器）+ execpolicy amendment 协议级回写
（AcceptWithExecpolicyAmendment → policy 合并）。按"审查 M7 方案→自主确认→
开工"循环推进。
