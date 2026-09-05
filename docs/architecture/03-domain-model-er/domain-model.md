# Nexus 领域模型 ER 设计方案

> 产物编号：任务二-3 · 领域模型与 ER 图
> 基座：`~/Nexus`（基于 OpenAI Codex Harness，codex-rs 111+ crate Rust 工作区）
> 日期：2026-09-06 · 配套图：`domain-model-core.svg` / `domain-model-governance.svg` / `domain-model.svg` / `.png` · 交互报告：`domain-model-report.html`

---

## 0. 领域模型设计原则

| # | 原则 | 说明 |
|---|---|---|
| 1 | **事件即真相** | 所有对用户可见状态来自 app-server 事件流；控制面消费事件写 Postgres，Pod 死不丢会话。`items` 表是事件流的持久化形态，`thread_id + turn_id + seq` 三元组构成幂等键，重复投递直接丢弃 |
| 2 | **幂等键 thread_id + turn_id + item_seq** | `items` 表 `UNIQUE(thread_id, turn_id, seq)`——同一个 Turn 内同序号的事件只能写入一次，支持 at-least-once 投递语义下的精确去重 |
| 3 | **RLS 租户隔离** | 所有业务表强制 Row-Level Security 策略 `tenant_id = current_setting('app.tenant_id')`；即使应用层 ORM 漏加 `WHERE tenant_id=?`，Postgres 层兜底拒绝跨租户访问 |
| 4 | **WORM 审计** | `audit_logs` 与 `usage_records` 只追加；应用账号无 `UPDATE`/`DELETE` 权限（独立角色写入，审计账号只读）；`audit_logs` 按月分区 + 投递 SIEM |
| 5 | **大字段外置** | `items.content_ref`、`approval_tickets.params_ref/diff_preview_ref`、`audit_logs.before_ref/after_ref` 均指向对象存储（S3/MinIO），主表只存摘要与 digest，避免 PG 膨胀 |
| 6 | **权限快照防漂移** | `threads.permission_snapshot_hash` 记录创建时的权限边界哈希；事后可证明"任务运行时权限是什么"，防止事后改权限导致无法审计 |
| 7 | **配置即政策** | 租户差异不落数据模型，而是下发的 `config.toml` + `execpolicy` 规则集；模型表只存储"策略元数据"，运行时配置由控制面动态生成注入沙箱 |

---

## 1. 核心实体清单

