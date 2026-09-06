//! M13 知识库 RAG (pgvector) — roadmap M11 T11-2/T11-3.
//!
//! 文档摄入（embedding 索引）→ 混合召回（向量 ANN + 关键词过滤）→ 溯源。
//! ACL 随索引写入（tenant_id on kb_documents）；权限过滤先于召回
//!（`WHERE tenant_id` 在 `ORDER BY embedding <=>` 之前）。

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

/// 维度（text-embedding-v3 = 1024）。表列 vector(1024) 必须匹配。
const EMBED_DIM: usize = 1024;

#[derive(sqlx::FromRow, Serialize)]
pub struct KbRow {
    pub id: i64,
    pub workspace_id: Option<i64>,
    pub tenant_id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct KbDocRow {
    pub id: i64,
    pub kb_id: i64,
    pub title: String,
    pub source_uri: Option<String>,
    pub content_hash: Option<String>,
    pub tokens: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct SearchHit {
    pub id: i64,
    pub title: String,
    pub source_uri: Option<String>,
    pub snippet: String,
    pub score: f64,
}

// ---- embedding 客户端 ----

/// 调 dashscope /v1/embeddings 生成向量。复用 M8 env：
/// - NEXUS_UPSTREAM_MODEL_URL（base，如 https://dashscope.aliyuncs.com/compatible-mode/v1）
/// - NEXUS_MODEL_KEY（凭证，绝不日志/记忆）
/// - NEXUS_EMBED_MODEL（default text-embedding-v3）
pub async fn embed(text: &str) -> Result<Vec<f32>> {
    let base = std::env::var("NEXUS_UPSTREAM_MODEL_URL")
        .unwrap_or_else(|_| "https://dashscope.aliyuncs.com/compatible-mode/v1".into());
    let key = std::env::var("NEXUS_MODEL_KEY")
        .context("NEXUS_MODEL_KEY not set (embedding needs model credentials)")?;
    let model =
        std::env::var("NEXUS_EMBED_MODEL").unwrap_or_else(|_| "text-embedding-v3".into());

    let url = format!("{}/embeddings", base.trim_end_matches('/'));
    let body = serde_json::json!({ "model": model, "input": text });
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .context("build reqwest client")?;
    let resp = client
        .post(&url)
        .bearer_auth(&key)
        .json(&body)
        .send()
        .await
        .context("embed request")?;
    if !resp.status().is_success() {
        let st = resp.status();
        let txt = resp.text().await.unwrap_or_default();
        anyhow::bail!("embed upstream error {st}: {txt}");
    }
    let v: Value = resp.json().await.context("decode embed response")?;
    let arr = v
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|d| d.get("embedding"))
        .and_then(|e| e.as_array())
        .context("embed response missing data[0].embedding")?;
    let vec: Vec<f32> = arr
        .iter()
        .filter_map(|x| x.as_f64().map(|f| f as f32))
        .collect();
    if vec.len() != EMBED_DIM {
        anyhow::bail!(
            "embed dim mismatch: got {} expected {}",
            vec.len(),
            EMBED_DIM
        );
    }
    Ok(vec)
}

/// 构造 pgvector 字面量 `'[0.1,0.2,...]'::vector`（sqlx 不引 pgvector feature，
/// 手动 bind 字符串，Surgical 避免 workspace 加依赖）。
fn vec_literal(v: &[f32]) -> String {
    let s: Vec<String> = v.iter().map(|x| format!("{x}")).collect();
    format!("[{}]", s.join(","))
}

// ---- CRUD ----

pub async fn create_kb(
    pool: &PgPool,
    tenant_id: i64,
    workspace_id: Option<i64>,
    name: &str,
    description: Option<&str>,
) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO knowledge_bases (workspace_id, name, tenant_id, description) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(workspace_id)
    .bind(name)
    .bind(tenant_id)
    .bind(description)
    .fetch_one(pool)
    .await
    .context("create kb")?;
    Ok(row.0)
}

