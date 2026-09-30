//! Integrations Lambda: Anthropic, Google Places, and Weather. Every endpoint
//! is called by an authenticated client; we verify the Cognito JWT (when the
//! pool id is configured) but do NOT do trip-membership authorization here.

mod anthropic;
mod places;
mod secrets;

use std::sync::Arc;

use lambda_http::{run, service_fn, Body, Error, Request};
use lambda_http::http::Response;
use serde::de::DeserializeOwned;
use serde::Serialize;

use core::error::ApiError;

struct State {
    secrets: secrets::Secrets,
    client: reqwest::Client,
    pool_id: String,
    region: String,
}

fn main() -> Result<(), Error> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime");
    runtime.block_on(async {
        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "ap-southeast-1".to_string());
        let pool_id = std::env::var("COGNITO_USER_POOL_ID").unwrap_or_default();
        let secrets = secrets::load(&region).await;
        let state = Arc::new(State {
            secrets,
            client: reqwest::Client::new(),
            pool_id,
            region,
        });
        let handler = move |req: Request| {
            let state = state.clone();
            async move { handle(req, &state).await }
        };
        run(service_fn(handler)).await
    })
}

async fn handle(req: Request, state: &State) -> Result<Response<Body>, Error> {
    // Optional JWT check — skip when no pool is configured (local/dev).
    if !state.pool_id.is_empty() {
        let auth = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if let Err(e) = core::auth::verify_jwt(auth, &state.pool_id, &state.region).await {
            return Ok(api_error_response(e));
        }
    }

    let response = match dispatch(&req, state).await {
        Ok(resp) => resp,
        Err(e) => api_error_response(e),
    };
    Ok(response)
}

async fn dispatch(req: &Request, state: &State) -> Result<Response<Body>, ApiError> {
    let method = req.method().as_str();
    let path = strip_prefix(req.uri().path());
    let query = req.uri().query();

    match (method, path) {
        ("POST", "/anthropic/recommendations") => {
            let args: anthropic::RecommendationsArgs = read_body(req).await?;
            let recs =
                anthropic::generate_recommendations(&state.client, &state.secrets.anthropic_key, &args)
                    .await?;
            json(200, &recs)
        }
        ("POST", "/anthropic/itinerary") => {
            let args: anthropic::ItineraryArgs = read_body(req).await?;
            let days =
                anthropic::generate_itinerary(&state.client, &state.secrets.anthropic_key, &args)
                    .await?;
            json(200, &days)
        }
        ("POST", "/anthropic/receipt") => {
            let req_: anthropic::ReceiptRequest = read_body(req).await?;
            let scanned =
                anthropic::scan_receipt(&state.client, &state.secrets.anthropic_key, &req_).await?;
            json(200, &scanned)
        }
        ("POST", "/anthropic/scan-trip") => {
            let req_: anthropic::ScanTripRequest = read_body(req).await?;
            let details = anthropic::scan_trip_documents(
                &state.client,
                &state.secrets.anthropic_key,
                &req_,
            )
            .await?;
            json(200, &details)
        }
        ("GET", "/places/nearby") => {
            let type_ = query_param(query, "type");
            let keyword = query_param(query, "keyword");
            let results = places::search_nearby(
                &state.client,
                &state.secrets.google_places_key,
                type_.as_deref(),
                keyword.as_deref(),
            )
            .await?;
            json(200, &results)
        }
        ("GET", "/places/autocomplete") => {
            let input = query_param(query, "input").unwrap_or_default();
            let results = places::place_autocomplete(
                &state.client,
                &state.secrets.google_places_key,
                &input,
            )
            .await?;
            json(200, &results)
        }
        ("GET", "/places/details") => {
            let place_id = query_param(query, "placeId").unwrap_or_default();
            let details =
                places::get_place_details(&state.client, &state.secrets.google_places_key, &place_id)
                    .await?;
            json(200, &details)
        }
        ("POST", "/places/enrich") => {
            let req_: places::EnrichRequest = read_body(req).await?;
            let enriched = places::enrich_recommendations(
                &state.client,
                &state.secrets.google_places_key,
                &req_,
            )
            .await?;
            json(200, &enriched)
        }
        ("GET", "/places/location") => {
            let place_id = query_param(query, "placeId").unwrap_or_default();
            let location =
                places::get_place_location(&state.client, &state.secrets.google_places_key, &place_id)
                    .await?;
            json(200, &location)
        }
        ("GET", "/weather") => {
            let location = query_param(query, "location").unwrap_or_default();
            let forecast = weather(&state.client, &state.secrets.weather_key, &location).await?;
            json(200, &forecast)
        }
        _ => Err(ApiError::NotFound(format!("no route for {method} {path}"))),
    }
}

async fn weather(
    client: &reqwest::Client,
    key: &str,
    location: &str,
) -> Result<serde_json::Value, ApiError> {
    if key.is_empty() {
        return Err(ApiError::Internal("missing weather API key".to_string()));
    }
    let mut url = reqwest::Url::parse("https://api.weatherapi.com/v1/forecast.json")
        .expect("static weather URL is valid");
    url.query_pairs_mut()
        .append_pair("key", key)
        .append_pair("q", location)
        .append_pair("days", "5")
        .append_pair("aqi", "no")
        .append_pair("alerts", "no");

    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("weather request failed: {e}")))?;
    if !res.status().is_success() {
        return Err(ApiError::Internal(format!(
            "Weather {}: {}",
            res.status(),
            res.text().await.unwrap_or_default()
        )));
    }
    res.json().await.map_err(ApiError::from)
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn strip_prefix(path: &str) -> &str {
    for p in ["/v1/integrations", "/integrations"] {
        if let Some(rest) = path.strip_prefix(p) {
            return rest;
        }
    }
    path
}

fn query_param(query: Option<&str>, name: &str) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == name {
                return Some(percent_decode(v));
            }
        }
    }
    None
}

/// Minimal percent-decoding for query values.
// ponytail: byte-level decode, ASCII-safe for the values used here (place ids,
// keywords, locations). Swap for the `url` crate if non-ASCII input appears.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn read_body<T: DeserializeOwned>(req: &Request) -> Result<T, ApiError> {
    let bytes = match req.body() {
        Body::Text(s) => s.clone().into_bytes(),
        Body::Binary(b) => b.clone(),
        Body::Empty => Vec::new(),
    };
    serde_json::from_slice(&bytes).map_err(ApiError::from)
}

fn json<T: Serialize>(status: u16, value: &T) -> Result<Response<Body>, ApiError> {
    let body = serde_json::to_string(value).map_err(|e| ApiError::Internal(e.to_string()))?;
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .map_err(|e| ApiError::Internal(e.to_string()))
}

fn api_error_response(e: ApiError) -> Response<Body> {
    Response::builder()
        .status(e.status_code())
        .header("content-type", "application/json")
        .body(Body::from(e.body()))
        .unwrap()
}