| 实体 | 所属域 | 主键 | 关键字段 | 说明 |
|---|---|---|---|---|
| `tenants` | 租户域 | id | name, plan, isolation_tier, cmk_id, quota_profile | 多租户根节点；isolation_tier 决定隔离级别（shared/dedicated/kata）；cmk_id 绑定按租户加密密钥 |
| `users` | 租户域 | id | tenant_id, idp_subject, email | 平台用户；idp_subject 对接 OIDC/SAML IdP |
| `roles` | 租户域 | id | tenant_id, name, permissions_json | 角色定义；permissions_json 存权限矩阵 |
| `tenant_memberships` | 租户域 | id | tenant_id, user_id, org_unit_id, role_id, scope_json | 用户↔角色 M:N 关联表；scope_json 限定作用范围 |
| `workspaces` | 工作区域 | id | tenant_id, name, env_tag, repos_json, sandbox_mode, approval_policy | 绑定仓库/连接器/知识库范围；sandbox_mode + approval_policy 是策略下发载体 |
| `environments` | 工作区域 | id | workspace_id, name, config_json | 环境配置（dev/staging/prod 等） |
| `knowledge_bases` | 工作区域 | id | workspace_id, name, embedding_model, acl_json | RAG 知识库；acl_json 控制可见性 |
| `connectors` | MCP/连接器域 | id | tenant_id, name, type, endpoint, auth_mode, cred_ref | 连接器注册表；type=mcp/http/builtin；cred_ref 指向 Vault |
| `threads` | 会话域 | id | tenant_id, workspace_id, owner_user_id, codex_thread_id, rollout_object_key, permission_snapshot_hash | ★对应 Codex Thread（云端化）；rollout_object_key 指向对象存储 |
| `turns` | 会话域 | id | thread_id, seq, status, model, sandbox_mode, approval_policy, input_tokens, output_tokens, cost_micros | ★对应 Codex Turn；status 状态机 pending→running→waiting_approval→done/failed/interrupted |
| `items` | 会话域 | id | thread_id, turn_id, seq, kind, actor, content_ref, content_digest, visibility | ★对应 Codex Item（最大最热的表）；UNIQUE(thread_id,turn_id,seq) 幂等键 |
| `steps` | 会话域 | id | turn_id, seq, sample_status, model_output_ref, tool_calls_json | ★对应 Codex Step（单次采样快照） |
| `approval_tickets` | 审批域 | id | thread_id, turn_id, item_seq, tool_name, params_ref, diff_preview_ref, risk_level, status, decided_by | HITL 审批一等公民；pending→approved/rejected/expired/cancelled |
| `usage_records` | 计费配额域 | id | tenant_id, user_id, thread_id, turn_id, metric, quantity, model, unit_cost_micros | 计量流水；metric 覆盖 token_in/out/cached、tool_call、sandbox_second、storage_byte |
| `quotas` | 计费配额域 | id | tenant_id, scope, metric, limit_value, period, used_value | 配额限值；scope=tenant/org_unit/user/workspace |
| `budgets` | 计费配额域 | id | tenant_id, org_unit_id, amount_micros, spent_micros, period, alert_threshold, hard_limit | 预算控制；hard_limit=true 时超支阻断 |
| `sandbox_pods` | 沙箱域 | id | tenant_id, thread_id, turn_id, pod_name, status, node, cpu_milli, memory_mb | 执行面 Pod 生命周期跟踪 |
| `workspace_snapshots` | 沙箱域 | id | workspace_id, thread_id, rollout_version, object_key, content_digest, size_bytes | ★对应 Codex Rollout；归档到对象存储（按租户前缀 + CMK） |
| `audit_logs` | 审计域 | id | tenant_id, actor_type, actor_id, action, resource_type, resource_id, before_ref, after_ref, trace_id | WORM 只追加；投递 SIEM；按月分区 |
| `model_routes` | 模型域 | id | tenant_id, route_name, model_id, provider, priority, fallback_model_id, max_tokens, rate_limit_rpm | 多模型路由；支持 fallback 降级 |
| `model_credentials` | 模型域 | id | tenant_id, provider, cred_ref, api_key_enc, org_id | 模型 API 密钥管理；cred_ref 指向 Vault |
| `mcp_servers` | MCP/连接器域 | id | tenant_id, workspace_id, name, transport, endpoint, command, tool_whitelist | MCP 服务器注册；transport=stdio/sse/websocket |
| `mcp_credentials` | MCP/连接器域 | id | tenant_id, mcp_server_id, cred_type, cred_ref, scope_json, expires_at | MCP 凭据；按需注入沙箱侧车 |
| `skills` | Skills 域 | id | tenant_id, name, description, scope, status, latest_version_id | 企业技能市场；scope=public/tenant/private |
| `skill_versions` | Skills 域 | id | skill_id, version, content_ref, checksum, changelog, published_by, published_at | 技能版本快照；支持回滚 |
| `tool_call_logs` | MCP/连接器域 | id | thread_id, turn_id, item_seq, connector_id, tool_name, duration_ms, status, cost_micros | 工具调用审计日志 |

---

## 2. 关系矩阵

