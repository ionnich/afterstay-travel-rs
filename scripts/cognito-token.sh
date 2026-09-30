#!/usr/bin/env bash
set -euo pipefail

for var in COGNITO_USER_POOL_ID COGNITO_CLIENT_ID COGNITO_EMAIL COGNITO_PASSWORD; do
  if [ -z "${!var:-}" ]; then
    echo "Error: $var is not set" >&2
    exit 1
  fi
done

aws cognito-idp admin-initiate-auth \
  --user-pool-id "$COGNITO_USER_POOL_ID" \
  --client-id "$COGNITO_CLIENT_ID" \
  --auth-flow ADMIN_USER_PASSWORD_AUTH \
  --auth-parameters "USERNAME=$COGNITO_EMAIL,PASSWORD=$COGNITO_PASSWORD" \
  | jq -r '.AuthenticationResult.AccessToken'
