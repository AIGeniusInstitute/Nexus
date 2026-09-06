-- M17 Skills 市场：企业 Skill 发布/版本/回滚治理层（roadmap T12-3）
-- 纯增量：ALTER skills 加治理字段，skill_versions 已有版本快照。

ALTER TABLE skills ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE skills ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'draft'
  CHECK(status IN ('draft','published','archived'));
ALTER TABLE skills ADD COLUMN IF NOT EXISTS owner_user_id BIGINT REFERENCES users(id);
ALTER TABLE skills ADD COLUMN IF NOT EXISTS active_version_id BIGINT REFERENCES skill_versions(id);
ALTER TABLE skills ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
CREATE INDEX IF NOT EXISTS idx_skills_tenant_status ON skills(tenant_id, status);