| 从 | 到 | 基数 | 外键 | 说明 |
|---|---|---|---|---|
| tenants | users | 1:N | users.tenant_id | 租户下多用户 |
| tenants | roles | 1:N | roles.tenant_id | 租户自定义角色 |
| users | tenant_memberships | M:N | memberships.user_id | 经关联表关联角色 |
| roles | tenant_memberships | 1:N | memberships.role_id | 一个角色多成员 |
| tenants | workspaces | 1:N | workspaces.tenant_id | 租户下多工作区 |
| workspaces | environments | 1:N | environments.workspace_id | 工作区多环境 |
| workspaces | knowledge_bases | 1:N | knowledge_bases.workspace_id | 工作区多知识库 |
| tenants | connectors | 1:N | connectors.tenant_id | 租户级连接器 |
| workspaces | threads | 1:N | threads.workspace_id | 工作区下多会话 |
| threads | turns | 1:N | turns.thread_id | 会话内多轮次 |
| turns | items | 1:N | items.turn_id | 轮次内多条事件 |
| turns | steps | 1:N | steps.turn_id | 轮次内多采样步骤 |
| threads | approval_tickets | 1:N | approval_tickets.thread_id | 会话级审批 |
| turns | approval_tickets | 1:N | approval_tickets.turn_id | 轮次级审批 |
| threads | usage_records | 1:N | usage_records.thread_id | 会话级用量 |
| tenants | usage_records | 1:N | usage_records.tenant_id | 租户级用量汇总 |
| tenants | quotas | 1:N | quotas.tenant_id | 租户配额 |
| tenants | budgets | 1:N | budgets.tenant_id | 租户预算 |
| threads | sandbox_pods | 1:N | sandbox_pods.thread_id | 会话级 Pod |
| threads | workspace_snapshots | 1:N | workspace_snapshots.thread_id | 会话级快照 |
| workspaces | workspace_snapshots | 1:N | workspace_snapshots.workspace_id | 工作区级快照 |
| tenants | model_routes | 1:N | model_routes.tenant_id | 租户级模型路由 |
| tenants | model_credentials | 1:N | model_credentials.tenant_id | 租户级模型凭据 |
| model_routes | model_credentials | N:1 | model_routes.provider→model_credentials.provider | 按提供商关联凭据 |
| tenants | mcp_servers | 1:N | mcp_servers.tenant_id | 租户级 MCP |
| mcp_servers | mcp_credentials | 1:N | mcp_credentials.mcp_server_id | MCP 凭据 |
| workspaces | mcp_servers | 1:N | mcp_servers.workspace_id | 工作区级 MCP |
| tenants | skills | 1:N | skills.tenant_id | 租户技能 |
| skills | skill_versions | 1:N | skill_versions.skill_id | 技能版本链 |
| turns | tool_call_logs | 1:N | tool_call_logs.turn_id | 轮次级工具调用 |

---

## 3. 分域详细 DDL

### 3.1 租户域

```sql
-- 租户
CREATE TABLE tenants (
    id              BIGSERIAL PRIMARY KEY,
    name            TEXT NOT NULL,
    plan            TEXT NOT NULL DEFAULT 'team',  -- free/team/enterprise
    isolation_tier  TEXT NOT NULL DEFAULT 'shared', -- shared/dedicated/kata
    cmk_id          TEXT,                          -- KMS 密钥 ID
    quota_profile   JSONB NOT NULL DEFAULT '{}',
    status          TEXT NOT NULL DEFAULT 'active', -- active/suspended/deleted
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 用户
CREATE TABLE users (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    idp_subject   TEXT,        -- OIDC/SAML subject
    email         CITEXT UNIQUE,
    display_name  TEXT,
    status        TEXT NOT NULL DEFAULT 'active',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 角色
CREATE TABLE roles (
    id               BIGSERIAL PRIMARY KEY,
    tenant_id        BIGINT NOT NULL REFERENCES tenants(id),
    name             TEXT NOT NULL,
    permissions_json JSONB NOT NULL DEFAULT '{}',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

-- 租户成员（用户↔角色 M:N）
CREATE TABLE tenant_memberships (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    user_id       BIGINT NOT NULL REFERENCES users(id),
    org_unit_id   BIGINT,
    role_id       BIGINT NOT NULL REFERENCES roles(id),
    scope_json    JSONB DEFAULT '{}',   -- 限定作用范围
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, user_id, role_id)
);

-- RLS 策略（所有租户表统一）
ALTER TABLE tenants  ENABLE ROW LEVEL SECURITY;
ALTER TABLE users     ENABLE ROW LEVEL SECURITY;
ALTER TABLE roles     ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_memberships ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON tenants  USING (id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON users     USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON roles     USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON tenant_memberships USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
```

### 3.2 工作区域

