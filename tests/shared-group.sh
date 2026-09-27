#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

for spec in \
  'aster-core --lib state::tests::session_roundtrip_preserves_verified_groups_and_accepts_old_records' \
  'aster-core --lib auth::tests::shared_model_grants_match_verified_groups_or_role_hierarchy' \
  'aster-server --test ai_registration callback_keeps_trusted_groups_and_ignores_forged_request_claims' \
  'aster-server --lib state::tests::sessions_are_shared_between_connections'; do
  selected=$(cargo test -p $spec -- --list)
  grep -q ': test$' <<<"$selected"
done

cargo test -p aster-core --lib state::tests::session_roundtrip_preserves_verified_groups_and_accepts_old_records -- --exact
cargo test -p aster-core --lib auth::tests::shared_model_grants_match_verified_groups_or_role_hierarchy -- --exact
cargo test -p aster-server --test ai_registration callback_keeps_trusted_groups_and_ignores_forged_request_claims -- --exact

name="aster-groups-valkey-$RANDOM-$$"
cleanup() { podman rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
podman run -d --name "$name" -p 127.0.0.1::6379 docker.io/valkey/valkey:9.1.2-alpine >/dev/null
for _ in $(seq 1 30); do
  if podman exec "$name" valkey-cli ping 2>/dev/null | grep -qx PONG; then break; fi
  sleep 1
done
podman exec "$name" valkey-cli ping | grep -qx PONG
port=$(podman port "$name" 6379/tcp | cut -d: -f2)
ASTER_TEST_STATE_URL="redis://127.0.0.1:$port/" cargo test -p aster-server --lib state::tests::sessions_are_shared_between_connections -- --ignored --exact
echo 'shared-group-validation OK'
