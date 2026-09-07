# Nexus M22 事件溯源 + Lineage + 电子签名 — 技术方案

## 设计原则
1. Simplicity First：PG append-only + WORM trigger + content_hash 模拟 CAS，不引入外部系统
2. Surgical Changes：纯增量新表，不改 M20/M21 评分逻辑，不改 start_run drain
3. 复用 M10 WORM trigger 模式（BEFORE UPDATE OR DELETE RAISE EXCEPTION）

## DDL（migration 20260906000016_m22_event_esign_cas.sql）

### domain_events（事件溯源 append-only）
```sql
CREATE TABLE domain_events (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  entity_type TEXT NOT NULL,      -- golden_set/rubric/judge_config/eval_batch_run/approval/agent_release
  entity_id BIGINT NOT NULL,
  event_type TEXT NOT NULL,       -- created/published/run_started/run_completed/approval_resolved/signed
  event_seq BIGINT NOT NULL,      -- per-entity 单调递增（应用层取 MAX+1）
  payload JSONB NOT NULL DEFAULT '{}',
  occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE(entity_type, entity_id, event_seq)
);
CREATE INDEX idx_events_tenant_entity_time ON domain_events(tenant_id, entity_type, entity_id, occurred_at);
-- WORM
CREATE TRIGGER prevent_event_modification BEFORE UPDATE OR DELETE ON domain_events
  FOR EACH ROW EXECUTE FUNCTION raise_append_only();
```

### esign_records（电子签名 21 CFR Part 11）
```sql
CREATE TABLE esign_records (
  id BIGSERIAL PRIMARY KEY,
  tenant_id BIGINT NOT NULL,
  entity_type TEXT NOT NULL,      -- entity_version / eval_batch_run / approval_ticket
  entity_id BIGINT NOT NULL,
  version_id BIGINT,              -- 签名对象版本（entity_version.id）
  signer_user_id BIGINT NOT NULL REFERENCES users(id),
  signature_purpose TEXT NOT NULL,  -- 签名含义（如"审核批准"/"合规放行"）
  signature_hash CHAR(64) NOT NULL, -- SHA-256(signer_id|entity|version|purpose|timestamp)
  meaning_text TEXT NOT NULL,       -- 21 CFR 11.50 签名含义文本
  signed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_esign_tenant_entity ON esign_records(tenant_id, entity_type, entity_id);
-- WORM
CREATE TRIGGER prevent_esign_modification BEFORE UPDATE OR DELETE ON esign_records
  FOR EACH ROW EXECUTE FUNCTION raise_append_only();
```

### content_store（CAS 对象存储）
```sql
CREATE TABLE content_store (
  content_hash CHAR(64) PRIMARY KEY,  -- SHA-256(原文)
  content BYTEA NOT NULL,
  size BIGINT NOT NULL,
  content_type TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
-- WORM：CAS 不可改不可删
CREATE TRIGGER prevent_content_modification BEFORE UPDATE OR DELETE ON content_store
  FOR EACH ROW EXECUTE FUNCTION raise_append_only();
```

### raise_append_only 函数（复用 M10 模式）
```sql
CREATE OR REPLACE FUNCTION raise_append_only() RETURNS TRIGGER AS $$
BEGIN RAISE EXCEPTION 'append-only (WORM): modification forbidden'; END;
$$ LANGUAGE plpgsql;
```

## 模块 lineages.rs / esign.rs / content_store.rs / events.rs

### events.rs
- `record_event(pool, tid, entity_type, entity_id, event_type, payload)`：event_seq = SELECT COALESCE(MAX(event_seq),0)+1，INSERT（best-effort 不阻塞业务）
- `list_events(pool, tid, entity_type, entity_id, as_of: Option<DateTime>)`：WHERE occurred_at <= as_of ORDER BY event_seq，as-of 回放
- `replay_state(events) -> Value`：应用层按 event_seq 顺序 merge payload → 实体状态快照（JSON）

### lineages.rs
- `lineage_for(pool, tid, entity_type, entity_id)`：基于 SQL JOIN 拼装上下游链：
  - eval_batch_run → golden_set_version → golden_set → cases
  - eval_batch_run → case_results → agent_output
  - eval_batch_run → domain_events（生命周期）
  - eval_batch_run → esign_records（签名链）
  返回 `{nodes:[], edges:[]}` 图结构供前端渲染

### esign.rs
- `sign(pool, tid, entity_type, entity_id, version_id, signer_uid, purpose, meaning)`：
  signature_hash = SHA-256(format!("{signer_uid}|{entity_type}|{entity_id}|{version_id}|{purpose}|{timestamp}"))
  INSERT esign_records + record_event(event_type="signed")，返回 id
- `verify_esign(pool, tid, esign_id)`：查 record → 重算 hash 比对 → 三要素完整性（signer/purpose/meaning/singed_at 非空）→ {valid, mismatch_fields}
- `list_esigns(pool, tid, entity_type, entity_id)`

### content_store.rs
- `store_content(content: &[u8], content_type) -> String`：hash=SHA-256(content)，INSERT ON CONFLICT DO NOTHING（去重），返回 hash
- `get_content(hash) -> Vec<u8>`：SELECT content WHERE content_hash
- `store_text(text) -> String`：便捷包装 store_content(text.as_bytes(), "text/plain")

## http_server.rs 路由（7 新路由）
- `GET /v1/events/{entity_type}/{entity_id}?as_of=` 事件列表 + as-of 回放
- `GET /v1/lineage/{entity_type}/{entity_id}` 血缘图
- `POST /v1/esign` 签名（body: entity_type/entity_id/version_id/purpose/meaning）
- `GET /v1/esign/{id}` 查签名
- `GET /v1/esign/{id}/verify` 验证签名
- `GET /v1/esigns?entity_type=&entity_id=` 列表
- `POST /v1/content` CAS 写入（body: content text）→ {content_hash}

## 事件埋点（events.rs record_event 调用）
- golden_set publish → record_event("golden_set", id, "published", {version_id, semver})
- judge_config publish → record_event("judge_config", ...)
- eval_batch_run start → record_event("eval_batch_run", id, "run_started", {...})
- eval_batch_run complete → record_event("eval_batch_run", id, "run_completed", {aggregate})
- approval resolve → record_event("approval", id, "approval_resolved", {decision})
- esign sign → record_event("esign", id, "signed", {...})
埋点 best-effort（失败 tracing::error 不 propagate，审计不阻塞业务，复用 M10 audit 模式）

## 关键决策
1. event_seq 应用层取 MAX+1（非 BIGSERIAL，per-entity 单调，回放有序）
2. WORM 用 PG trigger（DB 层强制，复用 M10 模式）
3. CAS 用 PG BYTEA + content_hash PK（去重 ON CONFLICT，MVP 不上 S3）
4. replay_state 应用层 merge（非 SQL 物化，简单）
5. Lineage SQL JOIN 拼装（非图数据库，MVP 够用）
6. 电子签名 hash 含 signer+entity+version+purpose+timestamp（21 CFR 三要素绑定）
7. 埋点 best-effort 不阻塞（审计失败不拖垮业务，复用 M10 audit_log 模式）

## 风险
- 事件埋点遗漏：MVP 覆盖 6 核心动作（publish×2/run×2/approval/esign），其余后续补
- replay_state merge 简单覆盖（后事件覆盖前事件 payload 字段，MVP 接受）
- content_store BYTEA 大对象性能（MVP case 量级小，大对象迁 S3 留后续）
