-- M13: 知识库 RAG (pgvector) — roadmap M11 T11-2/T11-3
-- ACL 随索引写入（tenant_id on kb_documents）+ 权限过滤先于召回（WHERE tenant_id before ORDER BY <=> ）
-- + 混合召回（向量 ANN + 关键词 ILIKE）+ 溯源（source_uri/title/snippet）

CREATE EXTENSION IF NOT EXISTS vector;

-- knowledge_bases 既有表（初始 migration 建空壳），ALTER 加 tenant_id 冗余 + description
ALTER TABLE knowledge_bases ADD COLUMN IF NOT EXISTS tenant_id BIGINT;
ALTER TABLE knowledge_bases ADD COLUMN IF NOT EXISTS description TEXT;

CREATE TABLE IF NOT EXISTS kb_documents (
    id           BIGSERIAL PRIMARY KEY,
    kb_id        BIGINT NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    tenant_id    BIGINT NOT NULL,
    title        TEXT NOT NULL,
    content      TEXT NOT NULL,
    source_uri   TEXT,
    acl_json     JSONB NOT NULL DEFAULT '{}'::jsonb,
    embedding    vector(1024) NOT NULL,
    content_hash TEXT,
    tokens       INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_kb_docs_tenant_kb ON kb_documents(tenant_id, kb_id);
CREATE INDEX IF NOT EXISTS idx_kb_docs_embedding ON kb_documents
    USING hnsw (embedding vector_cosine_ops);