```sql
CREATE TABLE workspaces (
    id                  BIGSERIAL PRIMARY KEY,
    tenant_id           BIGINT NOT NULL REFERENCES tenants(id),
    name                TEXT NOT NULL,
    env_tag             TEXT DEFAULT 'dev',  -- dev/staging/prod
    repos_json          JSONB DEFAULT '[]',  -- 绑定仓库列表
    connectors_json     JSONB DEFAULT '[]',
    knowledge_scope_json JSONB DEFAULT '{}',
    sandbox_mode        TEXT DEFAULT 'read-only',  -- read-only/workspace/danger-full-access
    approval_policy     TEXT DEFAULT 'on-request', -- untrusted/on-failure/on-request/never
    max_risk_level      TEXT DEFAULT 'medium',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

CREATE TABLE environments (
    id            BIGSERIAL PRIMARY KEY,
    workspace_id  BIGINT NOT NULL REFERENCES workspaces(id),
    name          TEXT NOT NULL,
    config_json   JSONB DEFAULT '{}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE knowledge_bases (
    id              BIGSERIAL PRIMARY KEY,
    workspace_id    BIGINT NOT NULL REFERENCES workspaces(id),
    name            TEXT NOT NULL,
    embedding_model TEXT DEFAULT 'text-embedding-3-large',
    acl_json        JSONB DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS
ALTER TABLE workspaces ENABLE ROW LEVEL SECURITY;
ALTER TABLE environments ENABLE ROW LEVEL SECURITY;
ALTER TABLE knowledge_bases ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON workspaces USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY ws_isolation ON environments USING (
    workspace_id IN (SELECT id FROM workspaces WHERE tenant_id = current_setting('app.tenant_id')::BIGINT)
);
CREATE POLICY ws_isolation ON knowledge_bases USING (
    workspace_id IN (SELECT id FROM workspaces WHERE tenant_id = current_setting('app.tenant_id')::BIGINT)
);
```

### 3.3 会话域（核心）

```sql
-- 会话（对应 Codex Thread）
CREATE TABLE threads (
    id                       BIGSERIAL PRIMARY KEY,
    tenant_id                BIGINT NOT NULL REFERENCES tenants(id),
    workspace_id             BIGINT NOT NULL REFERENCES workspaces(id),
    owner_user_id            BIGINT REFERENCES users(id),
    agent_account_id        BIGINT,
    codex_thread_id          TEXT,          -- app-server 的 Thread ID
    title                    TEXT,
    status                   TEXT NOT NULL DEFAULT 'active', -- active/archived/failed
    rollout_object_key       TEXT,          -- 对象存储 key
    rollout_version          INT DEFAULT 0,
    permission_snapshot_hash TEXT,          -- ★ 权限快照哈希
    total_tokens             BIGINT DEFAULT 0,
    total_cost_micros        BIGINT DEFAULT 0,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_active_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 轮次（对应 Codex Turn）
CREATE TABLE turns (
    id               BIGSERIAL PRIMARY KEY,
    thread_id        BIGINT NOT NULL REFERENCES threads(id),
    seq              INT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'pending', -- pending/running/waiting_approval/done/failed/interrupted
    trigger          TEXT DEFAULT 'user',             -- user/agent/schedule/resume
    model            TEXT,
    sandbox_mode     TEXT,
    approval_policy  TEXT,
    input_tokens     INT DEFAULT 0,
    output_tokens    INT DEFAULT 0,
    cached_tokens    INT DEFAULT 0,
    cost_micros      BIGINT DEFAULT 0,
    started_at       TIMESTAMPTZ,
    ended_at         TIMESTAMPTZ,
    error_code       TEXT,
    UNIQUE(thread_id, seq)
);

-- 事件项（对应 Codex Item）★ 最大最热的表
CREATE TABLE items (
    id             BIGSERIAL PRIMARY KEY,
    thread_id      BIGINT NOT NULL REFERENCES threads(id),
    turn_id        BIGINT NOT NULL REFERENCES turns(id),
    seq            INT NOT NULL,
    kind           TEXT NOT NULL,  -- user_message/agent_message/reasoning/command_exec/file_change/mcp_call/approval/error
    actor          TEXT NOT NULL,  -- user/agent/tool/system
    content_ref    TEXT,           -- ★ 大字段指向对象存储
    content_digest  TEXT,           -- 内容摘要哈希
    summary        TEXT,            -- 列表与检索用
    visibility     TEXT DEFAULT 'user_visible', -- user_visible/internal/redacted
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(thread_id, turn_id, seq)  -- ★★★ 幂等键，事件重复投递时直接丢弃
);

-- 采样步骤（对应 Codex Step）
CREATE TABLE steps (
    id                BIGSERIAL PRIMARY KEY,
    turn_id           BIGINT NOT NULL REFERENCES turns(id),
    seq               INT NOT NULL,
    sample_status     TEXT,     -- pending/completed/failed
    model_output_ref  TEXT,     -- 模型输出引用（对象存储）
    tool_calls_json   JSONB,    -- 工具调用快照
    duration_ms       INT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(turn_id, seq)
);

-- RLS
ALTER TABLE threads ENABLE ROW LEVEL SECURITY;
ALTER TABLE turns   ENABLE ROW LEVEL SECURITY;
ALTER TABLE items   ENABLE ROW LEVEL SECURITY;
ALTER TABLE steps   ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON threads USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON turns   USING (thread_id IN (SELECT id FROM threads WHERE tenant_id = current_setting('app.tenant_id')::BIGINT));
CREATE POLICY tenant_isolation ON items   USING (thread_id IN (SELECT id FROM threads WHERE tenant_id = current_setting('app.tenant_id')::BIGINT));
CREATE POLICY tenant_isolation ON steps   USING (turn_id IN (SELECT t.id FROM turns t JOIN threads th ON t.thread_id=th.id WHERE th.tenant_id = current_setting('app.tenant_id')::BIGINT));
```

