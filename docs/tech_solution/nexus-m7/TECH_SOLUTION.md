# Nexus M7 — execpolicy amendment 协议级回写 技术方案

## 数据流

```
app-server CommandExecutionRequestApproval
  (带 proposed_execpolicy_amendment: Option<ExecPolicyAmendment{command:Vec<String>}>)
  → driver ApprovalRequest 提取 proposed_amendment → ApprovalInfo 携带
  → http_server 落 approval_tickets (+proposed_amendment 列)
  → 人 POST /resolve decision=approve_with_amendment + amendment_command
  → http_server 取 ticket.command + amendment_command → DriverCommand::ResolveApproval
      { decision: ApproveWithAmendment { command } }
  → driver park 解锁：respond_approval 映射 AcceptWithExecpolicyAmendment
      + emit TurnEvent{item_type:"execpolicy/amendment", amendment:command}
  → http_server 收到 amendment 事件 → policy::merge_amendment
      (risk check + UPSERT policies allow/40 + 刷 .rules)
  → turn/completed
```

## 改动点

### stdio_client.rs
- `DecisionInput` += `ApproveWithAmendment { command: Vec<String> }`。
- `ApprovalRequest` += `proposed_amendment: Option<Vec<String>>`；`next_event`
  CommandExecution 分支从 `params.proposed_execpolicy_amendment` 取 `.command`
  （需读 raw_params 或 params 字段；common.rs v2 item.rs 有该字段）。
- `respond_approval` CommandExecution 路径：
  `ApproveWithAmendment { command }` →
  `CommandExecutionApprovalDecision::AcceptWithExecpolicyAmendment {
      execpolicy_amendment: ExecPolicyAmendment { command } }`。

### runtime.rs
- `ApprovalInfo` += `proposed_amendment: Option<Vec<String>>`（透传）。
- `TurnEvent` += `amendment: Option<Vec<String>>`（仅 `execpolicy/amendment` 事件）。
- `park_real_approval` ResolveApproval 分支：若 decision=ApproveWithAmendment，
  respond_approval 后 emit `execpolicy/amendment` TurnEvent（command 来自 decision）。
- SIMULATE：`NEXUS_SIMULATE_AMENDMENT_COMMAND` env（默认 `["ls"]`），SIMULATE
  approval 时 ApprovalInfo.proposed_amendment = 该值；resolve
  ApproveWithAmendment 时 emit 合成 amendment 事件（不走真实 app-server）。
  `park_for_decision` 返回 decision（含 ApproveWithAmendment），driver_loop SIMULATE
  分支据 decision 类型 emit amendment + turn/completed。
- `park_for_decision` 已返回 `Option<DecisionInput>`，ApproveWithAmendment 是
  DecisionInput 变体，自然携带 command。

### http_server.rs
- approval_resolve：解析 body `decision` 字符串，`approve_with_amendment` →
  `DecisionInput::ApproveWithAmendment { command: body.amendment_command }`。
  ticket load 已取 command（M6）。DriverCommand::ResolveApproval 携带。
- drain：新增 `execpolicy/amendment` 事件分支 →
  `policy::merge_amendment(pool, tid, &ev.amendment)` + 刷 rules（同 M6
  approval_resolve 后的刷新逻辑，可抽 helper）。
- approval_tickets migration：+`proposed_amendment JSONB`（可空，审计/前端展示）。
  M7 migration `20260906000006_m7_amendment.sql`。

### policy.rs
- `merge_amendment(pool, tid, command: &[String]) -> Result<Option<LearnedRule>>`：
  pattern = `extract_pattern(&command.join(" "))`（复用 M6）；
  `risk_of(&command.join(" "))`=="high" → return None（拒绝，高危不 allow）；
  否则 UPSERT policies(pattern, decision=allow, source=amendment, priority=40,
  risk_level) ON CONFLICT — **仅当 existing.source != 'seed' OR existing IS NULL
  时覆盖**（seed deny 不被 amendment 降级；learned/seed prompt 可被 amendment
  升级为 allow）。return Some(LearnedRule{pattern, decision:allow})。
- 单测 `merge_amendment_safety`：高危拒绝、seed deny 不覆盖、prompt→allow。

### main.rs
- 标题 "Nexus M7: serve"；migration 接线 m7。

## 安全

- amendment priority=40 < seed deny 80/100：deny 永远优先（evaluate ORDER BY priority DESC）。
- risk_of=high 拒绝 amendment（保守）。
- UPSERT 不覆盖 source=seed 行（ON CONFLICT DO UPDATE 加 `WHERE policies.source!='seed'`）。

## SIMULATE e2e

`NEXUS_SIMULATE_APPROVAL=1 NEXUS_SIMULATE_COMMAND="ls /tmp"
NEXUS_SIMULATE_AMENDMENT_COMMAND='["ls"]'`：
- approval resolve approve_with_amendment(["ls"]) →
  emit execpolicy/amendment → policies(pattern="ls*", decision=allow, source=amendment)
  → tenant-1.rules 含 `prefix_rule(["ls"], "allow")`。
- 高危 amendment（amendment_command=["rm","-rf"]）→ merge_amendment return None（不写入）。

## 验证

- cargo check 0/0；cargo test（+merge_amendment_safety）。
- e2e：amendment 落库 + rules 文件 + GET /v1/policy/rules + 高危拒绝 + 零回归（M6 learn/M5 并发/M4 计量）。

## 自确认

✅ 方案 OK — 聚焦 1 条协议通路，Surgical（只动 6 处），安全单调保留，SIMULATE 可端到端验证。
