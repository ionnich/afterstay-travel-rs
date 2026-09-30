# Operator Guide — AfterStay production readiness

Remaining manual actions, ordered by priority. Everything that could be done in
code/IaC is already done; these need console/dashboard access.

## Accounts involved

| Surface | Identity |
|---|---|
| GitHub | `ionnich` (`afterstay@afterstay.org`) — fork owner |
| GitHub upstream | `peterkgumapac-dotcom` (Peter Karl Gumapac, `peterkgumapac@gmail.com`) |
| AWS | account `755251749545`, profile `afterstay` (region `ap-southeast-1`) |
| Google Cloud | project `afterstay-travel` (owner must be confirmed in GCP console) |
| Supabase | legacy project (to be deleted) |
| Sentry | no project yet |

The GCP project's billing owner is not determinable from the repo — open
`console.cloud.google.com` and confirm which account owns it.

---

## 1. Enable GCP billing (blocks Maps + Places) ⚠️ highest priority

Every Maps Platform API (Places, Maps SDK, Geocoding, Static) currently returns
`REQUEST_DENIED — You must enable Billing`.

1. Go to `console.cloud.google.com/billing` (or project → Billing).
2. Attach/link a billing account to the `afterstay-travel` project.
3. Confirm Maps Platform APIs are enabled (APIs & Services → Enable APIs → "Maps SDK for Android", "Maps SDK for iOS", "Places API").
4. Re-probe from any shell:
   ```bash
   curl -s "https://maps.googleapis.com/maps/api/geocode/json?address=Boracay&key=AIzaSyAP7o2AvozqxFyoniXjKAgkDkS5sFOmnvc" | jq .status
   ```
   Expect `"OK"`, not `"REQUEST_DENIED"`.

The same key `AIzaSyAP7o2A…` is then reusable for **both** Places (server) and
Maps SDK (client tiles) — no new key needed.

---

## 2. Google OAuth (Cognito Google sign-in)

1. `console.cloud.google.com` → APIs & Services → Credentials → Create
   Credentials → **OAuth client ID** → type **Web application**.
2. Set the authorized redirect URI to the Cognito domain
   `https://<your-pool-domain>.auth.ap-southeast-1.amazoncognito.com/oauth2/idpresponse`.
3. Record the **Client ID** (`*.apps.googleusercontent.com`) and **Client Secret**
   (`GOCSPX-…`).
4. Wire into infra + client:
   ```bash
   # backend (afterstay-travel-rs), then `pulumi up`
   pulumi config set googleClientId "<client-id>"   --stack afterstay
   pulumi config set googleClientSecret "<secret>"  --stack afterstay
   # client .env
   EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID=<client-id>
   EXPO_PUBLIC_COGNITO_OAUTH_DOMAIN=<your-pool-domain>.auth.ap-southeast-1.amazoncognito.com
   ```

---

## 3. Maps SDK key (client native tiles)

After step 1: set the same key in the client `.env` and restrict it by bundle id.

```bash
# client .env
EXPO_PUBLIC_GOOGLE_MAPS_SDK_KEY=AIzaSyAP7o2AvozqxFyoniXjKAgkDkS5sFOmnvc
```

In GCP → Credentials → edit the key → **Application restrictions → Android/iOS
apps** → add package `com.afterstay.travel` / bundle id `com.afterstay.travel`.

---

## 4. Sentry (client error tracking)

CloudWatch already captures **backend** logs (see §7) — Sentry is for **client
(mobile) crashes + source maps**.

1. `sentry.io` → Create project → React Native.
2. Copy the DSN, set in client `.env`: `EXPO_PUBLIC_SENTRY_DSN=https://…@….ingest.sentry.io/…`.
3. (Optional) EAS source-map upload: add `SENTRY_AUTH_TOKEN`, `SENTRY_ORG`,
   `SENTRY_PROJECT` to EAS env + a `postPublish`/`postBuild` hook (see `eas.json`
   comment). Not required for basic error capture.

If you'd rather **skip Sentry entirely**, see §7 for the CloudWatch-only
alternative.

---

## 5. Decommission Supabase

1. `supabase.com` → project → Settings → **Pause** (or **Delete**).
2. Keep a DB backup for one week as rollback.
3. Rotate the leaked `sbp_…` token (it was in the old client bundle).

---

## 6. Rotate exposed keys

These three were committed to git history (`c11e500:expo.env`). History is now
cleaned, but the values are still live — rotate them and update Secrets Manager:

| Key | Rotate at | Update |
|---|---|---|
| Anthropic `sk-ant-api03-…` | `console.anthropic.com` → API keys | `aws secretsmanager put-secret-value --secret-id afterstay/anthropic --secret-string '<new>'` |
| Google Places `AIzaSyAP7o2A…` | GCP → Credentials → regenerate | `--secret-id afterstay/google-places` |
| Weather `f3cc84a7cc86…` | weatherapi.com → API key | `--secret-id afterstay/weather` |

(Deferred by operator — do before GA.)

---

## 7. Logging: CloudWatch vs Sentry

**Backend is already fully logged to CloudWatch** — no action needed. Every
Lambda writes to `/aws/lambda/afterstay-<crate>`:

```bash
aws logs tail /aws/lambda/afterstay-api --follow
aws logs tail /aws/lambda/afterstay-integrations --follow
```

**Client (mobile) is the gap.** CloudWatch can't ingest device errors directly.
Two options:

- **Sentry** (recommended): purpose-built RN SDK, breadcrumbs, releases, source
  maps. Set the DSN in §4.
- **CloudWatch-only**: add a `POST /v1/integrations/log` route (Lambda writes the
  body to CloudWatch Logs), then `lib/api.ts` reports JS/native errors there.
  Loses source-map upload + native-crash symbolication. Only worth it to avoid a
  Sentry account.

---

## 8. Smoke test the client

```bash
cd afterstay-travel
# ensure .env has API_URL, COGNITO_USER_POOL_ID, COGNITO_CLIENT_ID (required)
npm run ios   # or: npm run android
```

Check: email/password sign-in → trip loads → add expense → add moment (presign →
S3) → scan receipt (Anthropic) → Discover (Places, needs §1) → Home weather card
→ group chat (WebSocket).
