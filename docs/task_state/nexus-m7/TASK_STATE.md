# Nexus M7 execpolicy amendment 协议级回写 — 任务状态

| 字段 | 值 |
|---|---|
| 分支 | `feat/nexus-m7` |
| 里程碑 | M7 = execpolicy amendment 协议级回写 |
| 任务 | T7-1 ~ T7-5 |
| 起始基线 | M6（merge 6a01801） |
| 状态 | ✅ 全部完成，e2e + 安全 + 零回归全过 |

## 目标

接通 codex app-server 协议级 amendment 通路：app-server 发命令审批请求时
可带 `proposed_execpolicy_amendment: Option<ExecPolicyAmendment{command:Vec<String>}>`
（建议"此后匹配此前缀的命令免审批"）。人回 `AcceptWithExecpolicyAmendment`
→ driver 写回 JSON-RPC 响应 + emit `execpolicy/amendment` 事件 → http_server
调 `policy::merge_amendment` UPSERT policies(source=amendment, allow, pri=40)
+ 刷 tenant `.rules` → 下一 turn 自动加载。这是比 M6 统计学习更直接、
单次即生效的策略进化通路。保守：高危命令拒绝 amendment；seed deny 不被
amendment 降级。

## 任务清单

| 任务 | 内容 | 状态 |
|---|---|---|
| T7-1 | stdio_client.rs：`DecisionInput`+`ApproveWithAmendment{command}`；`ApprovalRequest`+`proposed_amendment`；`next_event` 提取 `proposed_execpolicy_amendment.command`；`respond_approval` 映射 `AcceptWithExecpolicyAmendment`。 | ✅ |
| T7-2 | runtime.rs：`ApprovalInfo`+`proposed_amendment`；`TurnEvent`+`amendment`；SIMULATE `NEXUS_SIMULATE_AMENDMENT_COMMAND` 注入合成 proposed amendment；resolve ApproveWithAmendment 时 emit `execpolicy/amendment`（SIMULATE + real park 两路径）。 | ✅ |
| T7-3 | policy.rs：`merge_amendment`（pattern=join+"*"，risk=high 拒绝，UPSERT WHERE source!='seed' 不覆盖 seed，priority=40）。 | ✅ |
| T7-4 | http_server.rs：`ResolveReq`+`amendment_command`；decision match 加 `approve_with_amendment`；drain `execpolicy/amendment` 事件分支 → merge_amendment + 刷 rules。 | ✅ |
| T7-5 | main.rs 标题 M7；无 schema 变更（复用 M6 source 列，source='amendment'）。 | ✅ |

## 关键决策

1. **聚焦 1 条协议通路**（Simplicity First）：per-tenant slot 隔离 / Redis
   多 Pod 留 M8（需真实多租户/多 Pod 场景驱动，避免 speculative 过度工程）。
2. **无 schema 变更**：amendment 复用 M6 加的 policies.source 列
   （source='amendment'），无需新 migration。
3. **pattern 总为前缀**：`format!("{}*", command.join(" "))`，`["git","clone"]`
   →`git clone*`（amendment 语义=允许此前缀所有命令，非精确匹配）。
4. **安全单调**：amendment priority=40 < seed deny 80~100（deny 永远优先）；
   risk_of=high 拒绝（rm -rf/sudo/mkfs/dd/curl|sh）；UPSERT `WHERE source!='seed'`
   确保 seed deny 不被降级。

## 坑

- amendment pattern 初版误用 `extract_pattern`（2 token 不加 `*` → "git clone"
  精确匹配，语义错）→ 改 `format!("{}*", join)` 总前缀。
- GET /v1/policy/rules filter 用 "git clone*"，但 DB 存 "git clone"（无 *）
  时假报 (none)——是 pattern 构造 bug 的暴露，修复后正确。
- `CommandExecutionRequestApprovalParams.proposed_execpolicy_amendment` 字段
  需在 move 其他字段前 `.as_ref().map(|a| a.command.clone())` 借用提取。

## 验证

- `cargo check -p nexus-control`：0 error 0 warning。
- `cargo test -p nexus-control`：20/20（零回归）。
- e2e（NEXUS_POOL_SIZE=2 + SIMULATE + SIMULATE_COMMAND="git clone x"）：
  - AC7.1 resolve approve_with_amendment ["git","clone"] → turn completed；
  - AC7.2 tenant-1.rules 含 `prefix_rule(["git","clone"], "allow")`；
  - AC7.3 GET /v1/policy/rules 含 `git clone* allow amendment 40`；
  - AC7.4 rm -rf amendment 被拒（rm -rf*/rm* stays seed deny，无 amendment allow）；
  - AC7.5 M6 npm install* deny learned 仍存在（零回归）。

## 下一步

M8：真实模型经 gateway 联调审批（非 SIMULATE，需 model API key 凭证）+
多 Pod 分布式 driver 池（Redis 跨 Pod slot 调度，需多 Pod 环境）+
per-tenant 独占 slot 隔离（真实多租户场景驱动）+ IM Bot 推送审批卡片
（飞书/钉钉，需 bot token + sender→userId 解析器）。按"审查 M8 方案→
自主确认→开工"循环推进；凭证依赖项待用户提供。