### 3.4 审批域

```sql
CREATE TABLE approval_tickets (
    id                     BIGSERIAL PRIMARY KEY,
    thread_id              BIGINT NOT NULL REFERENCES threads(id),
    turn_id                BIGINT NOT NULL REFERENCES turns(id),
    item_seq               INT NOT NULL,
    tool_name              TEXT NOT NULL,
    params_ref             TEXT,           -- 原始参数（对象存储）
    params_redacted        JSONB,          -- 脱敏后参数（可展示）
    diff_preview_ref       TEXT,           -- diff 预览（对象存储）
    risk_level             TEXT DEFAULT 'medium', -- low/medium/high/critical
    required_approver_role TEXT,           -- 需要的审批角色
    require_dual           BOOLEAN DEFAULT FALSE, -- 四眼原则
    status                 TEXT NOT NULL DEFAULT 'pending', -- pending/approved/rejected/expired/cancelled
    decided_by             BIGINT REFERENCES users(id),
    decided_at             TIMESTAMPTZ,
    decision_note          TEXT,
    context_snapshot_ref   TEXT,           -- 审批时上下文快照
    expires_at             TIMESTAMPTZ,
    default_action         TEXT DEFAULT 'deny', -- deny/allow
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 索引
CREATE INDEX idx_approval_status ON approval_tickets(status) WHERE status = 'pending';
CREATE INDEX idx_approval_thread ON approval_tickets(thread_id);

-- RLS
ALTER TABLE approval_tickets ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON approval_tickets USING (
    thread_id IN (SELECT id FROM threads WHERE tenant_id = current_setting('app.tenant_id')::BIGINT)
);
```

### 3.5 计费配额域

```sql
-- 用量流水（只追加）
CREATE TABLE usage_records (
    id               BIGSERIAL PRIMARY KEY,
    tenant_id        BIGINT NOT NULL REFERENCES tenants(id),
    org_unit_id      BIGINT,
    user_id          BIGINT REFERENCES users(id),
    thread_id        BIGINT REFERENCES threads(id),
    turn_id          BIGINT REFERENCES turns(id),
    metric           TEXT NOT NULL,  -- token_in/token_out/token_cached/tool_call/sandbox_second/storage_byte
    quantity         NUMERIC(20,4) NOT NULL,
    model            TEXT,
    unit_cost_micros BIGINT,
    occurred_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
-- ★ 应用账号无 UPDATE/DELETE 权限（独立角色写入）

-- 配额
CREATE TABLE quotas (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   BIGINT NOT NULL REFERENCES tenants(id),
    scope       TEXT NOT NULL,     -- tenant/org_unit/user/workspace
    scope_id    BIGINT,
    metric      TEXT NOT NULL,
    limit_value NUMERIC(20,4) NOT NULL,
    period      TEXT NOT NULL,     -- hourly/daily/monthly
    used_value  NUMERIC(20,4) DEFAULT 0,
    reset_at    TIMESTAMPTZ
);

-- 预算
CREATE TABLE budgets (
    id              BIGSERIAL PRIMARY KEY,
    tenant_id       BIGINT NOT NULL REFERENCES tenants(id),
    org_unit_id     BIGINT,
    amount_micros   BIGINT NOT NULL,
    spent_micros    BIGINT DEFAULT 0,
    period          TEXT NOT NULL,  -- monthly/quarterly/annual
    alert_threshold NUMERIC(5,2) DEFAULT 0.8,
    hard_limit      BOOLEAN DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS
ALTER TABLE usage_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE quotas ENABLE ROW LEVEL SECURITY;
ALTER TABLE budgets ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON usage_records USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON quotas USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON budgets USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
```

### 3.6 沙箱域

