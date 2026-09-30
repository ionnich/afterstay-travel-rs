#!/usr/bin/env bash
set -euo pipefail

REGION=${AWS_REGION:-ap-southeast-1}
ACCOUNT=755251749545
TAG=$(git rev-parse --short HEAD 2>/dev/null || echo dev)
ECR=$ACCOUNT.dkr.ecr.$REGION.amazonaws.com
CRATES=(api integrations chat)

say() { printf '\033[1;32m[oracle]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[oracle]\033[0m %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<'EOF'
Usage: ./oracle.sh <command>

Commands:
  doctor   Check toolchain (cargo docker aws pulumi git jq), docker daemon, AWS creds
  build    Build Rust lambdas (arm64) + docker images for api/integrations/chat
  push     Build, then tag & push images to ECR (SHA + latest)
  deploy   Push, then pulumi up with current image tag
  test     cargo test (lambdas)
  clippy   cargo clippy --all-targets -- -D warnings
  fmt      cargo fmt
  e2e      Run end-to-end smoke checks (scripts/e2e.sh)
  ci       fmt + clippy + test + build + push
  preview  pulumi preview (infra)
  up       pulumi up --yes (infra)
  destroy  pulumi destroy --yes (infra)
  help     Show this help
EOF
}

cmd_doctor() {
  for c in cargo docker aws pulumi git jq; do
    command -v "$c" >/dev/null 2>&1 || fail "missing tool: $c"
  done
  say "cargo  $(cargo --version)"
  say "docker $(docker --version)"
  say "aws    $(aws --version)"
  say "pulumi $(pulumi version)"
  say "git    $(git --version)"
  say "jq     $(jq --version)"
  docker info >/dev/null 2>&1 || fail "docker daemon not running"
  aws sts get-caller-identity >/dev/null 2>&1 || fail "AWS credentials invalid"
  say "doctor: all checks passed"
}

cmd_build() {
  say "building lambdas (arm64)..."
  (cd lambdas && cargo lambda build --release --arm64)
  for crate in "${CRATES[@]}"; do
    say "building image afterstay/$crate:$TAG"
    docker build --build-arg CRATE="$crate" -t "afterstay/$crate:$TAG" lambdas/
  done
}

cmd_push() {
  cmd_build
  say "logging in to ECR ($ECR)..."
  aws ecr get-login-password --region "$REGION" \
    | docker login --username AWS --password-stdin "$ECR" >/dev/null
  for crate in "${CRATES[@]}"; do
    for t in "$TAG" latest; do
      docker tag "afterstay/$crate:$TAG" "$ECR/afterstay/$crate:$t"
      docker push "$ECR/afterstay/$crate:$t"
    done
  done
}

cmd_deploy() {
  cmd_push
  (cd infra \
    && pulumi config set aws:region "$REGION" \
    && pulumi config set imageTag "$TAG" \
    && pulumi up --yes)
}

cmd_ci() {
  (cd lambdas && cargo fmt)
  (cd lambdas && cargo clippy --all-targets -- -D warnings)
  (cd lambdas && cargo test)
  cmd_push
}

case "${1:-help}" in
  doctor)  cmd_doctor ;;
  build)   cmd_build ;;
  push)    cmd_push ;;
  deploy)  cmd_deploy ;;
  test)    (cd lambdas && cargo test) ;;
  clippy)  (cd lambdas && cargo clippy --all-targets -- -D warnings) ;;
  fmt)     (cd lambdas && cargo fmt) ;;
  e2e)     shift; bash scripts/e2e.sh "$@" ;;
  ci)      cmd_ci ;;
  preview) (cd infra && pulumi preview) ;;
  up)      (cd infra && pulumi up --yes) ;;
  destroy) (cd infra && pulumi destroy --yes) ;;
  help|-h|--help) usage ;;
  *)       usage >&2; exit 2 ;;
esac
