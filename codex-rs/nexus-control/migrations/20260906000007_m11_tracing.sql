-- M11: per-turn trace_id + indexes for timeline / trace lookup.
-- audit_logs.trace_id was added in M10; here we add the turns-side column
-- and partial-index audit_logs by trace_id so the trace API is fast.

ALTER TABLE turns ADD COLUMN IF NOT EXISTS trace_id UUID NOT NULL DEFAULT gen_random_uuid();
CREATE INDEX IF NOT EXISTS idx_turns_trace ON turns (trace_id);
CREATE INDEX IF NOT EXISTS idx_audit_trace ON audit_logs (trace_id) WHERE trace_id IS NOT NULL;
