-- Nexus M4: 产物与计量 (Artifacts & Metering)
-- 2026-09-06

-- M4-1: 多租户并发上限（turn_start 前置门控，防同租户请求积压）。
ALTER TABLE tenants
    ADD COLUMN IF NOT EXISTS max_concurrent_turns INT NOT NULL DEFAULT 1;

-- M4-2: 审批策略推荐列（evaluate() 在 surface 时写入；人仍最终决策）。
-- risk_level 列在 initial 迁移已存在（default 'low'），M4 surface 时填 risk_of()。
ALTER TABLE approval_tickets
    ADD COLUMN IF NOT EXISTS policy_decision TEXT;

-- M4-3: 模型定价表（cost 推导；未知模型 → 0，不误计费）。
CREATE TABLE IF NOT EXISTS model_pricing (
    model                 TEXT PRIMARY KEY,
    input_rate_per_mtok   NUMERIC(12, 6) NOT NULL DEFAULT 0,
    output_rate_per_mtok  NUMERIC(12, 6) NOT NULL DEFAULT 0,
    currency              TEXT NOT NULL DEFAULT 'USD',
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO model_pricing (model, input_rate_per_mtok, output_rate_per_mtok) VALUES
    ('nexus-gateway-mock', 0,      0),
    ('gpt-4o',              2.50,   10.00),
    ('claude-sonnet',       3.00,   15.00)
ON CONFLICT (model) DO NOTHING;

-- usage_records 表在 initial 迁移已建（tenant_id/user_id/thread_id/turn_id/model/
-- input_tokens/output_tokens/cost_micros/recorded_at + idx_usage_tenant），
-- M4 首次写入（turn 完成时 record_usage）。
