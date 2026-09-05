//! Authentication: AuthProvider trait + local JWT provider + axum extractor (T1-3).
//!
//! M1 simplification: OIDC is abstracted as `AuthProvider`; `LocalProvider`
//! (username + bcrypt + JWT) implements it for M1 single-tenant. `OidcProvider`
//! (Keycloak Authorization Code + PKCE) is M5.

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::db;

/// JWT claims. `perms` is a snapshot at login time; WS re-checks live from DB
/// to support revocation-driven disconnect (AC4.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // email
    pub uid: i64,
    pub tid: i64,
    pub perms: Vec<String>,
    pub exp: usize,
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Authenticate and return (claims, signed jwt).
    async fn login(&self, email: &str, password: &str) -> Result<(Claims, String), AuthError>;
}

#[derive(Debug)]
pub enum AuthError {
    InvalidCredentials,
    Other(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::InvalidCredentials => write!(f, "invalid credentials"),
            AuthError::Other(m) => write!(f, "{m}"),
        }
    }
}
impl std::error::Error for AuthError {}
impl IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AuthError::InvalidCredentials => StatusCode::UNAUTHORIZED.into_response(),
            AuthError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

pub struct JwtIssuer {
    enc: EncodingKey,
    dec: DecodingKey,
    ttl_seconds: i64,
}

impl JwtIssuer {
    pub fn new(secret: &str, ttl_seconds: i64) -> Self {
        Self {
            enc: EncodingKey::from_secret(secret.as_bytes()),
            dec: DecodingKey::from_secret(secret.as_bytes()),
            ttl_seconds,
        }
    }

    pub fn issue(&self, claims: Claims) -> Result<String, AuthError> {
        let exp = (Utc::now() + Duration::seconds(self.ttl_seconds)).timestamp() as usize;
        let mut claims = claims;
        claims.exp = exp;
        encode(&jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256), &claims, &self.enc)
            .map_err(|e| AuthError::Other(e.to_string()))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, AuthError> {
        decode::<Claims>(token, &self.dec, &Validation::new(jsonwebtoken::Algorithm::HS256))
            .map(|d| d.claims)
            .map_err(|_| AuthError::InvalidCredentials)
    }
}

pub struct LocalProvider {
    pool: PgPool,
    jwt: JwtIssuer,
}

impl LocalProvider {
    pub fn new(pool: PgPool, jwt: JwtIssuer) -> Self {
        Self { pool, jwt }
    }
}

#[async_trait]
impl AuthProvider for LocalProvider {
    async fn login(&self, email: &str, password: &str) -> Result<(Claims, String), AuthError> {
        let row: Option<(i64, i64, Option<String>, String)> = sqlx::query_as(
            "SELECT u.id, u.tenant_id, u.password_hash, u.email
             FROM users u JOIN tenants t ON u.tenant_id = t.id
             WHERE u.email = $1 AND t.slug='default' AND u.status='active'",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AuthError::Other(e.to_string()))?;
        let (uid, tid, pw_hash, mail) = match row {
            Some(r) => r,
            None => return Err(AuthError::InvalidCredentials),
        };
        let pw_hash = pw_hash.ok_or(AuthError::InvalidCredentials)?;
        let ok = bcrypt::verify(password, &pw_hash).map_err(|_| AuthError::InvalidCredentials)?;
        if !ok {
            return Err(AuthError::InvalidCredentials);
        }
        let perms = db::user_permissions(&self.pool, uid)
            .await
            .map_err(|e| AuthError::Other(e.to_string()))?;
        let claims = Claims {
            sub: mail,
            uid,
            tid,
            perms,
            exp: 0,
        };
        let token = self.jwt.issue(claims.clone())?;
        Ok((claims, token))
    }
}

/// Axum extractor: pulls and verifies the Bearer JWT, yielding the claims.
pub struct AuthUser(pub Claims);

// axum 0.8 FromRequestParts uses native async fn (RPITIT); do NOT wrap with
// #[async_trait] — its lifetime desugaring conflicts with the trait declaration.
impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // The actual jwt verifier is stashed in extensions by the server bootstrap.
        let issuer = parts
            .extensions
            .get::<std::sync::Arc<JwtIssuer>>()
            .cloned()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let auth = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let claims = issuer.verify(auth).map_err(|_| StatusCode::UNAUTHORIZED)?;
        Ok(AuthUser(claims))
    }
}
