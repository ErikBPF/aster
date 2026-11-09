#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
name="aster-personal-model-pg-$RANDOM-$$"
cleanup() { podman rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
podman run -d --name "$name" -e POSTGRES_PASSWORD=disposable-test -p 127.0.0.1::5432 docker.io/library/postgres:17-alpine >/dev/null
ready=0
for _ in $(seq 1 60); do
  if podman exec "$name" createdb -U postgres personal_models_test >/dev/null 2>&1; then ready=1; break; fi
  sleep 1
done
test "$ready" -eq 1
port=$(podman port "$name" 5432/tcp | cut -d: -f2)
export ASTER_TEST_PERSONAL_MODEL_URL="postgres://postgres:disposable-test@127.0.0.1:$port/personal_models_test"
cargo test -p aster-server --test ai_registration personal_team_capability_does_not_override_git_policy_denial -- --ignored --nocapture
cargo test -p aster-server --lib postgres_personal_tokens_migrate_atomically_and_restart_without_plaintext -- --ignored --nocapture
export ASTER_TEST_SKILLS_URL="$ASTER_TEST_PERSONAL_MODEL_URL"
# These tests share a fresh schema; serialize initialization, not their concurrent operations.
cargo test -p aster-server --lib skills_pg::tests -- --ignored --nocapture --test-threads=1
cargo test -p aster-server --lib mcp_pg::tests -- --ignored --nocapture
echo 'AI_REGISTRY_POSTGRES_OK'
