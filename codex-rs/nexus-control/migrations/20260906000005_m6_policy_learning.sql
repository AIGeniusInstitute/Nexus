-- Nexus M6: 策略自学习闭环。
-- 1. policies 加 source 列（区分种子 seed 规则与学习生成的 learned 规则）。
-- 2. policy_feedback：每次 approval resolve 记一行决策反馈，learn() 据此提升策略。

-- 1. policies 扩展列（幂等）
ALTER TABLE policies ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'seed'; -- seed | learned
ALTER TABLE policies ADD COLUMN IF NOT EXISTS learned_from TEXT; -- 触发该 learned 规则的 feedback pattern

-- 2. policy_feedback 决策反馈流水
CREATE TABLE IF NOT EXISTS policy_feedback (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    pattern       TEXT NOT NULL,            -- argv 前缀 glob：rm -rf* / npm install*
    decision      TEXT NOT NULL,            -- approve | deny | cancel（人的决策）
    policy_rec    TEXT NOT NULL,            -- allow | prompt | deny（决策时 evaluate 推荐）
    risk_level    TEXT,
    turn_id       BIGINT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_policy_feedback_tenant_pattern
  ON policy_feedback(tenant_id, pattern, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_policy_feedback_tenant_time
  ON policy_feedback(tenant_id, created_at DESC);
