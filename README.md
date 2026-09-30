# afterstay — backend & infra

AWS backend for the afterstay travel app. Replaces Supabase: Rust Lambdas
(`api`, `integrations`, `chat`) shipped as ECR container images, provisioned
with Pulumi in `infra/`, secrets in AWS Secrets Manager.

## Layout

```
infra/          Pulumi (TypeScript) — VPC, RDS, S3, Cognito, DynamoDB, ECR, Lambdas, API GW
  db.ts         RDS Postgres + Secrets Manager secrets (afterstay/db, /anthropic, /google-places, /weather)
  lambda.ts     Lambda functions + HTTP API (/v1/data, /v1/integrations) + WebSocket API
  cognito.ts    Cognito user pool + Google identity provider
lambdas/        Rust workspace (cargo): core / api / integrations / chat
  core/         shared: JWT verify, trip-membership authz, DB pool, types
  api/          data CRUD behind ANY /v1/data/{proxy+} (42 routes)
  integrations/ Anthropic + Google Places + Weather behind ANY /v1/integrations/{proxy+}
  chat/         WebSocket $connect/$disconnect/$default (fan-out per trip)
  migrations/   0001_init.sql — applied at api cold start via sqlx::migrate!
scripts/        e2e.sh, cognito-token.sh
oracle.sh       single entry point for build/push/deploy/test/ci
Makefile        thin wrappers around oracle.sh
```

## Prerequisites

Docker, Rust toolchain (`cargo`), Node, Pulumi, AWS CLI, and the `afterstay`
AWS profile (account `755251749545`, region `ap-southeast-1`).

Secrets/config live in `.env.local` (gitignored): AWS creds,
`PULUMI_CONFIG_PASSPHRASE`, `PULUMI_BACKEND_URL=s3://afterstay-pulumi-state`.

## Workflow

```bash
source .env.local
./oracle.sh doctor   # toolchain + creds check
./oracle.sh build    # cargo build (arm64) + docker build
./oracle.sh push     # build + push images to ECR (git SHA + latest)
./oracle.sh deploy   # push + pulumi up with the SHA image tag
./oracle.sh e2e      # smoke-check the deployed API
./oracle.sh ci       # fmt --check + clippy -D warnings + test + push (+ client tsc)
```

`make build|push|deploy|test|ci|preview|up` are thin aliases. Deploys are
image-reference-only (`pulumi config set imageTag <sha>` then `pulumi up`),
so they are fast and never rebuild.

## API routes

`api` Lambda (`ANY /v1/data/{proxy+}`) — data CRUD, camelCase JSON, `{ error:
{ code, message } }` on failure. Route groups:

- **trips** — `GET /trips/active`, `GET /trips/past`, `POST /trips`,
  `PATCH /trips/:id/property|budget-mode|budget-limit`
- **invites** — `POST /invites`, `GET /invites`, `POST /invites/join`
- **flights / members** — `GET|POST /trips/:id/flights`, `GET|POST /trips/:id/members`,
  `POST /members/:id/photo`, `PATCH /members/:id/email|phone`
- **chat / packing / expenses / places / checklist / moments / files** — full
  CRUD under `GET|POST /trips/:id/...` with `PATCH|DELETE /:resource/:id` updates
- **profile** — `GET|PATCH /profile/:uid`
- **presign** — `POST /presign` → `{ uploadUrl, key }`

`integrations` Lambda (`ANY /v1/integrations/{proxy+}`):

- `POST /anthropic/recommendations|itinerary|receipt|scan-trip`
- `GET /places/nearby|autocomplete|details|location`, `POST /places/enrich`
- `GET /weather?location=…`

`chat` Lambda — WebSocket `$connect` / `$disconnect` / `$default`, fan-out per
`tripId` via the DynamoDB connection registry.

## Secrets

`afterstay/db` (auto-generated master password), `afterstay/anthropic`,
`afterstay/google-places`, `afterstay/weather` — all in Secrets Manager,
read at Lambda cold start. `afterstay/db` holds a JSON object
`{username,password,host,port,dbname}`; the other three hold bare API-key
strings (or `{"api_key": ...}`).

## CI

`.github/workflows/ci.yml` is **pull-only**: GitHub runners `docker pull` and
`docker image inspect` the pre-built ECR images. All real work (build, push,
clippy, test, tsc) runs locally via `./oracle.sh ci`. The workflow assumes an
OIDC role `arn:aws:iam::755251749545:role/<github-actions-role>` (ECR
read-only); see the comments in the YAML for the secrets-based fallback.

## Deployed endpoints

From `pulumi stack output` (`apiUrl`, `wsUrl`, `userPoolId`,
`userPoolClientId`). Current live values (region `ap-southeast-1`):

- REST: `https://tnrpwsjhad.execute-api.ap-southeast-1.amazonaws.com/`
- WS: `wss://hlsw9zjpti.execute-api.ap-southeast-1.amazonaws.com/prod`
- Cognito pool `ap-southeast-1_M1u6gpCDP`, client `1eqq2n2cbmbqfpm2n3g8slvchn`

These feed the client's `EXPO_PUBLIC_API_URL`, `EXPO_PUBLIC_WS_URL`,
`EXPO_PUBLIC_COGNITO_USER_POOL_ID`, and `EXPO_PUBLIC_COGNITO_CLIENT_ID`
(see `afterstay-travel`). Re-run `pulumi stack output` if they change on
redeploy.
