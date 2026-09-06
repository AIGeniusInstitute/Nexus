-- M16 连接器生态市场：连接器目录治理层（分级 + 质量分 + 上下线 + 贡献者）
-- 纯增量：ALTER 已有 connectors/tool_call_logs 表加字段，不碰核心路径。

-- 连接器治理字段
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS tier TEXT NOT NULL DEFAULT 'community'
  CHECK(tier IN ('official','enterprise','community'));
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'draft'
  CHECK(status IN ('draft','published','offline'));
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS contributor_user_id BIGINT REFERENCES users(id);
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS quality_score REAL NOT NULL DEFAULT 0;
ALTER TABLE connectors ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
CREATE INDEX IF NOT EXISTS idx_connectors_tenant_status ON connectors(tenant_id, status);

-- 质量分数据源：tool_call_logs 加 success + connector_id 关联
ALTER TABLE tool_call_logs ADD COLUMN IF NOT EXISTS success BOOLEAN;
ALTER TABLE tool_call_logs ADD COLUMN IF NOT EXISTS connector_id BIGINT REFERENCES connectors(id);
CREATE INDEX IF NOT EXISTS idx_toolcall_connector ON tool_call_logs(connector_id) WHERE connector_id IS NOT NULL;
