#!/usr/bin/env bash
set -euo pipefail

API_URL=${API_URL:-$(cd "$(dirname "$0")/.." && cd infra && pulumi stack output apiUrl 2>/dev/null | tr -d '"')}

if [ -z "$API_URL" ]; then
  echo "[e2e] API not deployed yet — skipping"
  exit 0
fi

pass() { echo "PASS: $*"; }
fail() { echo "FAIL: $*"; }

rc=0

# 1. health
if body=$(curl -sf "$API_URL/v1/data/health") && echo "$body" | jq -e '.ok == true' >/dev/null 2>&1; then
  pass "health returns ok:true"
else
  fail "health check (got: ${body:-<no body>})"
  rc=1
fi

# 2. unauthenticated trips/active -> 401
code=$(curl -s -o /dev/null -w '%{http_code}' "$API_URL/v1/data/trips/active")
if [ "$code" = "401" ]; then
  pass "trips/active unauthenticated -> 401"
else
  fail "trips/active unauthenticated -> expected 401, got $code"
  rc=1
fi

# 3. authenticated trips/active (if token provided) -> 200 or 404
if [ -n "${E2E_JWT:-}" ]; then
  code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $E2E_JWT" "$API_URL/v1/data/trips/active")
  if [ "$code" = "200" ] || [ "$code" = "404" ]; then
    pass "trips/active authenticated -> $code"
  else
    fail "trips/active authenticated -> expected 200/404, got $code"
    rc=1
  fi
else
  echo "[e2e] E2E_JWT not set — skipping authenticated check"
fi

exit $rc
