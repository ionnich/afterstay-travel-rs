//! API error type: a JSON envelope `{"error":{"code","message"}}` with the
//! correct HTTP status, plus `From` conversions from common fallible sources.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    pub fn status_code(&self) -> u16 {
        match self {
            ApiError::BadRequest(_) => 400,
            ApiError::Unauthorized => 401,
            ApiError::Forbidden => 403,
            ApiError::NotFound(_) => 404,
            ApiError::Internal(_) => 500,
        }
    }

    /// Serialize as `{"error":{"code":"<snake_case>","message":"<msg>"}}`.
    /// `Internal` never leaks its detail message.
    pub fn body(&self) -> String {
        let (code, message) = match self {
            ApiError::BadRequest(m) => ("bad_request", m.clone()),
            ApiError::Unauthorized => ("unauthorized", "unauthorized".to_string()),
            ApiError::Forbidden => ("forbidden", "forbidden".to_string()),
            ApiError::NotFound(m) => ("not_found", m.clone()),
            ApiError::Internal(_) => ("internal", "internal error".to_string()),
        };
        serde_json::json!({ "error": { "code": code, "message": message } }).to_string()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => ApiError::NotFound("row not found".to_string()),
            sqlx::Error::Database(db)
                if db.is_unique_violation()
                    || db.is_check_violation()
                    || db.is_foreign_key_violation() =>
            {
                ApiError::BadRequest(db.message().to_string())
            }
            _ => ApiError::Internal(e.to_string()),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::BadRequest(format!("invalid JSON: {e}"))
    }
}

impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(_e: jsonwebtoken::errors::Error) -> Self {
        // Never leak token/validation internals to the caller.
        ApiError::Unauthorized
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Internal(format!("http client error: {e}"))
    }
}
