//! `api` Lambda — data CRUD behind `ANY /v1/data/{proxy+}`.
//!
//! Startup: connect to RDS (secret from Secrets Manager), run migrations, then
//! serve via `lambda_http::run`. Every route (except `/health`) verifies the
//! Cognito JWT and, for trip-scoped routes, asserts trip membership.

mod handlers;

use lambda_http::{run, service_fn, Body, Error, Request};
use lambda_http::http::{Method, Response};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use core::error::ApiError;

/// Shared per-invocation state (cloned cheaply into each request handler).
#[derive(Clone)]
pub struct State {
    pub pool: PgPool,
    pub pool_id: String,
    pub region: String,
    pub media_bucket: String,
    pub s3: aws_sdk_s3::Client,
}

/// Per-request context: verified caller identity + shared state.
pub struct Ctx<'a> {
    pub state: &'a State,
    pub sub: &'a str,
    pub email: Option<&'a str>,
}

// ---------- response helpers ----------

pub fn json(status: u16, value: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .expect("valid json response")
}

pub fn ok(value: serde_json::Value) -> Response<Body> {
    json(200, value)
}

pub fn ok_body() -> Response<Body> {
    ok(json!({ "ok": true }))
}

pub fn err_response(e: ApiError) -> Response<Body> {
    let status = e.status_code();
    let body = e.body();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("valid error response")
}

pub fn parse_uuid(s: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(s).map_err(|_| ApiError::BadRequest(format!("invalid id: {s}")))
}

// ---------- router ----------