```sql
CREATE TABLE sandbox_pods (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    thread_id     BIGINT NOT NULL REFERENCES threads(id),
    turn_id       BIGINT REFERENCES turns(id),
    pod_name      TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending', -- pending/running/terminated/failed
    node          TEXT,
    cpu_milli     INT,
    memory_mb     INT,
    started_at    TIMESTAMPTZ,
    terminated_at TIMESTAMPTZ,
    exit_reason   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE workspace_snapshots (
    id              BIGSERIAL PRIMARY KEY,
    workspace_id    BIGINT NOT NULL REFERENCES workspaces(id),
    thread_id       BIGINT REFERENCES threads(id),
    rollout_version INT NOT NULL,
    object_key      TEXT NOT NULL,   -- ★ 对象存储 key（按租户前缀+CMK）
    content_digest  TEXT NOT NULL,
    size_bytes      BIGINT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS
ALTER TABLE sandbox_pods ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_snapshots ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON sandbox_pods USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON workspace_snapshots USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
```

### 3.7 审计域（WORM）

```sql
-- ★ 只追加，禁止 UPDATE/DELETE
CREATE TABLE audit_logs (
    id            BIGSERIAL,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    actor_type    TEXT NOT NULL,    -- user/service_account/agent/system
    actor_id      TEXT,
    action        TEXT NOT NULL,    -- create/update/delete/login/approve/reject
    resource_type TEXT,
    resource_id   TEXT,
    before_ref    TEXT,              -- 变更前快照（对象存储）
    after_ref     TEXT,              -- 变更后快照（对象存储）
    ip            INET,
    user_agent    TEXT,
    trace_id      TEXT,
    occurred_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
) PARTITION BY RANGE (occurred_at);  -- ★ 按月分区

-- 创建当月分区
CREATE TABLE audit_logs_2026_09 PARTITION OF audit_logs
    FOR VALUES FROM ('2026-09-01') TO ('2026-10-01');
CREATE TABLE audit_logs_2026_10 PARTITION OF audit_logs
    FOR VALUES FROM ('2026-10-01') TO ('2026-11-01');

-- 索引
CREATE INDEX idx_audit_tenant_time ON audit_logs(tenant_id, occurred_at DESC);
CREATE INDEX idx_audit_action ON audit_logs(action);

-- ★ 应用账号无 UPDATE/DELETE 权限（GRANT INSERT ONLY）
-- GRANT INSERT ON audit_logs TO nexus_app;
-- （不授予 UPDATE/DELETE）

-- RLS
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON audit_logs USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
```

### 3.8 模型域

```sql
CREATE TABLE model_routes (
    id                 BIGSERIAL PRIMARY KEY,
    tenant_id          BIGINT NOT NULL REFERENCES tenants(id),
    route_name         TEXT NOT NULL,
    model_id           TEXT NOT NULL,    -- gpt-4o / claude-3.5-sonnet / deepseek-v3
    provider           TEXT NOT NULL,    -- openai/anthropic/deepseek/ollama
    priority           INT DEFAULT 0,
    fallback_model_id  TEXT,             -- 降级模型
    max_tokens         INT,
    rate_limit_rpm     INT,
    enabled            BOOLEAN DEFAULT TRUE,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, route_name)
);

CREATE TABLE model_credentials (
    id              BIGSERIAL PRIMARY KEY,
    tenant_id       BIGINT NOT NULL REFERENCES tenants(id),
    provider        TEXT NOT NULL,
    cred_ref        TEXT NOT NULL,       -- Vault 引用
    api_key_enc     TEXT,                -- 加密后 API Key
    org_id          TEXT,
    enabled         BOOLEAN DEFAULT TRUE,
    last_rotated_at TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS
ALTER TABLE model_routes ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_credentials ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON model_routes USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON model_credentials USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
```

### 3.9 MCP/连接器域

