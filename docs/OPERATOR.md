# Operator Guide — AfterStay production readiness

Remaining manual actions, ordered by priority. Everything that could be done in
code/IaC is already done; these need console/dashboard access.

## Accounts involved

| Surface | Identity |
|---|---|
| GitHub | `ionnich` (commits attributed to `ionnich`, author `aarn.gmpc@gmail.com`) — fork owner |
| GitHub upstream | `peterkgumapac-dotcom` (Peter Karl Gumapac, `peterkgumapac@gmail.com`) |
| AWS | account `755251749545`, profile `afterstay` (region `ap-southeast-1`) |
| Google Cloud | billing account `011C65-BBC99B-0663B6` ("Afterstay", org `afterstay.org`); Maps/Places key `AIzaSyAP7o2A…` lives on project `core-outrider-493623-i2` ("My First Project"); Firebase/identity project is `afterstay-travel` (704335704962) |
| Supabase | legacy project (to be deleted) |
| Sentry | no project yet |

The GCP project's billing owner is not determinable from the repo — open
`console.cloud.google.com` and confirm which account owns it.

---

## 1. Enable GCP billing (blocks Maps + Places) ✅ DONE

Billing account `011C65-BBC99B-0663B6` ("Afterstay") is linked to the key's
project **`core-outrider-493623-i2`** ("My First Project" — rename it to
`afterstay-maps` for clarity), and the 4 Maps APIs are enabled. Geocoding +
Places + Static Maps all return `OK` with key `AIzaSyAP7o2A…`.

---

## 2. Google OAuth (Cognito Google sign-in)

Cognito domain `afterstay-users` + OAuth callback/logout URLs are already wired
in `infra/cognito.ts` (deployed on next `pulumi up`). Remaining: create the GCP
OAuth client and provide id + secret.

1. `console.cloud.google.com` → project `afterstay-travel`.
2. **APIs & Services → OAuth consent screen**: type External; app name
   "AfterStay"; support email `afterstay@afterstay.org`; authorized domains
   `afterstay.org`; scopes `openid email profile`; add yourself as a test user.
3. **Credentials → Create Credentials → OAuth client ID** → **Web application**.
4. Authorized redirect URI (exact):
   `https://afterstay-users.auth.ap-southeast-1.amazoncognito.com/oauth2/idpresponse`
5. Copy **Client ID** (`*.apps.googleusercontent.com`) and **Client Secret**
   (`GOCSPX-…`) → give to the agent.
6. Agent runs:
   ```bash
   pulumi config set googleClientId "<client-id>" --stack afterstay
   pulumi config set googleClientSecret "<secret>" --secret --stack afterstay
   pulumi up --yes
   # client .env
   EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID=<client-id>
   EXPO_PUBLIC_COGNITO_OAUTH_DOMAIN=afterstay-users.auth.ap-southeast-1.amazoncognito.com
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