async fn dispatch(state: State, req: Request) -> Result<Response<Body>, ApiError> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let segs: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    // Health check — no auth.
    if method == Method::GET && segs == ["v1", "data", "health"] {
        return Ok(ok(json!({ "ok": true })));
    }

    // Everything else must be under /v1/data.
    if segs.len() < 2 || segs[0] != "v1" || segs[1] != "data" {
        return Err(ApiError::NotFound("route not found".to_string()));
    }
    let r = &segs[2..];

    // Verify JWT.
    let auth = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let claims = core::auth::verify_jwt(auth, &state.pool_id, &state.region).await?;
    let ctx = Ctx {
        state: &state,
        sub: &claims.sub,
        email: claims.email.as_deref(),
    };

    match (method.as_str(), r) {
        ("GET", ["trips", "active"]) => handlers::get_active_trip(&ctx).await,
        ("GET", ["trips", "past"]) => handlers::get_past_trips(&ctx).await,
        ("POST", ["trips"]) => handlers::create_trip(&ctx, &req).await,
        ("POST", ["invites"]) => handlers::create_invite_code(&ctx, &req).await,
        ("POST", ["invites", "join"]) => handlers::join_trip_by_code(&ctx, &req).await,
        ("GET", ["invites"]) => handlers::get_invites(&ctx, &req).await,
        ("GET", ["trips", id, "flights"]) => handlers::get_flights(&ctx, parse_uuid(*id)?).await,
        ("POST", ["trips", id, "flights"]) => handlers::add_flight(&ctx, parse_uuid(*id)?, &req).await,
        ("GET", ["trips", id, "members"]) => handlers::get_group_members(&ctx, parse_uuid(*id)?).await,
        ("POST", ["trips", id, "members"]) => handlers::add_group_member(&ctx, parse_uuid(*id)?, &req).await,
        ("POST", ["members", id, "photo"]) => handlers::update_member_photo(&ctx, parse_uuid(*id)?, &req).await,
        ("PATCH", ["members", id, "email"]) => handlers::update_member_email(&ctx, parse_uuid(*id)?, &req).await,
        ("PATCH", ["members", id, "phone"]) => handlers::update_member_phone(&ctx, parse_uuid(*id)?, &req).await,
        ("GET", ["trips", id, "chat"]) => handlers::get_chat_messages(&ctx, parse_uuid(*id)?).await,
        ("POST", ["trips", id, "chat"]) => handlers::send_chat_message(&ctx, parse_uuid(*id)?, &req).await,
        ("GET", ["trips", id, "packing"]) => handlers::get_packing_list(&ctx, parse_uuid(*id)?).await,
        ("POST", ["trips", id, "packing"]) => handlers::add_packing_item(&ctx, parse_uuid(*id)?, &req).await,
        ("POST", ["packing", id, "toggle"]) => handlers::toggle_packed(&ctx, parse_uuid(*id)?, &req).await,
        ("GET", ["trips", id, "expenses"]) => handlers::get_expenses(&ctx, parse_uuid(*id)?).await,
        ("POST", ["trips", id, "expenses"]) => handlers::add_expense(&ctx, parse_uuid(*id)?, &req).await,
        ("GET", ["trips", id, "expenses", "summary"]) => {
            handlers::get_expense_summary(&ctx, parse_uuid(*id)?).await
        }
        ("PATCH", ["expenses", id]) => handlers::update_expense(&ctx, parse_uuid(*id)?, &req).await,
        ("DELETE", ["expenses", id]) => handlers::delete_expense(&ctx, parse_uuid(*id)?).await,
        ("GET", ["trips", id, "places"]) => handlers::get_saved_places(&ctx, parse_uuid(*id)?).await,
        ("POST", ["trips", id, "places"]) => handlers::add_place(&ctx, parse_uuid(*id)?, &req).await,
        ("POST", ["places", id, "vote"]) => handlers::vote_on_place(&ctx, parse_uuid(*id)?, &req).await,
        ("POST", ["places", id, "save"]) => handlers::save_place(&ctx, parse_uuid(*id)?, &req).await,
        ("GET", ["trips", id, "checklist"]) => handlers::get_checklist(&ctx, parse_uuid(*id)?).await,
        ("GET", ["trips", id, "moments"]) => handlers::get_moments(&ctx, parse_uuid(*id)?).await,
        ("POST", ["trips", id, "moments"]) => handlers::add_moment(&ctx, parse_uuid(*id)?, &req).await,
        ("GET", ["trips", id, "files"]) => handlers::get_trip_files(&ctx, parse_uuid(*id)?).await,
        ("POST", ["trips", id, "files"]) => handlers::add_trip_file(&ctx, parse_uuid(*id)?, &req).await,
        ("PATCH", ["trips", id, "property"]) => {
            handlers::update_trip_property(&ctx, parse_uuid(*id)?, &req).await
        }
        ("PATCH", ["trips", id, "budget-mode"]) => {
            handlers::update_trip_budget_mode(&ctx, parse_uuid(*id)?, &req).await
        }
        ("PATCH", ["trips", id, "budget-limit"]) => {
            handlers::update_trip_budget_limit(&ctx, parse_uuid(*id)?, &req).await
        }
        ("GET", ["profile", uid]) => handlers::get_profile(&ctx, *uid).await,
        ("PATCH", ["profile", uid]) => handlers::update_profile(&ctx, *uid, &req).await,
        ("POST", ["profile", "ensure"]) => handlers::ensure_profile(&ctx, &req).await,
        ("GET", ["stats", "lifetime"]) => handlers::get_lifetime_stats(&ctx).await,
        ("GET", ["stats", "highlights"]) => handlers::get_highlights(&ctx).await,
        ("POST", ["presign"]) => handlers::presign(&ctx, &req).await,
        _ => Err(ApiError::NotFound("route not found".to_string())),
    }
}

async fn handle(state: State, req: Request) -> Result<Response<Body>, Error> {
    Ok(dispatch(state, req).await.unwrap_or_else(err_response))
}

fn main() -> Result<(), Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime")
        .block_on(async {
            let db_secret_id = core::db::secret_env("DB_SECRET_ID");
            let region = core::db::secret_env("AWS_REGION");
            let pool_id = core::db::secret_env("COGNITO_USER_POOL_ID");
            let media_bucket = core::db::secret_env("MEDIA_BUCKET");

            let pool = core::db::connect(&db_secret_id, &region)
                .await
                .expect("failed to connect to database");
            sqlx::migrate!("../migrations")
                .run(&pool)
                .await
                .expect("failed to run migrations");

            let s3_config = aws_config::from_env()
                .region(aws_config::Region::new(region.clone()))
                .load()
                .await;
            let s3 = aws_sdk_s3::Client::new(&s3_config);

            let state = State {
                pool,
                pool_id,
                region,
                media_bucket,
                s3,
            };

            run(service_fn(move |req| handle(state.clone(), req))).await
        })
}
