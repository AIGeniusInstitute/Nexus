-- Nexus M1 initial schema: 26 domain entities + 2 gateway tables.
-- All tables created once (schema stable foundation); M1 logic touches only
-- identity + threads/turns/items + idempotency. Others are empty shells for M2-M8.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ============ 身份域 (T1-1) ============
CREATE TABLE IF NOT EXISTS tenants (
    id           BIGSERIAL PRIMARY KEY,
    name         TEXT NOT NULL,
    slug         TEXT NOT NULL UNIQUE,
    status       TEXT NOT NULL DEFAULT 'active',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS users (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    email         TEXT NOT NULL,
    password_hash TEXT,
    idp_subject   TEXT,
    display_name  TEXT,
    status        TEXT NOT NULL DEFAULT 'active',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, email)
);

CREATE TABLE IF NOT EXISTS roles (
    id              BIGSERIAL PRIMARY KEY,
    tenant_id       BIGINT NOT NULL REFERENCES tenants(id),
    name            TEXT NOT NULL,
    permissions_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    scope           TEXT NOT NULL DEFAULT 'tenant',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, name)
);

CREATE TABLE IF NOT EXISTS tenant_memberships (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   BIGINT NOT NULL REFERENCES tenants(id),
    user_id     BIGINT NOT NULL REFERENCES users(id),
    role_id     BIGINT NOT NULL REFERENCES roles(id),
    scope_json  JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, user_id, role_id)
);

