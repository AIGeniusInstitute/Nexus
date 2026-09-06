-- M12: evaluation center — eval cases + run results (roadmap M8 T8-1).
-- Governance-phase capstone: asserts on completed turns feed a CI gate.

CREATE TABLE IF NOT EXISTS eval_cases (
    id BIGSERIAL PRIMARY KEY,
    tenant_id BIGINT NOT NULL REFERENCES tenants(id),
    name TEXT NOT NULL,
    category TEXT,
    input TEXT NOT NULL,
    expected_status TEXT NOT NULL DEFAULT 'completed',
    expected_contains TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_eval_cases_tenant ON eval_cases (tenant_id);

CREATE TABLE IF NOT EXISTS eval_runs (
    id BIGSERIAL PRIMARY KEY,
    tenant_id BIGINT NOT NULL REFERENCES tenants(id),
    case_id BIGINT NOT NULL REFERENCES eval_cases(id),
    turn_id BIGINT NOT NULL,
    passed BOOLEAN NOT NULL,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_eval_runs_tenant_time ON eval_runs (tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_eval_runs_case ON eval_runs (case_id, created_at DESC);