pub async fn list_kbs(pool: &PgPool, tenant_id: i64) -> Result<Vec<KbRow>> {
    sqlx::query_as(
        "SELECT id, workspace_id, tenant_id, name, description, created_at \
         FROM knowledge_bases WHERE tenant_id=$1 ORDER BY created_at DESC LIMIT 200",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .context("list kbs")
    .map_err(Into::into)
}

pub async fn ingest_document(
    pool: &PgPool,
    tenant_id: i64,
    kb_id: i64,
    title: &str,
    content: &str,
    source_uri: Option<&str>,
) -> Result<i64> {
    // 校验 KB 归属当前租户（ACL 先于写入）
    let owned: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM knowledge_bases WHERE id=$1 AND tenant_id=$2",
    )
    .bind(kb_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .context("kb tenant check")?;
    if owned.is_none() {
        anyhow::bail!("kb {kb_id} not found or not owned by tenant {tenant_id}");
    }
    let emb = embed(content).await?;
    let lit = vec_literal(&emb);
    // content_hash 仅 dedup/debug（非安全场景，用 std hash 避免加依赖）
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    let content_hash = format!("{:016x}", h.finish());
    let tokens = content.split_whitespace().count() as i32;
    // tenant_id 随索引写入（ANN 预过滤，召回时不 JOIN）
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO kb_documents \
         (kb_id, tenant_id, title, content, source_uri, embedding, content_hash, tokens) \
         VALUES ($1, $2, $3, $4, $5, $6::vector, $7, $8) RETURNING id",
    )
    .bind(kb_id)
    .bind(tenant_id)
    .bind(title)
    .bind(content)
    .bind(source_uri)
    .bind(&lit)
    .bind(&content_hash)
    .bind(tokens)
    .fetch_one(pool)
    .await
    .context("ingest document")?;
    Ok(row.0)
}

pub async fn list_documents(pool: &PgPool, tenant_id: i64, kb_id: i64) -> Result<Vec<KbDocRow>> {
    sqlx::query_as(
        "SELECT id, kb_id, title, source_uri, content_hash, tokens, created_at \
         FROM kb_documents WHERE tenant_id=$1 AND kb_id=$2 ORDER BY created_at DESC LIMIT 200",
    )
    .bind(tenant_id)
    .bind(kb_id)
    .fetch_all(pool)
    .await
    .context("list documents")
    .map_err(Into::into)
}

pub async fn delete_document(pool: &PgPool, tenant_id: i64, doc_id: i64) -> Result<bool> {
    let res = sqlx::query("DELETE FROM kb_documents WHERE id=$1 AND tenant_id=$2")
        .bind(doc_id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .context("delete document")?;
    Ok(res.rows_affected() > 0)
}

/// 混合召回：向量 ANN（HNSW 余弦）+ tenant_id 预过滤 + 可选关键词 ILIKE + 溯源。
/// 权限过滤先于召回：`WHERE tenant_id` 在 `ORDER BY embedding <=>` 之前。
pub async fn search(
    pool: &PgPool,
    tenant_id: i64,
    kb_id: i64,
    query: &str,
    keyword: Option<&str>,
    top_k: i64,
) -> Result<Vec<SearchHit>> {
    let emb = embed(query).await?;
    let lit = vec_literal(&emb);
    // NULL-or-equal 模式：keyword=None 不加过滤；否则匹配 title OR content（混合召回）
    let rows: Vec<(i64, String, Option<String>, String, f64)> = sqlx::query_as(
        "SELECT id, title, source_uri, LEFT(content, 200) AS snippet, \
         1 - (embedding <=> $1::vector) AS score \
         FROM kb_documents \
         WHERE tenant_id=$2 AND kb_id=$3 \
           AND ($4::text IS NULL OR title ILIKE '%' || $4 || '%' OR content ILIKE '%' || $4 || '%') \
         ORDER BY embedding <=> $1::vector \
         LIMIT $5",
    )
    .bind(&lit)
    .bind(tenant_id)
    .bind(kb_id)
    .bind(keyword)
    .bind(top_k)
    .fetch_all(pool)
    .await
    .context("kb search")?;
    Ok(rows
        .into_iter()
        .map(|(id, title, source_uri, snippet, score)| SearchHit {
            id,
            title,
            source_uri,
            snippet,
            score,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_literal_format() {
        let v = vec![0.1_f32, 0.2, 0.3];
        let lit = vec_literal(&v);
        assert!(lit.starts_with('['));
        assert!(lit.ends_with(']'));
        // 三个分量
        assert_eq!(lit.matches(',').count(), 2);
    }

    #[test]
    fn embed_dim_constant() {
        // text-embedding-v3 = 1024；表列 vector(1024) 必须匹配
        assert_eq!(EMBED_DIM, 1024);
    }
}
