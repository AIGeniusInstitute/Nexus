-- M18 多 Agent 协作编排
-- roadmap T11-4。协调层位于 turn 之上，记录编排运行 + 每步 agent。

CREATE TABLE IF NOT EXISTS orchestrations (
    id BIGSERIAL PRIMARY KEY,
    tenant_id BIGINT NOT NULL,
    name TEXT,
    mode TEXT NOT NULL CHECK (mode IN ('orchestrator-worker','peer','critic-adversarial')),
    status TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running','completed','failed')),
    prompt TEXT,
    created_by BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_orch_tenant_time ON orchestrations(tenant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS orchestration_agents (
    id BIGSERIAL PRIMARY KEY,
    orchestration_id BIGINT NOT NULL REFERENCES orchestrations(id) ON DELETE CASCADE,
    tenant_id BIGINT NOT NULL,
    thread_id UUID NOT NULL,
    agent_seq INT NOT NULL,
    role TEXT NOT NULL,
    turn_id BIGINT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','running','completed','failed')),
    output_ref TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_orchagent_orch_seq ON orchestration_agents(orchestration_id, agent_seq);
