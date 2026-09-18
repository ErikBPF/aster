# aster workflows. Recipes are the single source of truth: CI, devenv's
# `enterTest`, and humans all call into this file.
#
# The workspace compiles on the remote build host (`just remote-*`), which owns
# the toolchain; devenv also provides it locally for quick edits and CI.
default:
    @just --list

root := justfile_directory()
dev_image := "aster-dev:latest"
host := env_var_or_default("ASTER_BUILD_HOST", "cache-host")
remote := "devenv shell --"
container_engine := env_var_or_default("CONTAINER_ENGINE", `command -v podman >/dev/null 2>&1 && echo podman || echo docker`)
chart := "charts/aster"

# ---------------------------------------------------------------- local build
# Inside devenv these run against the declared toolchain; `docker-*` variants
# exist for hosts without devenv.

check:
    cargo check --workspace --all-targets

test:
    cargo test --workspace

fmt:
    cargo fmt --all

format-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

lint: clippy

build-dev:
    {{container_engine}} build -f docker/Dockerfile.dev -t {{dev_image}} .

docker-check: build-dev
    {{container_engine}} run --rm -v {{root}}:/app -v aster-cargo-target:/app/target -v aster-cargo-registry:/usr/local/cargo/registry -w /app {{dev_image}} cargo check --workspace --all-targets

docker-test: build-dev
    {{container_engine}} run --rm -v {{root}}:/app -v aster-cargo-target:/app/target -v aster-cargo-registry:/usr/local/cargo/registry -w /app {{dev_image}} cargo test --workspace

docker-clippy: build-dev
    {{container_engine}} run --rm -v {{root}}:/app -v aster-cargo-target:/app/target -v aster-cargo-registry:/usr/local/cargo/registry -w /app {{dev_image}} cargo clippy --workspace --all-targets -- -D warnings

# ------------------------------------------------------------------- contracts

# Every behavior contract must live beside the code it validates and stay
# well-formed: crates/<crate>/features/<name>.feature for behavior and
# features/<name>.feature for this repository's own conventions.
features:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    files=(features/*.feature crates/*/features/*.feature)
    [[ ${#files[@]} -gt 0 ]] || { echo "no .feature files found"; exit 1; }
    fail=0
    for f in "${files[@]}"; do
      grep -q '^Feature:' "$f" || { echo "missing Feature: in $f"; fail=1; }
      grep -q 'Scenario' "$f" || { echo "no Scenario in $f"; fail=1; }
    done
    [[ $fail -eq 0 ]] && echo "features OK: ${#files[@]} file(s)"

# Binding for features/repo-setup.feature: tooling assertions run by the gate.
repo-check: features
    bash tests/repo-setup.sh

# Binding for docs/provider-matrix.md: every port is classified there and every
# selection site refuses an unknown provider name.
providers-check:
    bash tests/provider-matrix.sh

# ------------------------------------------------------------------ aggregates

ci: format-check lint test features repo-check providers-check
    @echo "CI GREEN (fmt + clippy + tests + features + repo + providers)"

# Run the server on this host (the Keycloak recipe in the README needs it, since
# the issuer the browser sees must be the one discovery returns).
run:
    cargo run -p aster-server

# ------------------------------------------------------------------- containers

build:
    {{container_engine}} build -f docker/Dockerfile.server -t aster-server:local .

build-tui:
    {{container_engine}} build -f docker/Dockerfile.tui -t aster-tui:local .

compose-up:
    {{container_engine}} compose up -d --build

compose-down:
    {{container_engine}} compose down

compose-logs:
    {{container_engine}} compose logs -f server controller

# ------------------------------------------------------------------------- helm

# Render the chart and validate the result; the ecosystem has no helm-unittest,
# so kubeconform is the render gate.
chart-lint:
    #!/usr/bin/env bash
    set -euo pipefail
    helm dependency build {{chart}} 2>/dev/null || true
    helm template aster {{chart}} \
      | nix shell nixpkgs#kubeconform -c kubeconform -strict -summary -ignore-missing-schemas

chart-diff:
    helm template aster {{chart}}

# ------------------------------------------------------------- remote build host
# Building happens on {{host}} (32 cores) through its declared devenv; the local
# checkout stays the source of truth and `sync` mirrors it with --delete so stale
# files cannot survive. Build artefacts, devenv state and local run data are
# excluded, and both sides ignore them.

sync:
    rsync -a --delete --exclude target --exclude .devenv --exclude data {{root}}/ {{host}}:~/aster/

remote-check: sync
    ssh {{host}} 'cd ~/aster && {{remote}} cargo check --workspace --all-targets'

remote-test: sync
    ssh {{host}} 'cd ~/aster && {{remote}} cargo test --workspace'

remote-clippy: sync
    ssh {{host}} 'cd ~/aster && {{remote}} cargo clippy --workspace --all-targets -- -D warnings'

# Format on the build host and pull the result back, so the host owns the
# toolchain version that decides formatting.
fmt-remote: sync
    ssh {{host}} 'cd ~/aster && {{remote}} cargo fmt --all'
    rsync -a {{host}}:~/aster/crates/ {{root}}/crates/
    ssh {{host}} 'cd ~/aster && {{remote}} cargo fmt --all -- --check'

# Full gate on the build host.
remote-ci: sync
    ssh {{host}} 'cd ~/aster && {{remote}} just ci'
    @echo "REMOTE CI GREEN (fmt + clippy + tests + features + repo + providers)"
