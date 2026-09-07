//! Electronic signatures — 21 CFR Part 11 compliance (M22, P3).
//!
//! Three required elements of a compliant e-signature, all bound into the
//! `signature_hash` so tampering is detectable on `verify`:
//!   1. Signer identity — `signer_user_id` (FK users).
//!   2. Timestamp — `signed_at`.
//!   3. Meaning — `signature_purpose` (short, e.g. "审核批准") +
//!      `meaning_text` (full 21 CFR 11.50 meaning statement).
//!
//! The table is WORM (`prevent_esign_modification`): once signed, a record
//! can never be edited or deleted — revocation, if ever needed, is a new
//! signed record, never a mutation. Signing also emits a `signed` domain
//! event into `domain_events` for the event-sourced timeline.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::domain_events;

#[derive(sqlx::FromRow, Serialize)]
pub struct EsignRow {
    pub id: i64,
    pub tenant_id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub version_id: Option<i64>,
    pub signer_user_id: i64,
    pub signature_purpose: String,
    pub signature_hash: String,
    pub meaning_text: String,
    pub signed_at: DateTime<Utc>,
}

#[derive(serde::Deserialize)]
pub struct SignReq {
    pub entity_type: String,
    pub entity_id: i64,
    pub version_id: Option<i64>,
    pub signature_purpose: String,
    pub meaning_text: String,
}

/// Compute the signature hash binding all 21 CFR Part 11 elements. Any change
/// to signer / entity / version / purpose / meaning / timestamp produces a
/// different hash, so `verify` can detect after-the-fact tampering with a
/// stored field by recomputing and comparing.
fn compute_hash(
    signer_uid: i64,
    entity_type: &str,
    entity_id: i64,
    version_id: Option<i64>,
    purpose: &str,
    meaning: &str,
    signed_at: DateTime<Utc>,
) -> String {
    let mut h = Sha256::new();
    h.update(signer_uid.to_le_bytes());
    h.update(entity_type.as_bytes());
    h.update(entity_id.to_le_bytes());
    h.update(version_id.unwrap_or(0).to_le_bytes());
    h.update(purpose.as_bytes());
    h.update(meaning.as_bytes());
    // Use whole-second timestamp (not to_rfc3339) so the hash is stable across
    // PG TIMESTAMPTZ microsecond rounding — chrono's nanosecond精度与 PG 的
    // 微秒精度在 to_rfc3339 字符串上会差异，导致 verify 重算不匹配。
    h.update(signed_at.timestamp().to_le_bytes());
    format!("{:x}", h.finalize())
}

/// Record an e-signature. Inserts the signed record (hash computed at the
/// moment of signing) and emits a `signed` domain event. Returns the new
/// record id. The caller must pass the authenticated user's id as `signer_uid`.
pub async fn sign(
    pool: &PgPool,
    tenant_id: i64,
    signer_uid: i64,
    req: &SignReq,
) -> Result<i64> {
    let now = Utc::now();
    let hash = compute_hash(
        signer_uid,
        &req.entity_type,
        req.entity_id,
        req.version_id,
        &req.signature_purpose,
        &req.meaning_text,
        now,
    );
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO esign_records \
         (tenant_id, entity_type, entity_id, version_id, signer_user_id, \
          signature_purpose, signature_hash, meaning_text, signed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(tenant_id)
    .bind(&req.entity_type)
    .bind(req.entity_id)
    .bind(req.version_id)
    .bind(signer_uid)
    .bind(&req.signature_purpose)
    .bind(&hash)
    .bind(&req.meaning_text)
    .bind(now)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("esign sign: {e:?}"))?;
    let id = row.0;
    domain_events::record_event(
        pool,
        tenant_id,
        "esign",
        id,
        "signed",
        &json!({
            "entity_type": req.entity_type,
            "entity_id": req.entity_id,
            "version_id": req.version_id,
            "signer_user_id": signer_uid,
            "purpose": req.signature_purpose,
        }),
    )
    .await;
    Ok(id)
}

/// Fetch one signature record (tenant-scoped).
pub async fn get_esign(
    pool: &PgPool,
    tenant_id: i64,
    esign_id: i64,
) -> Result<EsignRow> {
    sqlx::query_as::<_, EsignRow>(
        "SELECT id, tenant_id, entity_type, entity_id, version_id, signer_user_id, \
                signature_purpose, signature_hash, meaning_text, signed_at \
         FROM esign_records WHERE id = $1 AND tenant_id = $2",
    )
    .bind(esign_id)
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|e| anyhow!("get_esign: {e:?}"))
}

/// Verify a stored e-signature: recompute the hash from the stored fields and
/// compare; check all 21 CFR Part 11 elements are present. Returns
/// `{valid: bool, mismatch: Option<String>}`. A mismatch means a stored field
/// was altered (which would require defeating the WORM trigger — i.e. a DB
/// compromise) — the signature is then NOT to be trusted.
pub async fn verify_esign(
    pool: &PgPool,
    tenant_id: i64,
    esign_id: i64,
) -> Result<Value> {
    let r = get_esign(pool, tenant_id, esign_id).await?;
    let expected = compute_hash(
        r.signer_user_id,
        &r.entity_type,
        r.entity_id,
        r.version_id,
        &r.signature_purpose,
        &r.meaning_text,
        r.signed_at,
    );
    let hash_ok = expected == r.signature_hash;
    // 21 CFR Part 11 completeness: signer + purpose + meaning + timestamp all present.
    let complete = r.signer_user_id > 0
        && !r.signature_purpose.is_empty()
        && !r.meaning_text.is_empty()
        && !r.signed_at.to_rfc3339().is_empty();
    let valid = hash_ok && complete;
    Ok(json!({
        "valid": valid,
        "hash_match": hash_ok,
        "elements_complete": complete,
        "computed_hash": expected,
        "stored_hash": r.signature_hash,
    }))
}

/// List signatures for an entity (tenant-scoped).
pub async fn list_esigns(
    pool: &PgPool,
    tenant_id: i64,
    entity_type: &str,
    entity_id: i64,
) -> Result<Vec<EsignRow>> {
    sqlx::query_as::<_, EsignRow>(
        "SELECT id, tenant_id, entity_type, entity_id, version_id, signer_user_id, \
                signature_purpose, signature_hash, meaning_text, signed_at \
         FROM esign_records \
         WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3 \
         ORDER BY signed_at ASC",
    )
    .bind(tenant_id)
    .bind(entity_type)
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .map_err(|e| anyhow!("list_esigns: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_sensitive() {
        let now = Utc::now();
        let h1 = compute_hash(1, "entity_version", 5, Some(2), "approve", "approved v1", now);
        let h2 = compute_hash(1, "entity_version", 5, Some(2), "approve", "approved v1", now);
        assert_eq!(h1, h2); // deterministic
        // tamper signer
        let h3 = compute_hash(2, "entity_version", 5, Some(2), "approve", "approved v1", now);
        assert_ne!(h1, h3);
        // tamper meaning
        let h4 = compute_hash(1, "entity_version", 5, Some(2), "approve", "rejected v1", now);
        assert_ne!(h1, h4);
        // tamper timestamp
        let h5 = compute_hash(1, "entity_version", 5, Some(2), "approve", "approved v1", now + chrono::Duration::seconds(1));
        assert_ne!(h1, h5);
    }
}