```sql
CREATE TABLE mcp_servers (
    id             BIGSERIAL PRIMARY KEY,
    tenant_id      BIGINT NOT NULL REFERENCES tenants(id),
    workspace_id   BIGINT REFERENCES workspaces(id),
    name           TEXT NOT NULL,
    transport      TEXT NOT NULL,    -- stdio/sse/websocket
    endpoint       TEXT,             -- sse/ws 端点
    command        TEXT,             -- stdio 命令
    args_json      JSONB DEFAULT '[]',
    env_redacted   JSONB DEFAULT '{}',
    tool_whitelist JSONB DEFAULT '[]',
    enabled        BOOLEAN DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

CREATE TABLE mcp_credentials (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    mcp_server_id BIGINT NOT NULL REFERENCES mcp_servers(id),
    cred_type     TEXT NOT NULL,    -- api_key/oauth/bearer/basic
    cred_ref      TEXT NOT NULL,    -- Vault 引用
    scope_json    JSONB DEFAULT '{}',
    expires_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tool_call_logs (
    id            BIGSERIAL PRIMARY KEY,
    thread_id     BIGINT NOT NULL REFERENCES threads(id),
    turn_id       BIGINT NOT NULL REFERENCES turns(id),
    item_seq      INT NOT NULL,
    connector_id  BIGINT REFERENCES connectors(id),
    tool_name     TEXT NOT NULL,
    duration_ms   INT,
    status        TEXT,             -- success/failed/timeout
    error_type    TEXT,
    cost_micros   BIGINT DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS
ALTER TABLE mcp_servers ENABLE ROW LEVEL SECURITY;
ALTER TABLE mcp_credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_call_logs ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON mcp_servers USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON mcp_credentials USING (tenant_id = current_setting('app.tenant_id')::BIGINT);
CREATE POLICY tenant_isolation ON tool_call_logs USING (thread_id IN (SELECT id FROM threads WHERE tenant_id = current_setting('app.tenant_id')::BIGINT));
```

### 3.10 Skills 域

```sql
CREATE TABLE skills (
    id                  BIGSERIAL PRIMARY KEY,
    tenant_id           BIGINT NOT NULL REFERENCES tenants(id),
    name                TEXT NOT NULL,
    description         TEXT,
    scope               TEXT NOT NULL DEFAULT 'tenant', -- public/tenant/private
    status              TEXT NOT NULL DEFAULT 'active', -- active/draft/deprecated
    latest_version_id   BIGINT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

CREATE TABLE skill_versions (
    id            BIGSERIAL PRIMARY KEY,
    skill_id      BIGINT NOT NULL REFERENCES skills(id),
    version       TEXT NOT NULL,
    content_ref   TEXT NOT NULL,    -- 技能内容（对象存储）
    checksum      TEXT NOT NULL,
    changelog     TEXT,
    published_by  BIGINT REFERENCES users(id),
    published_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(skill_id, version)
);

-- RLS
ALTER TABLE skills ENABLE ROW LEVEL SECURITY;
ALTER TABLE skill_versions ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON skills USING (tenant_id = current_setting('app.tenant_id')::BIGINT OR scope = 'public');
CREATE POLICY tenant_isolation ON skill_versions USING (
    skill_id IN (SELECT id FROM skills WHERE tenant_id = current_setting('app.tenant_id')::BIGINT OR scope = 'public')
);
```

---

## 4. 与 Codex 原语映射

| Codex 原语 | Nexus 表 | 映射关系 | 说明 |
|---|---|---|---|
| **Thread** | `threads` | 1:1 映射 | app-server 的 Thread 对应云端 `threads` 一行；`codex_thread_id` 存 app-server 分配的 ID；`rollout_object_key` 指向同步到对象存储的 rollout 文件 |
| **Turn** | `turns` | 1:1 映射 | 每个 Turn 对应 `turns` 一行；`seq` = Turn 序号；status 状态机比 Codex 更丰富（加 `waiting_approval`） |
| **Item** | `items` | 1:1 映射 | Codex Item 是原子持久化事实；`items` 表是事件流持久化形态；`UNIQUE(thread_id, turn_id, seq)` 幂等键保证精确去重 |
| **Step** | `steps` | 1:1 映射 | Codex Step 是 Turn 内单次采样快照；`steps` 表存采样状态与工具调用快照 |
| **Approval** | `approval_tickets` | 桥接映射 | Codex 的 TUI 弹窗审批 → 云端一等公民 `approval_tickets`；pending→approved/rejected/expired；Pod 死后可从 DB 重建审批态 |
| **Rollout** | `workspace_snapshots` | 归档映射 | Codex 本地 rollout 文件 → 云端 `workspace_snapshots` 行 + 对象存储 key；Pod 死后可从对象存储恢复 workspace |
| **Session** | （不映射） | 内部运行态 | Codex Session 是 Core 内部运行时持有器，不对外暴露；云端不持久化，Pod 死后从 Thread + Turn + Item + Rollout 重建 |

