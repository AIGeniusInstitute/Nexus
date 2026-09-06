# Nexus M6 — 策略自学习闭环 · PRD

> 里程碑 M6（Policy Learning Loop）· 2026-09-06 · 分支 `feat/nexus-m6`

## 1. 背景

Nexus 定位为"自进化智能体引擎"——持续从用户反馈中进化。M3 引入 HITL 审批（人决策每条命令），M4 给审批加了策略推荐（evaluate）+ 风险标注。但**策略是静态的**：人每次都对 `rm -rf` 点 deny，下次同样 `rm -rf` 仍 prompt 问人——系统没有从人的决策中学习。

M6 闭环这个缺口：**记录人的审批决策 → 累计 N 次一致决策且与当前策略矛盾 → 自动提升策略（prompt→deny / prompt→allow）→ 写回 tenant rules 文件 → 下一 turn 自动加载**。这是"自进化"定位的最小可验证闭环。

## 2. 目标

| # | 目标 | 价值 |
|---|------|------|
| G1 | 决策反馈落库 | 每次 approval resolve 记录 (tenant, pattern, decision, policy_rec, risk, ts) 到 `policy_feedback` 表 |
| G2 | 学习推理 | `policy::learn()` 分析反馈：N 次连续一致决策且与当前策略矛盾 → 生成提升建议 |
| G3 | 策略自动提升 | 提升后的规则写入 `policies` 表（status=learned）+ 刷新 tenant rules 文件 |
| G4 | 可观测 | `GET /v1/policy/feedback` + `GET /v1/policy/rules` 查看学习状态 |
| G5 | 零回归 | M3/M4/M5 审批/计量/并发全部不变；学习是"叠加"非"替换" |

## 3. 学习规则（保守）

| 当前策略 | 人累计 N 次决策 | 自动提升到 | 理由 |
|----------|----------------|-----------|------|
| prompt | deny（一致） | **deny**（forbidden） | 人反复拒绝 → 危险，自动禁 |
| prompt | approve（一致，risk=low/medium） | **allow** | 人反复放行 → 低危，自动放 |
| prompt | approve（risk=high） | 不提升（保持 prompt） | 高危命令即使人反复放行也不自动 allow，安全兜底 |
| deny | 任何 | 不放松 | 永不从 deny 回退到 allow（安全单调） |
| allow | 任何 | 不变 | allow 已最宽 |

- **N = 3**（可配 `NEXUS_POLICY_LEARN_THRESHOLD`，default 3）：连续 3 次一致才提升。
- **"连续"**：按时间序最近 N 次该 pattern 的决策全一致（不允许中间有相反决策）。
- **pattern = command 的 argv[0] + 前 2 token**（与 M4 generate_rules 的 glob→argv 翻译一致，如 `rm -rf`）。
- 提升后的规则 status=`learned`（区别于种子规则的 `seed`），便于审计/回滚。

## 4. MVP 范围

| 范围 | 说明 |
|------|------|
| ✅ M6 | G1–G5 |
| ⏭ M7+ | 真实模型联调、IM Bot 推送、多 Pod 分布式、amendment 协议级回写 |

## 5. 验收标准

### AC6.1 反馈落库
- approval resolve 时，`policy_feedback` 表写一行：(id, tenant_id, pattern, decision, policy_rec, risk_level, turn_id, created_at)。
- pattern 从 ticket.command 提取（argv[0]+前2 token）。

### AC6.2 学习推理 + 自动提升
- `policy::learn(pool, tenant_id)`：对每个 pattern，查最近 N 次反馈；若全一致且与当前 `policies` 表该 pattern 的决策矛盾且可提升 → INSERT 新 `policies` 行（pattern, decision=`deny`/`allow`, status=`learned`，priority 高于 `prompt` 种子）+ 删除旧 `prompt` 种子（dedup 机制 M3 已有）。
- learn() 在 approval resolve 后调用。

### AC6.3 rules 文件刷新
- 提升后调 `policy::generate_rules()` + `write_tenant_rules()`（M4 已有）刷新 tenant-{id}.rules。
- 下一 turn 的 app-server 自动加载新规则。

### AC6.4 可观测 API
- `GET /v1/policy/feedback?days=7`：最近反馈列表。
- `GET /v1/policy/rules`：当前 tenant 的 policies 表（种子 + learned）。

### AC6.5 零回归
- SIMULATE approve/deny/interrupt + M4 计量 + M5 并发 全部不变。
- cargo check/test 不回归。

## 6. 非目标

- 协议级 AcceptWithExecpolicyAmendment 回写（M7+，需验证 app-server 协议支持）。
- 跨租户策略共享 / 策略市场（M7+）。
- 学习模型 / ML（当前是规则统计，非 ML）。

## 7. 风险

| 风险 | 缓解 |
|------|------|
| 误提升（噪声决策导致错误自动 deny/allow） | N=3 连续一致门槛 + 高危不自动 allow + deny 单调不回退 |
| pattern 提取过粗/过细 | argv[0]+前2 token，与 M4 rules 翻译一致；过粗则 `*` catch-all 仍走 prompt |
| learned 规则与种子冲突 | M3 policies 表已有 dedup（uq_policies_role_kind_pattern），learned 行覆盖旧 prompt 种子 |
| 持久化 rules 文件并发写 | write_tenant_rules 已用 tmp+rename 原子写（M4） |
