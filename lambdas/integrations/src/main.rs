use lambda_http::{run, service_fn, Body, Error, Request};
use lambda_http::http::Response;

async fn handler(_: Request) -> Result<Response<Body>, Error> {
    Ok(Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"ok":true}"#))?)
}

fn main() -> Result<(), Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build runtime")
        .block_on(run(service_fn(handler)))
}
