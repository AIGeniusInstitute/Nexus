-- M14: Thread Snapshot + Fork + Rollback (roadmap M11 T11-4 fork 基础)
-- workspace_snapshots 既有表（初始 migration 空壳），ALTER 加 tenant_id + forked_to_thread_id

ALTER TABLE workspace_snapshots ADD COLUMN IF NOT EXISTS tenant_id BIGINT;
ALTER TABLE workspace_snapshots ADD COLUMN IF NOT EXISTS forked_to_thread_id UUID;

CREATE INDEX IF NOT EXISTS idx_snapshots_tenant_thread
    ON workspace_snapshots(tenant_id, thread_id);
