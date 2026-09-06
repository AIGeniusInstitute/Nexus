-- M10: audit_logs — extend the existing table into a general-purpose WORM
-- audit query surface. The table already exists (created by an earlier
-- milestone with actor_user_id / resource / detail_json); we add the
-- target_type/target_id/trace_id columns needed for cross-cutting audit
-- queries and ensure the WORM trigger is in place. All idempotent.

ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS target_type TEXT;
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS target_id TEXT;
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS trace_id TEXT;

CREATE INDEX IF NOT EXISTS idx_audit_tenant_time
    ON audit_logs (tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_action_time
    ON audit_logs (action, created_at DESC);

-- WORM guarantee: block any UPDATE or DELETE on audit_logs.
CREATE OR REPLACE FUNCTION prevent_audit_modification()
RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'audit_logs is append-only (WORM): modification forbidden';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS audit_worm ON audit_logs;
CREATE TRIGGER audit_worm
    BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION prevent_audit_modification();
