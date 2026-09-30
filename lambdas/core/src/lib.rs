#[derive(serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub email: Option<String>,
}

pub fn now_epoch() -> i64 {
    chrono::Utc::now().timestamp()
}
