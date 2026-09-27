#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
selected=$(PATH="$PWD/.devenv/profile/bin:$PATH" RUSTC_WRAPPER= CARGO_TARGET_DIR=/home/developer/projects/aster/target PROTOC="$PWD/.devenv/profile/bin/protoc" cargo test -p aster-server --lib postgres_notebook_owner_cas_and_audit_survive_reconnect -- --ignored --list)
grep -q '^store::notebook_owner_tests::postgres_notebook_owner_cas_and_audit_survive_reconnect: test$' <<<"$selected"
name="aster-owner-pg-$RANDOM-$$"
cleanup() { podman rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
podman run -d --name "$name" -e POSTGRES_PASSWORD=disposable-test -p 127.0.0.1::5432 docker.io/library/postgres:17-alpine >/dev/null
for _ in $(seq 1 60); do
  if podman exec "$name" pg_isready -U postgres >/dev/null 2>&1; then break; fi
  sleep 1
done
port=$(podman port "$name" 5432/tcp | cut -d: -f2)
export ASTER_TEST_METADATA_URL="postgres://postgres:disposable-test@127.0.0.1:$port/postgres"
PATH="$PWD/.devenv/profile/bin:$PATH" RUSTC_WRAPPER= CARGO_TARGET_DIR=/home/developer/projects/aster/target PROTOC="$PWD/.devenv/profile/bin/protoc" cargo test -p aster-server --lib postgres_notebook_owner_cas_and_audit_survive_reconnect -- --ignored --nocapture