CREATE TABLE IF NOT EXISTS workspaces (
    id               BIGSERIAL PRIMARY KEY,
    tenant_id        BIGINT NOT NULL REFERENCES tenants(id),
    name             TEXT NOT NULL,
    sandbox_mode     TEXT NOT NULL DEFAULT 'readonly',
    approval_policy  TEXT NOT NULL DEFAULT 'never',
    repos_json       JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS environments (
    id            BIGSERIAL PRIMARY KEY,
    workspace_id  BIGINT NOT NULL REFERENCES workspaces(id),
    name          TEXT NOT NULL,
    vars_json     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============ 会话五原语 ============
CREATE TABLE IF NOT EXISTS threads (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                BIGINT NOT NULL REFERENCES tenants(id),
    workspace_id             BIGINT REFERENCES workspaces(id),
    owner_user_id            BIGINT NOT NULL REFERENCES users(id),
    codex_thread_id          TEXT,
    title                    TEXT,
    status                   TEXT NOT NULL DEFAULT 'active',
    rollout_object_key       TEXT,
    permission_snapshot_hash TEXT,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_threads_tenant ON threads(tenant_id);
CREATE INDEX IF NOT EXISTS idx_threads_owner ON threads(owner_user_id);

CREATE TABLE IF NOT EXISTS turns (
    id            BIGSERIAL PRIMARY KEY,
    thread_id     UUID NOT NULL REFERENCES threads(id),
    status        TEXT NOT NULL DEFAULT 'pending',
    input_tokens  INT NOT NULL DEFAULT 0,
    output_tokens INT NOT NULL DEFAULT 0,
    cost_micros   BIGINT NOT NULL DEFAULT 0,
    model         TEXT,
    started_at    TIMESTAMPTZ,
    completed_at  TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_turns_thread ON turns(thread_id);

CREATE TABLE IF NOT EXISTS items (
    id            BIGSERIAL PRIMARY KEY,
    thread_id     UUID NOT NULL REFERENCES threads(id),
    turn_id       BIGINT NOT NULL REFERENCES turns(id),
    seq           BIGINT NOT NULL,
    item_type     TEXT NOT NULL,
    content_ref   TEXT,
    content_digest TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (thread_id, turn_id, seq)
);
CREATE INDEX IF NOT EXISTS idx_items_thread_turn ON items(thread_id, turn_id);

CREATE TABLE IF NOT EXISTS steps (
    id              BIGSERIAL PRIMARY KEY,
    item_id         BIGINT NOT NULL REFERENCES items(id),
    sample_status   TEXT,
    tool_calls_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS workspace_snapshots (
    id                 BIGSERIAL PRIMARY KEY,
    thread_id          UUID NOT NULL REFERENCES threads(id),
    turn_id            BIGINT REFERENCES turns(id),
    rollout_object_key TEXT,
    content_digest     TEXT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============ 治理域 (DDL ready, M3-M4) ============
CREATE TABLE IF NOT EXISTS approval_tickets (
    id          BIGSERIAL PRIMARY KEY,
    thread_id   UUID NOT NULL REFERENCES threads(id),
    turn_id     BIGINT REFERENCES turns(id),
    risk_level  TEXT NOT NULL DEFAULT 'low',
    decided_by  BIGINT REFERENCES users(id),
    decided_at  TIMESTAMPTZ,
    decision    TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS usage_records (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    user_id       BIGINT REFERENCES users(id),
    thread_id     UUID REFERENCES threads(id),
    turn_id       BIGINT REFERENCES turns(id),
    model         TEXT,
    input_tokens  INT NOT NULL DEFAULT 0,
    output_tokens INT NOT NULL DEFAULT 0,
    cost_micros   BIGINT NOT NULL DEFAULT 0,
    recorded_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_usage_tenant ON usage_records(tenant_id, recorded_at);

CREATE TABLE IF NOT EXISTS quotas (
    id           BIGSERIAL PRIMARY KEY,
    tenant_id    BIGINT NOT NULL REFERENCES tenants(id),
    scope        TEXT NOT NULL,
    period       TEXT NOT NULL,
    limit_micros BIGINT NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS budgets (
    id           BIGSERIAL PRIMARY KEY,
    tenant_id    BIGINT NOT NULL REFERENCES tenants(id),
    scope        TEXT NOT NULL,
    hard_limit   BIGINT NOT NULL DEFAULT 0,
    spent_micros BIGINT NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============ 执行面 (M2) ============
CREATE TABLE IF NOT EXISTS sandbox_pods (
    id           BIGSERIAL PRIMARY KEY,
    workspace_id BIGINT REFERENCES workspaces(id),
    pod_name     TEXT,
    status       TEXT NOT NULL DEFAULT 'pending',
    started_at   TIMESTAMPTZ,
    terminated_at TIMESTAMPTZ
);

-- ============ 模型 / MCP (M2) ============
CREATE TABLE IF NOT EXISTS model_routes (
    id         BIGSERIAL PRIMARY KEY,
    tenant_id  BIGINT NOT NULL REFERENCES tenants(id),
    name       TEXT NOT NULL,
    provider   TEXT NOT NULL,
    model      TEXT NOT NULL,
    fallback   JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS model_credentials (
    id             BIGSERIAL PRIMARY KEY,
    tenant_id      BIGINT NOT NULL REFERENCES tenants(id),
    name           TEXT NOT NULL,
    vault_cred_ref TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS mcp_servers (
    id             BIGSERIAL PRIMARY KEY,
    tenant_id      BIGINT NOT NULL REFERENCES tenants(id),
    name           TEXT NOT NULL,
    transport      TEXT NOT NULL,
    url            TEXT,
    tool_whitelist JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS mcp_credentials (
    id             BIGSERIAL PRIMARY KEY,
    mcp_server_id  BIGINT NOT NULL REFERENCES mcp_servers(id),
    vault_cred_ref TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============ 知识库 / 技能 / 连接器 ============
CREATE TABLE IF NOT EXISTS knowledge_bases (
    id           BIGSERIAL PRIMARY KEY,
    workspace_id BIGINT REFERENCES workspaces(id),
    name         TEXT NOT NULL,
    acl_json     JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS skills (
    id         BIGSERIAL PRIMARY KEY,
    tenant_id  BIGINT REFERENCES tenants(id),
    scope      TEXT NOT NULL DEFAULT 'tenant',
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS skill_versions (
    id           BIGSERIAL PRIMARY KEY,
    skill_id     BIGINT NOT NULL REFERENCES skills(id),
    version      TEXT NOT NULL,
    checksum     TEXT NOT NULL,
    content_ref  TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS connectors (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   BIGINT NOT NULL REFERENCES tenants(id),
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    cred_ref    TEXT,
    config_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============ 审计 (M1 普通表; M8 加 PARTITION BY RANGE) ============
CREATE TABLE IF NOT EXISTS audit_logs (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    actor_user_id BIGINT REFERENCES users(id),
    action        TEXT NOT NULL,
    resource      TEXT,
    detail_json   JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_audit_tenant ON audit_logs(tenant_id, created_at);

CREATE TABLE IF NOT EXISTS tool_call_logs (
    id          BIGSERIAL PRIMARY KEY,
    thread_id   UUID REFERENCES threads(id),
    turn_id     BIGINT REFERENCES turns(id),
    tool_name   TEXT NOT NULL,
    args_json   JSONB NOT NULL DEFAULT '{}'::jsonb,
    result_ref  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============ Gateway 自建 (M1) ============
CREATE TABLE IF NOT EXISTS idempotency_records (
    key           TEXT PRIMARY KEY,
    tenant_id     BIGINT REFERENCES tenants(id),
    user_id       BIGINT REFERENCES users(id),
    response_json JSONB NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at    TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_idem_expires ON idempotency_records(expires_at);

CREATE TABLE IF NOT EXISTS rate_limit_buckets (
    id             BIGSERIAL PRIMARY KEY,
    tenant_id      BIGINT REFERENCES tenants(id),
    user_id        BIGINT REFERENCES users(id),
    bucket_key     TEXT NOT NULL,
    tokens         DOUBLE PRECISION NOT NULL DEFAULT 0,
    last_refill_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (bucket_key)
);

-- ============ M0 events 迁移 (file-backed → Postgres) ============
CREATE TABLE IF NOT EXISTS app_server_events (
    id          BIGSERIAL PRIMARY KEY,
    thread_id   UUID NOT NULL REFERENCES threads(id),
    turn_id     BIGINT,
    seq         BIGINT NOT NULL,
    event_json  JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (thread_id, seq)
);
CREATE INDEX IF NOT EXISTS idx_events_thread ON app_server_events(thread_id, seq);

-- ============ Seed: default 租户 + admin 角色 ============
INSERT INTO tenants (name, slug) VALUES ('default', 'default')
    ON CONFLICT (slug) DO NOTHING;

INSERT INTO roles (tenant_id, name, permissions_json, scope)
SELECT t.id, 'admin', '["*:*"]'::jsonb, 'tenant'
FROM tenants t WHERE t.slug = 'default'
ON CONFLICT (tenant_id, name) DO NOTHING;
