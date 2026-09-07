-- M21: JudgeConfig 版本化 + eval_batch_runs.judge_version_id
-- 纯增量：不触碰 M20 的 6 张表结构（仅 ALTER ADD COLUMN）
-- JudgeConfig 复用 M20 entity_version 表（entity_type='judge_config'）

CREATE TABLE IF NOT EXISTS judge_configs (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','locked','archived')),
  active_version_id BIGINT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_judge_configs_tenant_status ON judge_configs(tenant_id, status);

-- eval_batch_runs 加 judge 版本关联（M20 表 ALTER ADD COLUMN，外科手术式）
ALTER TABLE eval_batch_runs ADD COLUMN IF NOT EXISTS judge_version_id BIGINT;
