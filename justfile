# aster workflows. Recipes are the single source of truth: CI, devenv's
# `enterTest`, and humans all call into this file.
#
# devenv provides the whole toolchain locally, so `just ci` needs nothing else.
# The `just remote-*` recipes are an optional way to offload a build over ssh.
default:
    @just --list

root := justfile_directory()
dev_image := "aster-dev:latest"
host := env_var_or_default("ASTER_BUILD_HOST", "")
remote := "devenv shell --"
container_engine := env_var_or_default("CONTAINER_ENGINE", `command -v podman >/dev/null 2>&1 && echo podman || echo docker`)
chart := "charts/aster"

# Local validation stack: minikube on build-host, driven by devspace (the pattern
# example-airflow uses). Dev-only; platform-gitops stays the authoritative
# deployment for the shared cluster.
minikube_profile := env_var_or_default("ASTER_MINIKUBE_PROFILE", "aster")
kube_context := env_var_or_default("ASTER_KUBE_CONTEXT", "aster")
stack_memory := env_var_or_default("ASTER_STACK_MEMORY", "65536")
stack_cpus := env_var_or_default("ASTER_STACK_CPUS", "12")

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

# Install the chart into the minikube cluster with the side-loaded image. The
# engine and catalog pools must point at endpoints reachable from the namespace.
chart-install: require-minikube
    helm upgrade --install aster {{chart}} --namespace aster --create-namespace

chart-uninstall:
    helm uninstall aster --namespace aster || true

# ------------------------------------------------------- local stack (minikube)
# A self-contained development stack on a throwaway minikube cluster: aster
# (server, postgres, valkey) plus the compute backends (Trino, Trino Gateway,
# Spark Connect with YuniKorn and the Spark operator). It pulls only public
# upstream images, so any host with a container driver (podman or docker) can run
# it; nothing here needs a platform host or a private registry.

# Point kubectl at the local stack.
use-minikube:
    kubectl config use-context {{kube_context}}

# Abort unless the current context is the local stack, so no recipe can hit the
# shared cluster by accident.
[private]
require-minikube:
    #!/usr/bin/env bash
    set -euo pipefail
    ctx=$(kubectl config current-context)
    if [ "$ctx" != "{{kube_context}}" ]; then
        echo "ERROR: kubectl context is '$ctx', expected '{{kube_context}}'." >&2
        echo "Run 'just use-minikube' to switch." >&2
        exit 1
    fi

# Start the minikube cluster (podman driver, containerd runtime) and open pod
# egress to the host network so image pulls and the Spark operator work.
minikube-start:
    #!/usr/bin/env bash
    set -euo pipefail
    if minikube status --profile={{minikube_profile}} >/dev/null 2>&1; then
        echo "minikube profile '{{minikube_profile}}' already running"
    elif [ "{{container_engine}}" = "podman" ]; then
        # Rootless podman needs the explicit opt-in; minikube otherwise assumes
        # the socket belongs to root.
        MINIKUBE_ROOTLESS=true minikube start \
            --profile={{minikube_profile}} \
            --driver=podman \
            --cpus={{stack_cpus}} \
            --memory={{stack_memory}} \
            --container-runtime=containerd
    else
        minikube start \
            --profile={{minikube_profile}} \
            --driver=docker \
            --cpus={{stack_cpus}} \
            --memory={{stack_memory}} \
            --container-runtime=containerd
    fi
    just use-minikube
    just setup-network

# Rootless podman uses netavark + pasta with --no-map-gw, so bridge containers
# cannot route outbound. Add ip_forward + MASQUERADE inside the rootless netns
# (where the podman1 bridge lives), owned by aardvark-dns. Idempotent.
setup-network:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "{{container_engine}}" != "podman" ]; then
        echo "setup-network: docker driver — no host NAT configuration needed."
        exit 0
    fi
    aardvark=$(pgrep -f aardvark-dns | head -1)
    if [ -z "$aardvark" ]; then
        echo "ERROR: aardvark-dns not found. Is minikube running?" >&2
        exit 1
    fi
    sudo nsenter -t "$aardvark" -n sysctl -w net.ipv4.ip_forward=1 >/dev/null
    if ! sudo nsenter -t "$aardvark" -n iptables -t nat -C POSTROUTING -s 192.168.49.0/24 -j MASQUERADE 2>/dev/null; then
        sudo nsenter -t "$aardvark" -n iptables -t nat -A POSTROUTING -s 192.168.49.0/24 -j MASQUERADE
    fi
    echo "setup-network: pod egress via podman1 ready"

minikube-stop:
    minikube stop --profile={{minikube_profile}}

# Delete the cluster and every volume it owns — the stack is disposable. The
# podman sweep is idempotent and clears the container/volume a failed start can
# leave behind, which minikube's own delete then cannot find.
minikube-delete:
    #!/usr/bin/env bash
    set -euo pipefail
    minikube delete --profile={{minikube_profile}} || true
    if [ "{{container_engine}}" = "podman" ]; then
        "{{container_engine}}" rm -f {{minikube_profile}} >/dev/null 2>&1 || true
        "{{container_engine}}" volume rm -f {{minikube_profile}} >/dev/null 2>&1 || true
    fi
    echo "deleted minikube profile '{{minikube_profile}}'"

# Build the server image and side-load it into minikube's containerd.
build-minikube: require-minikube
    #!/usr/bin/env bash
    set -euo pipefail
    image=aster-server:local
    "{{container_engine}}" build -t "$image" -f docker/Dockerfile.server .
    "{{container_engine}}" save --format docker-archive "$image" \
        | "{{container_engine}}" exec -i {{minikube_profile}} \
            sh -c '
              set -e
              ctr -n k8s.io images import -
              # kubelet resolves an unqualified manifest name to
              # docker.io/library/aster-server:local, but the imported archive is
              # tagged localhost/aster-server:local by podman (or aster-server:local
              # by docker). Add the alias kubelet actually looks for.
              for src in localhost/aster-server:local aster-server:local; do
                  if ctr -n k8s.io images ls -q | grep -qx "$src"; then
                      ctr -n k8s.io images tag --force "$src" docker.io/library/aster-server:local
                      break
                  fi
              done
            '
    echo "loaded $image into minikube"

# Deploy or update a profile of the stack: local (default) | trino | spark | apps.
stack-up profile="local": require-minikube
    devspace deploy --profile {{profile}}

# Tear a profile down again.
stack-down profile="local":
    devspace purge --profile {{profile}}

# One command from a bare host to a running stack: start minikube, build and
# side-load the server image, then deploy a profile.
stack-bootstrap profile="local": minikube-start build-minikube
    devspace deploy --profile {{profile}}
    @just stack-status

# What is up, and is the compute path answering.
stack-status: require-minikube
    #!/usr/bin/env bash
    set -euo pipefail
    kubectl get pods -A -l app.kubernetes.io/part-of=aster
    kubectl get sparkconnect -n spark-jobs 2>/dev/null || true
    kubectl get svc -n trino 2>/dev/null || true


# ------------------------------------------------------------- remote build host
# Optional: offload a build to another host over ssh (`ASTER_BUILD_HOST`). The
# local checkout stays the source of truth and `sync` mirrors it with --delete so
# stale files cannot survive. Build artefacts, devenv state and local run data
# are excluded, and both sides ignore them.

# Clear failure when the optional host is not configured.
require-build-host:
    @test -n "{{host}}" || { echo "ASTER_BUILD_HOST is unset: run 'just ci' locally, or set it to offload the build"; exit 1; }

sync: require-build-host
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
