-- M22: 事件溯源 + Lineage + 电子签名 + CAS/WORM 对象存储 (P3)
-- 设计原则：PG append-only + WORM trigger + content_hash 模拟 CAS，
--   不引入 Kafka/EventStore/Neo4j/外部 CAS（Simplicity First）。
-- 纯增量：不删/不改 M20/M21 表。复用 M10 WORM trigger 模式。

-- 1. 通用 append-only guard 函数（各 WORM 表挂各自 trigger，避免与 M10
--    prevent_audit_modification 耦合）
CREATE OR REPLACE FUNCTION raise_append_only()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'append-only (WORM): modification forbidden';
END;
$$ LANGUAGE plpgsql;

-- 2. domain_events：领域事件流（事件溯源）
--    覆盖 golden_set/rubric/judge_config/eval_batch_run/approval/esign
--    entity 生命周期动作。event_seq per-entity 单调递增（应用层取 MAX+1），
--    支持 as-of 时间旅行回放。
CREATE TABLE IF NOT EXISTS domain_events (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  entity_type TEXT NOT NULL,
  entity_id BIGINT NOT NULL,
  event_type TEXT NOT NULL,
  event_seq BIGINT NOT NULL,
  payload JSONB NOT NULL DEFAULT '{}'::jsonb,
  occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE(entity_type, entity_id, event_seq)
);
CREATE INDEX IF NOT EXISTS idx_events_tenant_entity_time
  ON domain_events(tenant_id, entity_type, entity_id, occurred_at);
CREATE INDEX IF NOT EXISTS idx_events_entity_seq
  ON domain_events(entity_type, entity_id, event_seq);
DROP TRIGGER IF EXISTS prevent_event_modification ON domain_events; CREATE TRIGGER prevent_event_modification
  BEFORE UPDATE OR DELETE ON domain_events
  FOR EACH ROW EXECUTE FUNCTION raise_append_only();

-- 3. esign_records：电子签名（21 CFR Part 11 三要素）
--    签名者身份(signer_user_id) + 签名时间(signed_at) + 签名含义
--    (signature_purpose + meaning_text)。signature_hash 绑定上述要素 +
--    签名对象(entity/version)，任何篡改可经 verify 重算发现。
CREATE TABLE IF NOT EXISTS esign_records (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  entity_type TEXT NOT NULL,
  entity_id BIGINT NOT NULL,
  version_id BIGINT,
  signer_user_id BIGINT NOT NULL REFERENCES users(id),
  signature_purpose TEXT NOT NULL,
  signature_hash CHAR(64) NOT NULL,
  meaning_text TEXT NOT NULL,
  signed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_esign_tenant_entity
  ON esign_records(tenant_id, entity_type, entity_id);
DROP TRIGGER IF EXISTS prevent_esign_modification ON esign_records; CREATE TRIGGER prevent_esign_modification
  BEFORE UPDATE OR DELETE ON esign_records
  FOR EACH ROW EXECUTE FUNCTION raise_append_only();

-- 4. content_store：内容寻址存储（CAS / WORM 对象存储）
--    content_hash = SHA-256(原文)，主键去重。M20 entity_version.content_ref
--    是 TEXT（完整内容或 hash 引用）；M22 新增写入路径：大对象原文落
--    content_store，content_ref 存 "cas:{hash}" 引用。读取经 CAS 解引用。
--    WORM：不可改不可删（content_hash 即内容指纹，改即不同 key）。
CREATE TABLE IF NOT EXISTS content_store (
  content_hash CHAR(64) PRIMARY KEY,
  content BYTEA NOT NULL,
  size BIGINT NOT NULL,
  content_type TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
DROP TRIGGER IF EXISTS prevent_content_modification ON content_store; CREATE TRIGGER prevent_content_modification
  BEFORE UPDATE OR DELETE ON content_store
  FOR EACH ROW EXECUTE FUNCTION raise_append_only();
