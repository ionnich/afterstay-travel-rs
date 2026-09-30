# AGENTS.md — afterstay backend & infra

Standing instructions for agents working in this repo. Companion to the client
repo `afterstay-travel` (Expo/RN app) — keep the two in sync.

## Stack (do not swap)

- **Pulumi** (TypeScript) in `infra/` — all AWS resources as code.
- **Rust** Lambdas (Cargo workspace `lambdas/`): `core` (shared lib), `api`,
  `integrations`, `chat`. Shipped as **ECR container images**
  (`public.ecr.aws/lambda/provided:al2023` + `bootstrap`), arm64.
- **Postgres** (RDS), **Cognito** (auth), **S3** (media), **DynamoDB**
  (WebSocket connections), **Secrets Manager** (keys), **API Gateway** (HTTP + WebSocket).
- Region `ap-southeast-1`, account `755251749545`, AWS profile `afterstay`.

## Golden rules

1. **Never build on CI.** All builds/pushes/tests run locally via
   `./oracle.sh ci`. GitHub runners are pull-only.
2. **Secrets live in Secrets Manager**, never in env vars, code, or git:
   `afterstay/db` (JSON), `afterstay/anthropic`, `afterstay/google-places`,
   `afterstay/weather` (bare key or `{"api_key": ...}`). Read at Lambda cold start.
3. **Every per-trip table routes through `assert_trip_member`** (single
   `SELECT 1` from `trip_members`, 403 on miss) after `verify_jwt`. Never trust
   a client-supplied `user_id`; derive identity from the Cognito `sub` claim.
4. **Client contract is frozen.** `api` Lambda routes mirror the function
   signatures the client's `lib/api.ts` calls 1:1 (camelCase JSON, `{ error:
   { code, message } }` envelopes). Changing a route means changing the client
   in the same change.
5. **Migrations** live in `lambdas/migrations/` and run at `api` cold start via
   `sqlx::migrate!("../migrations")`. Schema mirrors the client `lib/types.ts`;
   columns snake_case, table names plural.
6. **Money**: `numeric`/`BigDecimal` is serialized as `f64` for the client
   (BigDecimal's default serde emits strings — the client expects numbers).
7. **Timezones**: PHT (UTC+8) is canonical; date-only strings must be parsed
   with the +08:00 offset, never `new Date(iso)` equivalent.

## Commands

```bash
source .env.local          # required before any aws/pulumi/docker command
./oracle.sh doctor|build|push|deploy|test|clippy|fmt|e2e|ci|preview|up|destroy
make build|push|deploy|ci  # aliases
```

`deploy` = push images (SHA tag) + `pulumi config set imageTag <sha>` +
`pulumi up`. A redeploy with new code requires a commit (new SHA).

## Conventions

- **Rust**: hand-rolled `(method, path)` match in each Lambda's router — no web
  framework. `lambda_http` + `lambda_runtime`, `sqlx` (Postgres, pool), `reqwest`,
  `jsonwebtoken`, AWS SDK (`s3` presign, `dynamodb`, `secretsmanager`).
- **Env workarounds in place** (do not "fix"): manual `tokio::Runtime::Builder`
  instead of `#[tokio::main]`; chrono `.format()` instead of `.month0()`/etc.;
  Docker `--provenance=false` + `CMD ["bootstrap"]`; `AWS_REGION` is read from
  env, not set on the Lambda.
- **Presigned uploads**: `POST /v1/data/presign` returns `{ uploadUrl, key }`;
  clients PUT to S3, then POST the row with the key. Photos/files never stream
  through a Lambda.
- **Lazy over clever**: small, boring diffs. A deliberate simplification with a
  known ceiling gets a `ponytail:` comment naming the ceiling and upgrade path.

## What NOT to do

- Don't put secrets in `infra/` or `lambdas/` source; don't commit `.env.local`.
- Don't add a web framework, ORM, or build step to the Lambdas.
- Don't regenerate the RDS master password (it's a Pulumi `random_password` in
  state — re-creating it desyncs from the live instance).
- Don't enable `deprecated`/`dead_code` lints as errors; they're allow-listed in
  `lambdas/Cargo.toml` so `-D warnings` stays meaningful.
- Don't add data access to the client screens — all data flows through the
  `api` Lambda routes the client `lib/api.ts` already calls.
