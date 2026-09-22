#!/usr/bin/env bash
# Binding for features/repo-setup.feature.
#
# These are tooling assertions, so they are checked here instead of through a
# Gherkin runner: every check prints what it verified, and any failure exits
# non-zero so `just ci` stops.
set -euo pipefail

cd "$(dirname "$0")/.."

failures=0

check() {
  local description="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo "ok: ${description}"
  else
    echo "FAIL: ${description}" >&2
    failures=$((failures + 1))
  fi
}

contains() {
  grep -Eq "$2" "$1"
}

# --- toolchain is declared ---------------------------------------------------
check "devenv.yaml requests the rolling devenv nixpkgs input" \
  contains devenv.yaml 'github:cachix/devenv-nixpkgs/rolling'
for tool in cargo rustc rustfmt clippy just; do
  check "devenv.nix provides ${tool}" contains devenv.nix "pkgs\\.${tool}"
done
check "devenv.nix provides helm" contains devenv.nix 'pkgs\.kubernetes-helm'
check "rust-toolchain.toml pins a channel" contains rust-toolchain.toml 'channel'

# --- one gate ----------------------------------------------------------------
check "justfile defines a ci gate" contains justfile '^ci:'
check "the ci gate includes the repository contract" contains justfile 'repo-check'
check "devenv enterTest runs just" contains devenv.nix 'just ci'

# --- contracts sit next to their code ---------------------------------------
check "the features recipe covers root and crate contracts" \
  bash -c 'grep -q "features/\*.feature" justfile && grep -q "crates/\*/features/\*.feature" justfile'
for file in features/*.feature crates/*/features/*.feature; do
  check "${file} declares a Feature" contains "$file" '^Feature:'
  check "${file} declares at least one Scenario" contains "$file" 'Scenario'
done
for crate in core engines catalogs server controller tui; do
  check "crates/${crate}/features exists" test -d "crates/${crate}/features"
done

# --- local stack -------------------------------------------------------------
check "docker-compose.yml declares postgres, server and controller" \
  bash -c 'grep -q "^  postgres:" docker-compose.yml && grep -q "^  server:" docker-compose.yml && grep -q "^  controller:" docker-compose.yml'
check "the compose file is valid for the container engine" \
  docker compose -f docker-compose.yml config

# --- chart -------------------------------------------------------------------
for file in charts/aster/Chart.yaml charts/aster/values.yaml charts/aster/values.schema.json; do
  check "${file} exists" test -f "$file"
done
check "the chart ships a Secret template" test -f charts/aster/templates/secret.yaml
check "the justfile renders and validates the chart" contains justfile 'kubeconform'
check "the chart exposes a configurable image repository" \
  contains charts/aster/values.schema.json '"repository"'
check "the chart names no private registry" \
  bash -c '! grep -qiE "harbor" charts/aster/values.yaml charts/aster/values.schema.json charts/aster/templates/*.yaml'
check "the values schema accepts an immutable digest" \
  contains charts/aster/values.schema.json 'sha256:\[0-9a-f\]\{64\}'

# --- secrets -----------------------------------------------------------------
check "no tracked *.secrets.json" \
  bash -c '! git ls-files -z | tr "\0" "\n" | grep -q "\.secrets\.json$"'
check "no tracked .env file" bash -c '! git ls-files -z | tr "\0" "\n" | grep -qE "(^|/)\.env$"'
check "values.yaml carries no credential keys" \
  bash -c '! grep -Eq "client_secret|api_key|password" charts/aster/values.yaml'
check ".gitignore covers local secrets" bash -c 'grep -q "\*.secrets.json" .gitignore && grep -q "^\.env$" .gitignore'

if [ "$failures" -ne 0 ]; then
  echo "repo-setup FAILED (${failures} check(s))" >&2
  exit 1
fi
echo "repo-setup OK"
