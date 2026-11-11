#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
selected=$(cargo test -p aster-server --test notebook_isolation postgres_workspace_ -- --ignored --list)
grep -q '^postgres_workspace_route_durable_workspace_survives_login_and_is_owner_only: test$' <<<"$selected"
name="aster-workspace-pg-$RANDOM-$$"
trap 'podman rm -f "$name" >/dev/null 2>&1 || true' EXIT
podman run -d --name "$name" -e POSTGRES_PASSWORD=disposable-test -p 127.0.0.1::5432 docker.io/library/postgres:17-alpine >/dev/null
for _ in {1..60}; do
  if podman exec "$name" pg_isready -U postgres >/dev/null 2>&1; then break; fi
  sleep 1
done
port=$(podman port "$name" 5432/tcp | cut -d: -f2)
export ASTER_TEST_METADATA_URL="postgres://postgres:disposable-test@127.0.0.1:$port/postgres"
failed=0
cargo test -p aster-server --test notebook_isolation postgres_workspace_ -- --ignored --nocapture || failed=1
cargo test -p aster-server --lib workspace::tests::postgres_workspace_ -- --ignored --nocapture || failed=1
cargo test -p aster-server --test notebook_isolation durable_workspace_creation_requires_a_new_session_independent_identity -- --exact || failed=1
cargo test -p aster-server --lib workspace_recovery::tests -- --nocapture || failed=1
cargo test -p aster-server --lib workspace_boot_tests::postgres_workspace_opt_in -- --ignored --exact --nocapture || failed=1
if [[ ${ASTER_TEST_WORKSPACE_GIT:-0} == 1 ]]; then
  python3 tests/notebook-git-https.py || failed=1
fi
if (( failed )); then exit 1; fi
echo 'workspace-postgres OK'
