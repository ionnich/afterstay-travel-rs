//! Database connection bootstrap: fetch the RDS master secret from Secrets
//! Manager and build a pooled Postgres connection.

use sqlx::postgres::PgPoolOptions;

use crate::error::ApiError;

#[derive(serde::Deserialize)]
struct DbSecret {
    username: String,
    password: String,
    host: String,
    port: u16,
    dbname: String,
}

/// Read the DB secret JSON from Secrets Manager and connect with a 5-connection pool.
#[allow(deprecated)] // aws_config::from_env is fine for our pinned behavior
pub async fn connect(db_secret_id: &str, region: &str) -> Result<sqlx::PgPool, ApiError> {
    let config = aws_config::from_env()
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await;
    let client = aws_sdk_secretsmanager::Client::new(&config);

    let output = client
        .get_secret_value()
        .secret_id(db_secret_id)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("secretsmanager get failed: {e}")))?;
    let secret_string = output
        .secret_string()
        .ok_or_else(|| ApiError::Internal("db secret has no string value".to_string()))?;
    let secret: DbSecret = serde_json::from_str(secret_string).map_err(ApiError::from)?;

    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        secret.username,
        percent_encode(&secret.password),
        secret.host,
        secret.port,
        secret.dbname
    );

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .map_err(ApiError::from)
}

/// Percent-encode a string for safe embedding in a Postgres connection URL.
// ponytail: hand-rolled; swap for the `percent-encoding` crate if this is needed elsewhere.
fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Read an environment variable, defaulting to the empty string when unset.
pub fn secret_env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}
