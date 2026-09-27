#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
filter=postgres_team_target_
cargo_env=(env PATH="$PWD/.devenv/profile/bin:$PATH" RUSTC_WRAPPER= CARGO_TARGET_DIR=/home/developer/projects/aster/target PROTOC="$PWD/.devenv/profile/bin/protoc")
selected=$("${cargo_env[@]}" cargo test -p aster-server --lib "$filter" -- --ignored --list)
grep -q '^store::team_target_tests::postgres_team_target_schema_bootstraps_on_connect: test$' <<<"$selected"
grep -q '^store::team_target_tests::postgres_team_target_cas_and_audit_survive_reconnect: test$' <<<"$selected"
grep -q '^store::team_target_tests::postgres_team_target_concurrent_updates_choose_one_version: test$' <<<"$selected"
grep -q '^team_git::tests::postgres_team_target_verified_maintainer_configures_pending_target: test$' <<<"$selected"
boot_selected=$("${cargo_env[@]}" cargo test -p aster-server --lib team_only_boot_needs_no_shared_model_registry_or_role_groups -- --ignored --list)
grep -q '^team_git_boot_tests::team_only_boot_needs_no_shared_model_registry_or_role_groups: test$' <<<"$boot_selected"
route_selected=$("${cargo_env[@]}" cargo test -p aster-server --test notebook_isolation postgres_team_target_route_ -- --ignored --list)
grep -q '^postgres_team_target_route_uses_fresh_uuid_groups_and_never_activates_workspace: test$' <<<"$route_selected"
name="aster-team-target-pg-$RANDOM-$$"
cleanup() { podman rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
podman run -d --name "$name" -e POSTGRES_PASSWORD=disposable-test -p 127.0.0.1::5432 docker.io/library/postgres:17-alpine >/dev/null
for _ in $(seq 1 60); do
  if podman exec "$name" pg_isready -U postgres >/dev/null 2>&1; then break; fi
  sleep 1
done
port=$(podman port "$name" 5432/tcp | cut -d: -f2)
export ASTER_TEST_METADATA_URL="postgres://postgres:disposable-test@127.0.0.1:$port/postgres"
"${cargo_env[@]}" cargo test -p aster-server --lib "$filter" -- --ignored --nocapture
"${cargo_env[@]}" cargo test -p aster-server --lib team_only_boot_needs_no_shared_model_registry_or_role_groups -- --ignored --nocapture
"${cargo_env[@]}" cargo test -p aster-server --test notebook_isolation postgres_team_target_route_ -- --ignored --nocapture
echo "notebook-team-target-postgres OK"
