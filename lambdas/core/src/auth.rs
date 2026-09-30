//! Cognito JWT verification + per-trip membership authorization (replaces
//! Supabase RLS). Never trust a client-supplied `user_id`; always use the `sub`
//! from the verified token.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use parking_lot::Mutex;
use serde::Deserialize;

use crate::error::ApiError;

#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub email: Option<String>,
}

struct CachedJwks {
    keys: Vec<serde_json::Value>,
    fetched_at: Instant,
}

const JWKS_TTL: Duration = Duration::from_secs(5 * 60);

static JWKS_CACHE: LazyLock<Mutex<CachedJwks>> = LazyLock::new(|| {
    Mutex::new(CachedJwks {
        keys: Vec::new(),
        // Start stale so the first call triggers a fetch.
        fetched_at: Instant::now() - JWKS_TTL,
    })
});

async fn fetch_keys(pool_id: &str, region: &str) -> Result<Vec<serde_json::Value>, ApiError> {
    let url = format!("https://cognito-idp.{region}.amazonaws.com/{pool_id}/.well-known/jwks.json");
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| ApiError::Internal(format!("jwks fetch failed: {e}")))?;
    #[derive(Deserialize)]
    struct Jwks {
        keys: Vec<serde_json::Value>,
    }
    let jwks: Jwks = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("jwks parse failed: {e}")))?;
    Ok(jwks.keys)
}

async fn key_for_kid(
    kid: &str,
    pool_id: &str,
    region: &str,
) -> Result<serde_json::Value, ApiError> {
    // Fast path: cache hit within TTL.
    {
        let cache = JWKS_CACHE.lock();
        if cache.fetched_at.elapsed() < JWKS_TTL {
            if let Some(key) = cache
                .keys
                .iter()
                .find(|k| k.get("kid").and_then(|v| v.as_str()) == Some(kid))
            {
                return Ok(key.clone());
            }
        }
    }

    let keys = fetch_keys(pool_id, region).await?;
    let mut cache = JWKS_CACHE.lock();
    *cache = CachedJwks {
        keys: keys.clone(),
        fetched_at: Instant::now(),
    };
    keys.into_iter()
        .find(|k| k.get("kid").and_then(|v| v.as_str()) == Some(kid))
        .ok_or(ApiError::Unauthorized)
}

/// Verify a `Bearer <token>` header against the Cognito pool JWKS and return
/// the decoded claims. Missing/empty header, bad signature, or expired token
/// all yield [`ApiError::Unauthorized`].
pub async fn verify_jwt(header: &str, pool_id: &str, region: &str) -> Result<Claims, ApiError> {
    let token = header
        .strip_prefix("Bearer ")
        .ok_or(ApiError::Unauthorized)?
        .trim();
    if token.is_empty() {
        return Err(ApiError::Unauthorized);
    }

    let kid = decode_header(token)
        .map_err(ApiError::from)?
        .kid
        .ok_or(ApiError::Unauthorized)?;

    let jwk_value = key_for_kid(&kid, pool_id, region).await?;
    let jwk: jsonwebtoken::jwk::Jwk =
        serde_json::from_value(jwk_value).map_err(|_| ApiError::Unauthorized)?;
    let decoding_key = DecodingKey::from_jwk(&jwk).map_err(ApiError::from)?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = true;
    // Cognito access tokens carry no `aud` claim; skip audience validation.
    validation.validate_aud = false;

    let data = decode::<Claims>(token, &decoding_key, &validation).map_err(ApiError::from)?;
    Ok(data.claims)
}

/// 403 unless `sub` is a row in `trip_members` for `trip_id`.
pub async fn assert_trip_member(
    pool: &sqlx::PgPool,
    sub: &str,
    trip_id: uuid::Uuid,
) -> Result<(), ApiError> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM trip_members WHERE trip_id = $1 AND user_id = $2 LIMIT 1",
    )
    .bind(trip_id)
    .bind(sub)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from)?;

    row.map(|_| ()).ok_or(ApiError::Forbidden)
}
