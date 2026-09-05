-- Nexus M3: 审批与策略增量 schema。
-- approval_tickets 是 M1 预留骨架（id/thread_id/turn_id/risk_level/decided_by/decided_at/decision/created_at）。
-- M3 扩展为完整 HITL 工单：加 tenant_id/kind/status/item_id/jsonrpc_id/command/cwd/reason/raw_params。
-- 新增 approval_audit（决策审计流水）+ policies（策略中心：角色×工具×风险×决策）。

-- 1. approval_tickets 扩展列（幂等 ADD COLUMN IF NOT EXISTS）
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS tenant_id BIGINT;
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS kind TEXT;          -- command_execution | file_change
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending';  -- pending|approved|denied|cancelled|interrupted
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS item_id TEXT;       -- codex ThreadItem.id
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS jsonrpc_id JSONB;   -- 原始 JSON-RPC request id（回写匹配用）
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS command TEXT;
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS cwd TEXT;
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS reason TEXT;
ALTER TABLE approval_tickets ADD COLUMN IF NOT EXISTS raw_params JSONB;
-- 给已有行回填 tenant_id（从 thread 推导）
UPDATE approval_tickets a SET tenant_id = (SELECT tenant_id FROM threads WHERE id = a.thread_id) WHERE a.tenant_id IS NULL;
-- pending 索引（只索引未决）
CREATE INDEX IF NOT EXISTS idx_approval_status ON approval_tickets(status) WHERE status='pending';
CREATE INDEX IF NOT EXISTS idx_approval_tenant ON approval_tickets(tenant_id);

-- 2. approval_audit 决策审计流水（每次状态变更加一行）
CREATE TABLE IF NOT EXISTS approval_audit (
    id            BIGSERIAL PRIMARY KEY,
    approval_id   BIGINT NOT NULL REFERENCES approval_tickets(id),
    actor_user_id BIGINT,
    action        TEXT NOT NULL,           -- created | resolved | revoked_deny | interrupted | timeout_deny
    decision      TEXT,                    -- approve | deny | cancel（解析时）
    params_digest TEXT,                    -- 命令摘要（脱敏后）
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_approval_audit_approval ON approval_audit(approval_id);

-- 3. policies 策略中心（角色×工具×风险×决策矩阵）
CREATE TABLE IF NOT EXISTS policies (
    id           BIGSERIAL PRIMARY KEY,
    tenant_id    BIGINT NOT NULL REFERENCES tenants(id),
    role         TEXT NOT NULL,            -- admin | developer | viewer | *（通配）
    action_kind  TEXT NOT NULL,           -- command_execution | file_change | *
    pattern      TEXT NOT NULL,           -- 命令/路径 glob，如 rm -rf* / sudo* / *
    risk_level   TEXT NOT NULL,           -- low | medium | high
    decision     TEXT NOT NULL,           -- allow | prompt | deny
    priority     INT NOT NULL DEFAULT 0,
    enabled      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_policies_tenant ON policies(tenant_id, enabled);
-- 唯一约束使 seed 的 ON CONFLICT 真正幂等（重跑 migrate 不插重）。
-- 先去重已存在的重复行，再加唯一索引。
DELETE FROM policies a USING policies b
  WHERE a.id > b.id AND a.tenant_id=b.tenant_id AND a.role=b.role
    AND a.action_kind=b.action_kind AND a.pattern=b.pattern;
CREATE UNIQUE INDEX IF NOT EXISTS uq_policies_role_kind_pattern
  ON policies(tenant_id, role, action_kind, pattern);

-- 4. 默认策略 seed（default 租户）—— 最小集，双保险（协议级 .rules + 运行时 HITL）
INSERT INTO policies (tenant_id, role, action_kind, pattern, risk_level, decision, priority)
SELECT t.id, '*', 'command_execution', 'rm -rf*', 'high', 'deny', 100
FROM tenants t WHERE t.slug='default'
ON CONFLICT (tenant_id, role, action_kind, pattern) DO NOTHING;
INSERT INTO policies (tenant_id, role, action_kind, pattern, risk_level, decision, priority)
SELECT t.id, '*', 'command_execution', 'sudo*', 'high', 'deny', 90
FROM tenants t WHERE t.slug='default'
ON CONFLICT (tenant_id, role, action_kind, pattern) DO NOTHING;
INSERT INTO policies (tenant_id, role, action_kind, pattern, risk_level, decision, priority)
SELECT t.id, '*', 'command_execution', 'rm*', 'high', 'deny', 80
FROM tenants t WHERE t.slug='default'
ON CONFLICT (tenant_id, role, action_kind, pattern) DO NOTHING;
INSERT INTO policies (tenant_id, role, action_kind, pattern, risk_level, decision, priority)
SELECT t.id, '*', 'command_execution', 'ls*', 'low', 'allow', 10
FROM tenants t WHERE t.slug='default'
ON CONFLICT (tenant_id, role, action_kind, pattern) DO NOTHING;
INSERT INTO policies (tenant_id, role, action_kind, pattern, risk_level, decision, priority)
SELECT t.id, '*', 'command_execution', 'cat*', 'low', 'allow', 10
FROM tenants t WHERE t.slug='default'
ON CONFLICT (tenant_id, role, action_kind, pattern) DO NOTHING;
INSERT INTO policies (tenant_id, role, action_kind, pattern, risk_level, decision, priority)
SELECT t.id, '*', 'command_execution', '*', 'medium', 'prompt', 0
FROM tenants t WHERE t.slug='default'
ON CONFLICT (tenant_id, role, action_kind, pattern) DO NOTHING;
INSERT INTO policies (tenant_id, role, action_kind, pattern, risk_level, decision, priority)
SELECT t.id, 'developer', 'file_change', '*', 'medium', 'prompt', 0
FROM tenants t WHERE t.slug='default'
ON CONFLICT (tenant_id, role, action_kind, pattern) DO NOTHING;
