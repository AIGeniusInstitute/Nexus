//! Content-addressable storage (CAS) — WORM object store (M22, P3).
//!
//! Content is keyed by `SHA-256(原文)` (the content hash). Identical content
//! deduplicates (`INSERT ON CONFLICT DO NOTHING` — the existing row is kept,
//! which is safe because the hash is the content's fingerprint: same hash ⇒
//! same bytes). The table is WORM (`prevent_content_modification`):
//! `content_hash` is the primary key and the row can never be updated or
//! deleted — "modifying" content produces a new, different key.
//!
//! M20 stored `entity_version.content_ref` as the full text inline. M22 adds
//! a CAS path: large content is written here and referenced as
//! `"cas:{hash}"` from `content_ref`. Reads detect the `cas:` prefix and
//! dereference via `get_content`. MVP backs onto PG `BYTEA`; an S3/MinIO
//! backend is a later swap behind the same `store_content`/`get_content` API.

use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct StoreReq {
    pub content: String,
    #[serde(default = "default_content_type")]
    pub content_type: String,
}
fn default_content_type() -> String {
    "text/plain".into()
}

/// Store `content` bytes under their SHA-256 hash. Deduplicates silently
/// (ON CONFLICT DO NOTHING). Returns the hash (64 lowercase hex chars).
pub async fn store_content(
    pool: &PgPool,
    content: &[u8],
    content_type: &str,
) -> Result<String> {
    let mut h = Sha256::new();
    h.update(content);
    let hash = format!("{:x}", h.finalize());
    sqlx::query(
        "INSERT INTO content_store (content_hash, content, size, content_type) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (content_hash) DO NOTHING",
    )
    .bind(&hash)
    .bind(content)
    .bind(content.len() as i64)
    .bind(content_type)
    .execute(pool)
    .await
    .map_err(|e| anyhow!("store_content: {e:?}"))?;
    Ok(hash)
}

/// Convenience wrapper for text content.
pub async fn store_text(pool: &PgPool, text: &str) -> Result<String> {
    store_content(pool, text.as_bytes(), "text/plain").await
}

/// Retrieve content by hash. Returns `None` if not found.
pub async fn get_content(pool: &PgPool, hash: &str) -> Result<Option<Vec<u8>>> {
    let row: Option<(Vec<u8>,)> =
        sqlx::query_as("SELECT content FROM content_store WHERE content_hash = $1")
            .bind(hash)
            .fetch_optional(pool)
            .await
            .map_err(|e| anyhow!("get_content: {e:?}"))?;
    Ok(row.map(|(b,)| b))
}

/// If `ref_` is a `cas:{hash}` reference, dereference to the stored bytes;
/// otherwise return `ref_` unchanged (callers treat inline content and CAS
/// refs uniformly). Errors only on a broken CAS ref (hash present but row
/// missing); inline content never fails.
pub async fn resolve_ref(pool: &PgPool, ref_: &str) -> Result<String> {
    if let Some(hash) = ref_.strip_prefix("cas:") {
        match get_content(pool, hash).await? {
            Some(bytes) => Ok(String::from_utf8(bytes)
                .map_err(|e| anyhow!("resolve_ref: content not utf-8: {e:?}"))?),
            None => Err(anyhow!("resolve_ref: cas ref {hash} not found")),
        }
    } else {
        Ok(ref_.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let mut h = Sha256::new();
        h.update(b"hello");
        let a = format!("{:x}", h.finalize());
        let mut h2 = Sha256::new();
        h2.update(b"hello");
        let b = format!("{:x}", h2.finalize());
        assert_eq!(a, b);
        // the well-known SHA-256 of "hello"
        assert_eq!(
            a,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn cas_prefix_detection() {
        assert_eq!("cas:abc".strip_prefix("cas:"), Some("abc"));
        assert_eq!("inline text".strip_prefix("cas:"), None);
    }
}
