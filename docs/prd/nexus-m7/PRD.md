# Nexus M7 — execpolicy amendment 协议级回写

## 背景

M6 的策略自学习靠"累计 N 次人决策"提升规则（统计驱动）。codex app-server
协议本身还有一条**协议级**的 amendment 通路：app-server 在发起命令审批请求
`CommandExecutionRequestApproval` 时，可附带 `proposed_execpolicy_amendment:
Option<ExecPolicyAmendment { command: Vec<String> }>`（建议"此后匹配此前缀
的命令免审批"）。人可回 `AcceptWithExecpolicyAmendment { execpolicy_amendment }`
——批准本命令 + 把此前缀加入 execpolicy 永久 allow。

M7 把这条协议级 amendment 接到 Nexus 策略层：driver 把人接受的 amendment
回写到 `policies` 表（source=amendment）+ 刷新 tenant `.rules`，下一 turn
自动生效。这是比 M6 统计学习更直接、单次即生效的策略进化通路。

## 范围（MVP，自包含）

- **T7-1 协议接线**：`DecisionInput` +`ApproveWithAmendment { command: Vec<String> }`；
  `ApprovalRequest` +`proposed_amendment: Option<Vec<String>>`（从 raw_params 提取
  `proposed_execpolicy_amendment.command`）；`ApprovalInfo` 携带 proposed_amendment；
  `respond_approval` CommandExecution 路径映射 `ApproveWithAmendment`→
  `CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment`。
- **T7-2 driver amendment 事件**：approval resolve 为 ApproveWithAmendment 时，
  除写回 JSON-RPC 响应外，emit `execpolicy/amendment` TurnEvent（带 command 前缀）。
  SIMULATE 模式：注入合成 proposed amendment（`NEXUS_SIMULATE_AMENDMENT_COMMAND`
  env，默认 `["ls"]`），resolve ApproveWithAmendment → emit 合成 amendment 事件。
- **T7-3 policy 合并 + 下发**：`policy::merge_amendment(pool, tid, command)` —
  pattern = command tokens join(" ") + 末尾 `*`（与 policies glob 一致），
  `risk_of` 检查：高危命令（rm -rf/sudo/mkfs/dd/curl|sh）拒绝 amendment（保守，
  不 allow）；否则 UPSERT policies(pattern, decision=allow, source=amendment,
  priority=40) + 刷 tenant .rules。http_server 收到 `execpolicy/amendment` 事件
  调 merge_amendment。
- **T7-4 决策 API**：`POST /v1/approvals/{id}/resolve` 接受
  `decision=approve_with_amendment` + `amendment_command: [String]`。
- **T7-5 可观测**：`GET /v1/policy/rules` 已列 source=amendment 规则（复用 M6）；
  `GET /v1/policy/amendments` 列 amendment 落库历史（policy_feedback 加
  amendment 标记 OR 复用 list_rules filter source=amendment）。

## 非目标（留 M8+）

- per-tenant 独占 slot 隔离（M4 max_concurrent_turns 已防饿死，M5 全局池已并发；
  真正隔离需求待多租户真实场景驱动，避免 speculative）。
- Redis 跨 Pod slot 调度（需多 Pod 环境；单进程 free-list 已够 MVP）。
- 真实模型经 gateway 联调（需 model API key 凭证）。
- IM Bot 推送审批卡片（需 bot token + sender→userId 解析器）。

## 安全约束

- amendment allow 的 priority=40，低于 deny 种子（80~100），deny 永远优先。
- `risk_of(command)`=high 的命令拒绝 amendment（即使 app-server 提议 + 人接受，
  也不自动 allow 高危）——保守，与 M6 安全单调一致。
- amendment 不回退已有 deny（UPSERT ON CONFLICT 仅当 source 非 seed 时覆盖；
  seed deny 不被 amendment 降级）。

## 验收（AC7.1 ~ AC7.4）

- AC7.1 SIMULATE resolve approve_with_amendment → `execpolicy/amendment` 事件落库
  policies(pattern=ls* , decision=allow, source=amendment)。
- AC7.2 刷新后 tenant-1.rules 含 `prefix_rule(["ls"], "allow")`。
- AC7.3 `GET /v1/policy/rules` 返回 amendment 规则（source=amendment）。
- AC7.4 安全：高危命令（`rm -rf`）的 amendment 被拒绝（不写入 allow）；
  deny 种子不被 amendment 覆盖。
- AC7.5 零回归：M6 学习（3 deny→learn）+ M5 并发 + M4 计量不变。

## 自审

- ✅ Simplicity First：聚焦 1 条协议通路，不做 speculative 的 per-tenant slot/
  Redis 骨架（留 M8 真实场景驱动）。
- ✅ Surgical Changes：只动 DecisionInput/ApprovalRequest/ApprovalInfo/
  respond_approval/driver park/http_server resolve/policy merge_amendment；
  不碰 DriverPool 调度核心。
- ✅ 安全单调：amendment 不覆盖 deny、高危不 allow。
