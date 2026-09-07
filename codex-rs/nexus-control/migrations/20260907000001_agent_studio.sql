-- M Agent Studio: Agent 定义 + thread 绑定 + delta 流式转发支持。
-- agent_definitions: 可复用 Agent 模板（system_prompt + model），thread 创建时绑定。
-- threads.agent_def_id: 关联到 Agent 定义，turn_start 读 system_prompt 注入 codex base_instructions。
-- 幂等：CREATE TABLE IF NOT EXISTS + ALTER ADD COLUMN IF NOT EXISTS（重启安全）。

CREATE TABLE IF NOT EXISTS agent_definitions (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    name          TEXT NOT NULL,
    description   TEXT,
    system_prompt TEXT,
    model         TEXT,
    created_by    BIGINT REFERENCES users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_agent_defs_tenant ON agent_definitions(tenant_id);

ALTER TABLE threads ADD COLUMN IF NOT EXISTS agent_def_id BIGINT REFERENCES agent_definitions(id);
