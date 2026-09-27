#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

selected=$(cargo test -p aster-core --lib notebook_helpers_are_private_and_independent_of_last_opened_state -- --list)
grep -q ': test$' <<<"$selected"
selected=$(cargo test -p aster-server --test ai_registration helper_choice_ -- --list)
grep -q ': test$' <<<"$selected"
selected=$(cargo test -p aster-server --test ai_registration legacy_choice_migrates_once_without_changing_other_notebooks -- --list)
grep -q ': test$' <<<"$selected"
selected=$(cargo test -p aster-server --lib state::tests::notebook_helpers_are_isolated_atomic_and_outlive_working_state -- --list)
grep -q ': test$' <<<"$selected"

cargo test -p aster-core --lib notebook_helpers_are_private_and_independent_of_last_opened_state
cargo test -p aster-server --test ai_registration helper_choice_
cargo test -p aster-server --test ai_registration legacy_choice_migrates_once_without_changing_other_notebooks

name="aster-helper-valkey-$RANDOM-$$"
cleanup() { podman rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
podman run -d --name "$name" -p 127.0.0.1::6379 docker.io/valkey/valkey:9.1.2-alpine >/dev/null
for _ in $(seq 1 30); do
    if podman exec "$name" valkey-cli ping 2>/dev/null | grep -qx PONG; then break; fi
    sleep 1
done
podman exec "$name" valkey-cli ping | grep -qx PONG
port=$(podman port "$name" 6379/tcp | cut -d: -f2)
export ASTER_TEST_STATE_URL="redis://127.0.0.1:$port/"
cargo test -p aster-server --lib state::tests::notebook_helpers_are_isolated_atomic_and_outlive_working_state -- --ignored --exact

cargo build -p aster-server
export ASTER_SERVER_BIN="${ASTER_SERVER_BIN:-${CARGO_TARGET_DIR:-target}/debug/aster-server}"
python3 tests/ai-selection-browser.py
python3 tests/conversation-browser.py
echo 'ai-selection-validation OK'
