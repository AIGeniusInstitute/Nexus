# Nexus M13 技术方案 — 知识库 RAG（pgvector）

## 1. 架构定位
M13 在控制面 `codex-rs/nexus-control` 内新增 `kb.rs` 模块 + `kb_documents` 表。复用：
- **多租户隔离**（M1 RBAC + tenant_id 硬过滤，与 audit/eval/timeline 同模式）
- **reqwest HTTP 客户端**（M8 引入 reqwest+rustls，复用做 embedding API 调用）
- **PG 连接池**（sqlx，M1 起）

## 2. 数据模型

### 2.1 knowledge_bases（既有表，ALTER 加列）
```sql
ALTER TABLE knowledge_bases ADD COLUMN IF NOT EXISTS tenant_id BIGINT;
ALTER TABLE knowledge_bases ADD COLUMN IF NOT EXISTS description TEXT;
```
tenant_id 冗余自 workspaces.tenant_id（摄入时查 KB→workspace→tenant 写入 kb_documents.tenant_id，ANN 预过滤不用 JOIN）。

### 2.2 kb_documents（新表）
```sql
CREATE EXTENSION IF NOT EXISTS vector;  -- 可移植性
CREATE TABLE IF NOT EXISTS kb_documents (
    id           BIGSERIAL PRIMARY KEY,
    kb_id        BIGINT NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    tenant_id    BIGINT NOT NULL,          -- ACL 随索引写入，ANN 预过滤
    title        TEXT NOT NULL,
    content      TEXT NOT NULL,
    source_uri   TEXT,
    acl_json     JSONB NOT NULL DEFAULT '{}'::jsonb,  -- 细粒度 ACL 扩展位
    embedding    vector(1024) NOT NULL,    -- text-embedding-v3 1024 维
    content_hash TEXT,
    tokens       INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_kb_docs_tenant_kb ON kb_documents(tenant_id, kb_id);
CREATE INDEX IF NOT EXISTS idx_kb_docs_embedding ON kb_documents
    USING hnsw (embedding vector_cosine_ops);
```

## 3. kb.rs 模块

### 3.1 embedding 客户端
```rust
async fn embed(text: &str) -> Result<Vec<f32>>
```
- POST `{NEXUS_UPSTREAM_MODEL_URL 基址}/embeddings`，model=`NEXUS_EMBED_MODEL`（default `text-embedding-v3`），key=`NEXUS_MODEL_KEY`
- 复用 M8 model_gateway 的 env 变量（同一 dashscope 凭证）
- 返回 `data[0].embedding`（Vec<f32>，1024 维）

### 3.2 CRUD
- `create_kb(pool, tenant_id, workspace_id, name, description) -> i64`：INSERT knowledge_bases(tenant_id, workspace_id, name)
- `list_kbs(pool, tenant_id) -> Vec<KbRow>`
- `ingest_document(pool, tenant_id, kb_id, title, content, source_uri) -> i64`：校验 KB tenant → embed → INSERT kb_documents(tenant_id 随索引)
- `search(pool, tenant_id, kb_id, query, keyword, top_k) -> Vec<SearchHit>`：embed query → ANN + tenant 预过滤 + 可选 keyword ILIKE → 溯源
- `list_documents / delete_document`

### 3.3 混合召回 SQL
```sql
-- 权限过滤先于召回：tenant_id WHERE 在 ORDER BY ... <=> 之前
SELECT id, title, source_uri, LEFT(content, 200) AS snippet,
       1 - (embedding <=> $1::vector) AS score
FROM kb_documents
WHERE tenant_id = $2 AND kb_id = $3
  AND ($4::text IS NULL OR content ILIKE '%' || $4 || '%')  -- 可选关键词
ORDER BY embedding <=> $1::vector
LIMIT $5;
```
score = `1 - cosine_distance`（余弦相似度，越高越相关）。

## 4. HTTP 路由
```
POST   /v1/kbs                      create_kb  (body: workspace_id, name, description)
GET    /v1/kbs                       list_kbs
POST   /v1/kbs/{id}/documents        ingest_document (body: title, content, source_uri)
GET    /v1/kbs/{id}/documents        list_documents
DELETE /v1/kbs/{id}/documents/{did}  delete_document
POST   /v1/kbs/{id}/search           search (body: query, keyword?, top_k?)
```
所有路由 tenant_id 从 AuthUser 提取（M1 模式），跨租户不可见。

## 5. 关键决策
1. **权限过滤先于召回**：`WHERE tenant_id=$X` 在 `ORDER BY embedding <=>` 之前，pgvector HNSW 支持过滤 ANN（0.8.x）
2. **ACL 随索引写入**：kb_documents.tenant_id 在摄入时写入（不召回时 JOIN）
3. **冗余 tenant_id**：避免 ANN 时 JOIN knowledge_bases→workspaces（性能 + 简单）
4. **embedding 复用 M8 env**：NEXUS_UPSTREAM_MODEL_URL/NEXUS_MODEL_KEY，加 NEXUS_EMBED_MODEL
5. **混合召回最小实现**：向量 ANN + 可选 ILIKE 关键词过滤；rerank 留扩展（向量距离排序足够 MVP）
6. **溯源**：返回 source_uri + title + snippet(LEFT content 200) + score
7. **Simplicity First**：单文档单向量（不分块）；细粒度 ACL 列保留但不实现（tenant_id 硬隔离已满足 MVP 安全）

## 6. 坑预判
- pgvector `vector(1024)` 列需固定维度；embedding API 返回必须 1024（已验证 text-embedding-v3=1024）
- `embedding <=> $1::vector`：sqlx bind Vec<f32> 需 `&format!("'[{}]'", vec.join(","))::vector` 或用 `sqlx::types::PgIsVector`？实际 sqlx-pgvector 需 feature。**决策**：手动构造 `'[0.1,0.2,...]'::vector` 字符串 bind，不引 sqlx-pgvector 依赖（Surgical，避免 workspace 加 feature）
- HNSW 索引在空表建没问题；大量摄入后重建慢（M13 数据量小，OK）
- cosine_ops：embedding 向量需归一化？pgvector `<=>` 是余弦距离，不要求归一化（内部分母处理），但归一化更稳。dashscope embedding 已近似归一化，直接用
