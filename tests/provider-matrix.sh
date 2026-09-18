#!/usr/bin/env bash
# Binding for docs/provider-matrix.md: every port in the workspace is listed
# there, every listed contract exists, and every selection site refuses an
# unknown provider name.
set -euo pipefail
cd "$(dirname "$0")/.."

failures=0

check() {
  local description="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo "ok: $description"
  else
    echo "FAIL: $description"
    failures=$((failures + 1))
  fi
}

matrix="docs/provider-matrix.md"

check "the provider matrix exists" test -f "$matrix"

# Every trait declared in the workspace must be classified in the matrix.
traits=$(grep -rhoE '^pub trait [A-Za-z_]+' crates/*/src | awk '{print $3}' | sort -u)
for trait in $traits; do
  check "the matrix lists port $trait" grep -q "\`$trait\`" "$matrix"
done

# Every contract named in the matrix must exist on disk.
for feature in $(grep -oE 'crates/[a-z]+/features/[a-z-]+\.feature' "$matrix" | sort -u); do
  check "the matrix contract $feature exists" test -f "$feature"
done

# Selection sites: unknown names are refused, not defaulted.
check "engine kinds are validated" grep -q 'unknown engine kind' crates/engines/src/lib.rs
check "catalog kinds are validated" grep -q 'unknown catalog kind' crates/catalogs/src/lib.rs
check "store providers are validated" grep -q 'unknown secret provider' crates/server/src/providers.rs
check "every store domain is selected in one module" bash -c \
  'for kind in secret_store metadata state notebooks; do grep -q "pub .*fn $kind" crates/server/src/providers.rs || exit 1; done'

# The ports that carry per-provider behaviour keep at least one real and one
# in-memory implementation, so the contract tests never need a live dependency.
check "sessions ship a valkey and a memory provider" bash -c \
  'grep -q "impl SessionRegistry for ValkeySessions" crates/server/src/state.rs &&
   grep -q "impl SessionRegistry for InMemorySessions" crates/core/src/state.rs'
check "secrets ship an env and a memory provider" bash -c \
  'grep -q "impl SecretStore for EnvSecrets" crates/core/src/secrets.rs &&
   grep -q "impl SecretStore for InMemorySecrets" crates/core/src/secrets.rs'

if [ "$failures" -eq 0 ]; then
  echo "provider-matrix OK"
else
  echo "provider-matrix FAILED ($failures check(s))"
  exit 1
fi
