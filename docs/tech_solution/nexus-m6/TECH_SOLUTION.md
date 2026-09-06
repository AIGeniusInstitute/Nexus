# Nexus M6 — 策略自学习闭环 · 技术方案

> 里程碑 M6（Policy Learning Loop）· 2026-09-06 · 分支 `feat/nexus-m6`

## 1. 数据模型

### 1.1 迁移 `20260906000005_m6_policy_learning.sql`

```sql
-- policies 加 source 列（区分种子规则与学习生成规则）
ALTER TABLE policies ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'seed'; -- seed | learned
ALTER TABLE policies ADD COLUMN IF NOT EXISTS learned_from TEXT; -- 触发学习的 feedback pattern

-- 决策反馈流水：每次 approval resolve 记一行
CREATE TABLE IF NOT EXISTS policy_feedback (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    pattern       TEXT NOT NULL,        -- argv 前缀 glob：rm -rf* / npm install*
    decision      TEXT NOT NULL,        -- approve | deny | cancel（人决策）
    policy_rec    TEXT NOT NULL,        -- allow | prompt | deny（决策时 evaluate 推荐）
    risk_level    TEXT,
    turn_id       BIGINT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_policy_feedback_tenant_pattern
  ON policy_feedback(tenant_id, pattern, created_at DESC);
```

### 1.2 pattern 提取
`extract_pattern(command) -> String`：取命令 argv 前 2 token，若命令 token 数 > 2 则末尾加 `*`。
- `rm -rf /tmp/x`（3 token）→ `rm -rf*`
- `npm install nexus-sim`（3 token）→ `npm install*`
- `ls`（1 token）→ `ls`
- `sudo apt update`（3 token）→ `sudo apt*`

与 policies 表 glob 风格一致（`*` 结尾=前缀匹配，无 `*`=精确）。

## 2. 学习规则（保守、单调）

```
learn(pool, tenant_id) -> Vec<LearnedRule>:
  threshold = NEXUS_POLICY_LEARN_THRESHOLD (default 3)
  feedback = SELECT pattern, decision FROM policy_feedback
             WHERE tenant_id=$1 ORDER BY created_at DESC
  group by pattern (Rust HashMap)
  for each pattern with >= threshold recent rows:
    recent_N = first N of (desc-ordered) for this pattern
    if recent_N all same decision D:
      cur = SELECT decision FROM policies WHERE tenant_id AND pattern=P AND enabled LIMIT 1
            (None → "prompt" default)
      promote = match (cur, D):
        (prompt, deny)        → Some("deny")     # 人反复拒 → 自动禁
        (prompt, approve)     → if risk != "high": Some("allow") else None  # 高危不自动放
        _                     → None             # deny 不回退，allow 不变
      if let Some(new_dec) = promote:
        # UPSERT：覆盖同 pattern 的旧 prompt 种子（dedup 已有唯一索引）
        INSERT INTO policies(tenant_id, role='*', action_kind='command_execution',
            pattern=P, risk_level=cur_risk, decision=new_dec, priority=50,
            source='learned', learned_from=P)
        ON CONFLICT (tenant_id, role, action_kind, pattern) DO UPDATE
          SET decision=EXCLUDED.decision, source='learned',
              learned_from=EXCLUDED.learned_from, priority=50, enabled=TRUE
        learned.push(P)
  return learned
```

**安全单调性**：deny 永不回退到 allow/prompt；高危命令即使人反复 approve 也不自动 allow。

## 3. 接线

### 3.1 approval_resolve（http_server.rs）
扩展 ticket 加载字段：`SELECT status, turn_id, command, policy_decision, risk_level`。
resolve 后：
1. `policy::record_feedback(pool, tid, pattern, new_status, policy_rec, risk, turn_id)` —— pattern_rec 取 ticket.policy_decision（M4 已在 requested 时落库），pattern = extract_pattern(command)。
2. `let learned = policy::learn(pool, tid).await?` —— 若非空，`generate_rules` + `write_tenant_rules` 刷新 tenant-{id}.rules（下一 turn 自动加载）。

### 3.2 可观测 API
- `GET /v1/policy/feedback?days=7` → `Vec<FeedbackRow>`（pattern, decision, policy_rec, risk, created_at）。
- `GET /v1/policy/rules` → `Vec<PolicyRow>`（pattern, decision, source, priority, enabled）。

### 3.3 SIMULATE 命令可配（runtime.rs）
`NEXUS_SIMULATE_COMMAND`（default `rm -rf /tmp/nexus-sim`）注入合成 approval 的 command/cwd。M6 测试设为 `npm install nexus-sim`（当前 prompt，无种子）→ 3 次 deny → 学习 `npm install*` deny。

## 4. 任务分解

| 任务 | 文件 | 内容 |
|------|------|------|
| T6-1 | `migrations/...m6...sql` + `db.rs` | policy_feedback 表 + policies.source 列 + 迁移接线 |
| T6-2 | `policy.rs` | extract_pattern + record_feedback + learn + list_feedback/list_rules |
| T6-3 | `http_server.rs` | approval_resolve 记反馈+学习+刷 rules；2 GET 端点 |
| T6-4 | `runtime.rs` | NEXUS_SIMULATE_COMMAND env 可配 |
| T6-5 | `main.rs` | 标题 M6 |
| T6-6 | e2e | 3 次 deny（prompt 命令）→ 学习 → rules 文件含新 deny 规则 + 零回归 |

## 5. 验证

| 验证 | 方法 |
|------|------|
| cargo check/test | 0 error 0 warning；单测 extract_pattern + learn 逻辑 |
| e2e 学习 | NEXUS_SIMULATE_COMMAND="npm install nexus-sim" + 3 turn deny → GET /v1/policy/rules 含 `npm install*` deny (source=learned) → rules 文件 prefix_rule ["npm","install"] forbidden |
| e2e 反馈 | GET /v1/policy/feedback 返回 3 行 deny |
| 零回归 | SIMULATE approve/interrupt + M4 计量 + M5 并发不变 |

## 6. 自审

- [x] 不改 codex-rs 内核
- [x] 学习是叠加（不改 evaluate/park/resolve 主流程，只追加 record+learn）
- [x] 安全单调（deny 不回退，高危不自动 allow）
- [x] Surgical：只动 policy.rs(追加) + http_server.rs(resolve 追加 + 2 端点) + runtime.rs(env) + migration
- [x] 可 SIMULATE 验证（无需真实模型）

## 7. 方案自确认

✅ 方案 OK。保守学习规则 + env 可配 SIMULATE 命令 + 叠加式接线。开工 T6-1。
