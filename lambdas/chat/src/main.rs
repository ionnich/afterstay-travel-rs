//! Chat WebSocket Lambda: connection registry + message fan-out.
//!
//! Handles API Gateway WebSocket `$connect` / `$disconnect` / `$default`:
//! - `$connect`    verifies the JWT + trip membership, then registers the
//!                 connection in DynamoDB (`afterstay-ws-connections`).
//! - `$disconnect` removes the connection row.
//! - `$default`    receives `{action:"sendMessage", message}` bodies, inserts a
//!                 `chat_messages` row, and fans the new message out to every
//!                 connection sharing the same `tripId` via the
//!                 API Gateway Management `@connections` endpoint.

use std::time::{SystemTime, UNIX_EPOCH};

use aws_sdk_dynamodb::types::AttributeValue;
use lambda_http::http::Response;
use lambda_http::request::RequestContext;
use lambda_http::{run, service_fn, tracing, Body, Error, Request, RequestExt};
use serde::Deserialize;

use core::auth::{assert_trip_member, verify_jwt};
use core::db::{connect, secret_env};
use core::error::ApiError;
use core::types::ChatMessage;

/// Registry TTL: connections are refreshed on connect and expire after 2h.
const WS_TTL_SECONDS: i64 = 2 * 60 * 60;

#[derive(Deserialize)]
struct ClientMessage {
    #[serde(default)]
    action: String,
    #[serde(default)]
    message: Option<String>,
}

/// Per-request WebSocket routing facts extracted from the request context.
struct WsCtx {
    event_type: String,
    connection_id: Option<String>,
    domain_name: Option<String>,
    stage: Option<String>,
}

async fn handler(request: Request) -> Result<Response<Body>, Error> {
    let ctx = ws_ctx(&request);
    match dispatch(&request, &ctx).await {
        Ok(resp) => Ok(resp),
        Err(e) => Ok(error_response(e)),
    }
}

fn ws_ctx(request: &Request) -> WsCtx {
    if let Some(RequestContext::WebSocket(ws)) = request.request_context_ref() {
        return WsCtx {
            event_type: ws
                .event_type
                .clone()
                .unwrap_or_else(|| "MESSAGE".to_string())
                .to_uppercase(),
            connection_id: ws.connection_id.clone(),
            domain_name: ws.domain_name.clone(),
            stage: ws.stage.clone(),
        };
    }
    // Fallback: infer the route from the path (API Gateway `$connect`/`$disconnect`).
    let path = request.uri().path();
    let event_type = if path.contains("$connect") {
        "CONNECT"
    } else if path.contains("$disconnect") {
        "DISCONNECT"
    } else {
        "MESSAGE"
    };
    WsCtx {
        event_type: event_type.to_string(),
        connection_id: None,
        domain_name: None,
        stage: None,
    }
}

async fn dispatch(request: &Request, ctx: &WsCtx) -> Result<Response<Body>, ApiError> {
    match ctx.event_type.as_str() {
        "CONNECT" => handle_connect(request, ctx).await,
        "DISCONNECT" => handle_disconnect(ctx).await,
        _ => handle_message(request, ctx).await,
    }
}

async fn handle_connect(request: &Request, ctx: &WsCtx) -> Result<Response<Body>, ApiError> {
    let connection_id = ctx
        .connection_id
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("missing connectionId".to_string()))?;
    let query = request.uri().query();
    let token = query_param(query, "token")
        .ok_or_else(|| ApiError::BadRequest("missing token".to_string()))?;
    let trip_id_str = query_param(query, "tripId")
        .ok_or_else(|| ApiError::BadRequest("missing tripId".to_string()))?;
    let trip_id = uuid::Uuid::parse_str(&trip_id_str)
        .map_err(|_| ApiError::BadRequest("invalid tripId".to_string()))?;

    let region = secret_env("AWS_REGION");
    let claims = verify_jwt(&token, &secret_env("COGNITO_USER_POOL_ID"), &region).await?;
    let pool = connect(&secret_env("DB_SECRET_ID"), &region).await?;
    assert_trip_member(&pool, &claims.sub, trip_id).await?;

    let cfg = sdk_config(&region).await;
    let ddb = aws_sdk_dynamodb::Client::new(&cfg);
    ddb.put_item()
        .table_name(secret_env("WS_TABLE"))
        .item("connectionId", AttributeValue::S(connection_id.to_string()))
        .item("tripId", AttributeValue::S(trip_id.to_string()))
        .item("userId", AttributeValue::S(claims.sub))
        .item("ttl", AttributeValue::N((unix_now() + WS_TTL_SECONDS).to_string()))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("dynamodb put failed: {e}")))?;

    ok_response()
}

async fn handle_disconnect(ctx: &WsCtx) -> Result<Response<Body>, ApiError> {
    if let Some(id) = ctx.connection_id.as_deref() {
        let cfg = sdk_config(&secret_env("AWS_REGION")).await;
        let ddb = aws_sdk_dynamodb::Client::new(&cfg);
        ddb.delete_item()
            .table_name(secret_env("WS_TABLE"))
            .key("connectionId", AttributeValue::S(id.to_string()))
            .send()
            .await
            .map_err(|e| ApiError::Internal(format!("dynamodb delete failed: {e}")))?;
    }
    ok_response()
}

