dev_image := "aster-dev:latest"
root := justfile_directory()
# Build host: Rust compiles on cache-host via nix-provided toolchain (no local cargo).
host := "cache-host"
nix_rust := "nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#rustfmt nixpkgs#clippy -c"

build-dev:
    docker build -f docker/Dockerfile.dev -t {{dev_image}} .

check: build-dev
    docker run --rm -v {{root}}:/app -v aster-cargo-target:/app/target -v aster-cargo-registry:/usr/local/cargo/registry -w /app {{dev_image}} cargo check --workspace --all-targets

test: build-dev
    docker run --rm -v {{root}}:/app -v aster-cargo-target:/app/target -v aster-cargo-registry:/usr/local/cargo/registry -w /app {{dev_image}} cargo test --workspace

fmt: build-dev
    docker run --rm -v {{root}}:/app -v aster-cargo-registry:/usr/local/cargo/registry -w /app {{dev_image}} cargo fmt --all

clippy: build-dev
    docker run --rm -v {{root}}:/app -v aster-cargo-target:/app/target -v aster-cargo-registry:/usr/local/cargo/registry -w /app {{dev_image}} cargo clippy --workspace --all-targets -- -D warnings

build:
    docker build -f docker/Dockerfile.server -t aster-server:local .

build-tui:
    docker build -f docker/Dockerfile.tui -t aster-tui:local .

up:
    docker compose up -d --build

down:
    docker compose down -v

# Validate that every in-repo behavior contract is well-formed. Cheap guard so
# the .feature files cannot rot; running them needs a Gherkin runner (queued).
features-check:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob
    files=(features/*.feature)
    [[ ${#files[@]} -gt 0 ]] || { echo "no .feature files"; exit 1; }
    fail=0
    for f in "${files[@]}"; do
      grep -q '^Feature:' "$f" || { echo "missing Feature: in $f"; fail=1; }
      grep -q 'Scenario' "$f" || { echo "no Scenario in $f"; fail=1; }
    done
    [[ $fail -eq 0 ]] && echo "features OK: ${#files[@]} file(s)"

# Mirror the working tree to the build host.
sync:
    rsync -a --exclude target {{root}}/ {{host}}:~/aster/

remote-check: sync
    ssh {{host}} 'cd ~/aster && {{nix_rust}} cargo check --workspace --all-targets'

remote-test: sync
    ssh {{host}} 'cd ~/aster && {{nix_rust}} cargo test --workspace'

remote-clippy: sync
    ssh {{host}} 'cd ~/aster && {{nix_rust}} cargo clippy --workspace --all-targets -- -D warnings'

# Format on the build host and pull the result back (no local toolchain).
fmt-remote: sync
    ssh {{host}} 'cd ~/aster && {{nix_rust}} cargo fmt --all'
    rsync -a {{host}}:~/aster/crates/ {{root}}/crates/
    ssh {{host}} 'cd ~/aster && {{nix_rust}} cargo fmt --all -- --check'
