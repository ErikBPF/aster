#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

name="aster-shared-model-pg-$RANDOM-$$"
cleanup() { podman rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT

podman run -d --name "$name" -e POSTGRES_PASSWORD=disposable-test \
  -p 127.0.0.1::5432 docker.io/library/postgres:17-alpine >/dev/null
ready=0
for _ in $(seq 1 60); do
  if podman exec "$name" createdb -U postgres shared_models_test >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done
test "$ready" -eq 1
port=$(podman port "$name" 5432/tcp | cut -d: -f2)
export ASTER_TEST_SHARED_MODEL_URL="postgres://postgres:disposable-test@127.0.0.1:$port/shared_models_test"
cargo test -p aster-server --lib postgres_tokens_are_encrypted_restart_safe_and_rotation_is_atomic -- --ignored --list | grep -q ': test$'
cargo test -p aster-server --lib postgres_tokens_are_encrypted_restart_safe_and_rotation_is_atomic -- --ignored --nocapture
