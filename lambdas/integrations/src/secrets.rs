//! Load integration API keys from Secrets Manager at cold start.

/// API keys held in memory for the life of the Lambda process.
pub struct Secrets {
    pub anthropic_key: String,
    pub google_places_key: String,
    pub weather_key: String,
}

/// Load all three secrets. Any failure yields an empty string for that key so
/// startup never blocks; the route will fail with a clear "missing key" error
/// instead.
pub async fn load(region: &str) -> Secrets {
    let config = aws_config::from_env()
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await;
    let client = aws_sdk_secretsmanager::Client::new(&config);

    Secrets {
        anthropic_key: get_secret(&client, "afterstay/anthropic").await,
        google_places_key: get_secret(&client, "afterstay/google-places").await,
        weather_key: get_secret(&client, "afterstay/weather").await,
    }
}

async fn get_secret(client: &aws_sdk_secretsmanager::Client, id: &str) -> String {
    match client.get_secret_value().secret_id(id).send().await {
        Ok(out) => out.secret_string().map(extract_key).unwrap_or_default(),
        Err(_) => String::new(),
    }
}

/// Secrets may be a bare API key string or a JSON object like
/// `{"api_key": "..."}`. Extract the key either way.
fn extract_key(secret_string: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(secret_string) {
        for k in ["api_key", "apiKey", "key", "value", "secret"] {
            if let Some(s) = v.get(k).and_then(|x| x.as_str()) {
                return s.to_string();
            }
        }
    }
    secret_string.trim().to_string()
}