```
Codex 本地                          Nexus 云端 Postgres
──────────                          ──────────────────
Thread (app-server)    ──────────→  threads
  └ Turn                ──────────→    └ turns
      └ Step             ──────────→        └ steps
      └ Item             ──────────→        └ items ★幂等键
  └ Approval (TUI)      ── bridge ─→    └ approval_tickets
Rollout (本地文件)       ── sync ──→  workspace_snapshots + 对象存储
Session (内部运行态)     ── (不映射) ──  从 threads+turns+items+rollout 重建
```

---

## 5. 索引与分区策略

| 表 | 策略 | 说明 |
|---|---|---|
| `items` | 按 `tenant_id` + 时间范围分区 | ★ 最大最热的表；按 tenant_id 聚簇 + 按月范围分区；大字段外置对象存储，主表只存摘要 |
| `audit_logs` | 按月分区 (PARTITION BY RANGE occurred_at) | WORM 只追加；冷分区可归档到对象存储（降低 PG 体积）；保留期合规要求 1-7 年 |
| `usage_records` | 按 `tenant_id` + 月分区 | 计量流水高频写入；按租户+时间分区便于聚合查询与归档 |
| `turns` | 指数索引 `(thread_id, seq)` | 会话内顺序访问；UNIQUE 保证序号唯一 |
| `threads` | 索引 `(tenant_id, status, last_active_at DESC)` | 列表页高频查询"我的活跃会话" |
| `approval_tickets` | 部分索引 `WHERE status='pending'` | 审批中心只查 pending；部分索引极小极快 |
| `sandbox_pods` | 索引 `(tenant_id, status)` | 调度器查可用 Pod |
| `tool_call_logs` | 按 `thread_id` 聚簇 | 会话级工具调用审计 |
| `model_routes` | 索引 `(tenant_id, route_name)` UNIQUE | 路由查找 |

**分区管理原则**：
- 分区表主键必须包含分区键（`items` 用 `(id, tenant_id, created_at)` 复合主键）
- 自动分区：pg_partman 或 pg_cron 定期创建未来分区、归档旧分区
- 冷分区（>3 个月）DETACH 后导出到对象存储，PG 只保留热分区

---

## 6. 数据生命周期

| 层级 | 数据 | 存储 | 保留期 | 说明 |
|---|---|---|---|---|
| **热** | `items`（近 3 个月）、`turns`、`threads`（active）、`approval_tickets`（pending） | PG 主库 | 0-3 月 | 高频读写；items 大字段外置，主表查摘要 |
| **热** | `usage_records`（当月）、`sandbox_pods`（active） | PG 主库 | 当月 | 计量实时聚合 |
| **温** | `items`（3-12 月）、`threads`（archived）、`audit_logs`（当年） | PG 分区（较旧） | 3-12 月 | 低频查询；分区自动管理 |
| **冷** | `audit_logs`（>1 年）、`usage_records`（>1 年） | 对象存储 + SIEM | 1-7 年 | 合规留存；PG DETACH 分区后导出 |
| **冷** | `workspace_snapshots`（rollout 归档） | 对象存储（按租户前缀 + CMK） | 按租户策略 | Pod 死后重建用；按租户 CMK 加密 |
| **冷** | `items.content_ref` / `approval_tickets.params_ref` 等大字段 | 对象存储 | 与主表同生命周期 | 主表只存 digest + ref |

**归档流程**：
1. `items` 冷分区 DETACH → 导出 Parquet → 上传对象存储 `s3://nexus-archive/{tenant}/items/{yyyy-mm}/`
2. `audit_logs` 冷分区 DETACH → 导出 JSON → 上传 + 投递 SIEM
3. `workspace_snapshots` 超过保留期 → 对象存储生命周期策略删除
4. 禁用租户 CMK → 对象存储数据不可解密（加密粉碎）

---

## 7. 配套产物索引

| 产物 | 文件 |
|---|---|
| 详细方案 | `domain-model.md`（本文件） |
| 核心域 ER 图 | `domain-model-core.svg` / `.png` |
| 治理域 ER 图 | `domain-model-governance.svg` / `.png` |
| 合并 ER 图 | `domain-model.svg` / `.png` |
| SVG 生成脚本 | `_gen_svg.py` |
| 交互报告 | `domain-model-report.html` |

> 本方案与 `../Nexus 基于CodexHarness的企业级Agent平台_系统设计与实施路线图.md` §5「核心数据模型」对齐，为其工程化落地版。ER 图遵循 archify 深色主题风格，色板复用 `01-system-architecture/_gen_svg.py`。