async fn handle_message(request: &Request, ctx: &WsCtx) -> Result<Response<Body>, ApiError> {
    let connection_id = ctx
        .connection_id
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("missing connectionId".to_string()))?;

    let bytes: Vec<u8> = match request.body() {
        Body::Text(s) => s.as_bytes().to_vec(),
        Body::Binary(b) => b.clone(),
        Body::Empty => return ok_response(),
    };
    let msg: ClientMessage = serde_json::from_slice(&bytes)?;
    if msg.action != "sendMessage" {
        return ok_response();
    }
    let text = msg.message.unwrap_or_default().trim().to_string();
    if text.is_empty() {
        return ok_response();
    }

    let region = secret_env("AWS_REGION");
    let ws_table = secret_env("WS_TABLE");
    let cfg = sdk_config(&region).await;
    let ddb = aws_sdk_dynamodb::Client::new(&cfg);

    // Resolve this connection's trip + user from the registry.
    let out = ddb
        .get_item()
        .table_name(&ws_table)
        .key("connectionId", AttributeValue::S(connection_id.to_string()))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("dynamodb get failed: {e}")))?;
    let item = out
        .item()
        .ok_or_else(|| ApiError::NotFound("connection not found".to_string()))?;
    let trip_id_str = item
        .get("tripId")
        .and_then(|a| a.as_s().ok())
        .cloned()
        .ok_or_else(|| ApiError::NotFound("connection missing tripId".to_string()))?;
    let user_id = item
        .get("userId")
        .and_then(|a| a.as_s().ok())
        .cloned()
        .unwrap_or_default();
    let trip_id = uuid::Uuid::parse_str(&trip_id_str)
        .map_err(|_| ApiError::Internal("bad tripId in registry".to_string()))?;

    // Resolve the sender's display name/avatar from their profile (fall back to sub).
    let pool = connect(&secret_env("DB_SECRET_ID"), &region).await?;
    let (sender_name, sender_avatar) = resolve_profile(&pool, &user_id).await;

    let row: ChatMessage = sqlx::query_as::<_, ChatMessage>(
        "INSERT INTO chat_messages (trip_id, sender_name, sender_avatar, message)
         VALUES ($1, $2, $3, $4)
         RETURNING id, trip_id, sender_name, sender_avatar, message, created_at",
    )
    .bind(trip_id)
    .bind(&sender_name)
    .bind(sender_avatar.as_deref())
    .bind(&text)
    .fetch_one(&pool)
    .await
    .map_err(ApiError::from)?;

    fan_out(&cfg, &ws_table, ctx, trip_id, &row).await?;

    ok_response()
}

async fn resolve_profile(pool: &sqlx::PgPool, user_id: &str) -> (String, Option<String>) {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT full_name, avatar_url FROM profiles WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    match row {
        Some((name, avatar)) if !name.is_empty() => (name, avatar),
        _ => (user_id.to_string(), None),
    }
}

async fn fan_out(
    cfg: &aws_config::SdkConfig,
    ws_table: &str,
    ctx: &WsCtx,
    trip_id: uuid::Uuid,
    row: &ChatMessage,
) -> Result<(), ApiError> {
    let ddb = aws_sdk_dynamodb::Client::new(cfg);
    let res = ddb
        .query()
        .table_name(ws_table)
        .index_name("tripId-index")
        .key_condition_expression("tripId = :trip")
        .expression_attribute_values(":trip", AttributeValue::S(trip_id.to_string()))
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("dynamodb query failed: {e}")))?;

    let payload = serde_json::to_vec(row).map_err(ApiError::from)?;
    let domain = ctx
        .domain_name
        .as_deref()
        .ok_or_else(|| ApiError::Internal("missing domainName".to_string()))?;
    let endpoint = match ctx.stage.as_deref() {
        Some(s) if !s.is_empty() => format!("https://{domain}/{s}"),
        _ => format!("https://{domain}"),
    };
    let mgmt_cfg = aws_sdk_apigatewaymanagement::config::Builder::from(cfg)
        .endpoint_url(endpoint)
        .build();
    let mgmt = aws_sdk_apigatewaymanagement::Client::from_conf(mgmt_cfg);

    for item in res.items() {
        let Some(cid) = item.get("connectionId").and_then(|a| a.as_s().ok()) else {
            continue;
        };
        let send = mgmt
            .post_to_connection()
            .connection_id(cid.clone())
            .data(aws_sdk_apigatewaymanagement::primitives::Blob::new(
                payload.clone(),
            ))
            .send()
            .await;
        if let Err(e) = send {
            // 410 = stale connection already gone; drop it silently.
            let gone = matches!(
                &e,
                aws_sdk_apigatewaymanagement::error::SdkError::ServiceError(se)
                    if se.raw().status().as_u16() == 410
            );
            if !gone {
                tracing::warn!(connection_id = %cid, error = %e, "post_to_connection failed");
            }
        }
    }
    Ok(())
}

async fn sdk_config(region: &str) -> aws_config::SdkConfig {
    aws_config::from_env()
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let mut it = pair.splitn(2, '=');
        let k = it.next()?;
        let v = it.next().unwrap_or("");
        (k == key).then(|| percent_decode(v))
    })
}

// ponytail: hand-rolled percent-decode; swap for the `percent-encoding` crate if
// query strings grow beyond token/tripId.
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
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn ok_response() -> Result<Response<Body>, ApiError> {
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"ok":true}"#))
        .map_err(|e| ApiError::Internal(e.to_string()))
}

fn error_response(e: ApiError) -> Response<Body> {
    Response::builder()
        .status(e.status_code())
        .header("content-type", "application/json")
        .body(Body::from(e.body()))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(500)
                .body(Body::from("{}"))
                .unwrap()
        })
}

fn main() -> Result<(), Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime")
        .block_on(run(service_fn(handler)))
}
