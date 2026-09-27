#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
name="aster-chat-pg-$RANDOM-$$"
cleanup() { podman rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
podman run -d --name "$name" -e POSTGRES_PASSWORD=disposable-test -p 127.0.0.1::5432 docker.io/library/postgres:17-alpine >/dev/null
for _ in $(seq 1 60); do
  if podman exec "$name" pg_isready -U postgres >/dev/null 2>&1; then break; fi
  sleep 1
done
podman exec "$name" createdb -U postgres metadata
podman exec "$name" createdb -U postgres conversations
port=$(podman port "$name" 5432/tcp | cut -d: -f2)
export ASTER_TEST_METADATA_URL="postgres://postgres:disposable-test@127.0.0.1:$port/metadata"
export ASTER_TEST_CONVERSATION_URL="postgres://postgres:disposable-test@127.0.0.1:$port/conversations"
cargo test -p aster-server --lib postgres_delegation_persists_and_serializes_exchanges -- --ignored --nocapture
