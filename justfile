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
    if helm template aster {{chart}} --set ingress.enabled=true >/dev/null 2>&1; then
      echo "development identity must not render with ingress enabled" >&2
      exit 1
    fi
    if helm template aster {{chart}} --set ingress.enabled=true --set server.identity.kind=oidc >/dev/null 2>&1; then
      echo "ingress requires a configured OIDC issuer" >&2
      exit 1
    fi

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

# Real default/delegated conversation persistence against two disposable databases.
conversation-postgres:
    bash tests/conversation-postgres.sh

# Browser interaction against an isolated server and fake helper.
conversation-browser:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p aster-server
    export ASTER_SERVER_BIN="${ASTER_SERVER_BIN:-${CARGO_TARGET_DIR:-target}/debug/aster-server}"
    python3 tests/conversation-browser.py

# Measured cell conversation geometry with the real browser; no provider calls.
cell-panel-visual:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p aster-server
    export ASTER_SERVER_BIN="${ASTER_SERVER_BIN:-${CARGO_TARGET_DIR:-target}/debug/aster-server}"
    python3 tests/cell-panel-visual.py

# Team page actions with the real browser script and a disposable transport.
team-browser-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 tests/team-browser.py
    echo 'team-browser-validation OK'

# Focused local Git transaction and notebook-format regressions.
notebook-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    report="$(mktemp)"
    trap 'rm -f "$report"' EXIT
    cargo test -p aster-server --lib gitstore::tests -- --list | grep -q '^gitstore::tests::.*: test$'
    cargo test -p aster-server --lib checkout_lock_ -- --list | grep -q '^gitstore::tests::checkout_lock_.*: test$'
    cargo test -p aster-core --lib notebook::tests -- --list | grep -q '^notebook::tests::.*: test$'
    cargo test -p aster-server --lib gitstore::tests
    cargo test -p aster-core --lib notebook::tests
    cargo test -p aster-core --test features 2>&1 | tee "$report"
    grep -Eq '[1-9][0-9]* scenarios \([1-9][0-9]* passed\)' "$report"
    grep -Fq 'Scenario: A SQL comment resembling a cell marker remains SQL' "$report"
    echo 'notebook-validation OK'

# Personal helper validation and subject-isolated fake-upstream routing.
ai-registration-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p aster-server --lib ai::tests -- --list | grep -q '^ai::tests::.*: test$'
    cargo test -p aster-server --test ai_registration -- --list | grep -q ': test$'
    cargo test -p aster-core --lib llm::tests -- --list | grep -q '^llm::tests::.*: test$'
    cargo test -p aster-server --lib ai::tests
    cargo test -p aster-server --test ai_registration
    cargo test -p aster-core --lib llm::tests
    echo 'ai-registration-validation OK'

# Bound browse-catalog admission scenarios, including REST and Connect RPC.
catalog-routing-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    report="$(mktemp)"
    trap 'rm -f "$report"' EXIT
    cargo test -p aster-server --test contracts -- --tags @catalog-routing 2>&1 | tee "$report"
    grep -Eq '[1-9][0-9]* scenarios \([1-9][0-9]* passed\)' "$report"
    echo 'catalog-routing-validation OK'

# Protected bindings refuse shared-engine execution and privileged metadata.
backend-identity-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    report="$(mktemp)"
    trap 'rm -f "$report"' EXIT
    cargo test -p aster-server --test backend_identity -- --list >"$report"
    grep -q '^protected_binding_refuses_rest_connect_and_legacy_queries_before_engine_call: test$' "$report"
    grep -q '^missing_binding_cannot_browse_privileged_catalog_metadata: test$' "$report"
    cargo test -p aster-server --test backend_identity
    cargo test -p aster-core --lib config::tests
    cargo test -p aster-server --test polaris_generic_browse
    echo 'backend-identity-validation OK'

# Synthetic verified-session handoff; production Trino/Spark remain closed.
verified-engine-handoff-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    report="$(mktemp)"
    trap 'rm -f "$report"' EXIT
    cargo test -p aster-server --test backend_identity -- --list >"$report"
    grep -q '^protected_query_requires_a_live_session_before_authenticated_engine_handoff: test$' "$report"
    cargo test -p aster-core --lib state::tests::only_verified_identity_creation_marks_a_session_verified -- --list >"$report"
    grep -q '^state::tests::only_verified_identity_creation_marks_a_session_verified: test$' "$report"
    cargo test -p aster-engines --lib protected_trino_and_spark_refuse_without_backend_delegation -- --list >"$report"
    grep -q '^tests::protected_trino_and_spark_refuse_without_backend_delegation: test$' "$report"
    cargo test -p aster-server --test backend_identity
    cargo test -p aster-core --lib state::tests::only_verified_identity_creation_marks_a_session_verified -- --exact
    cargo test -p aster-engines --lib tests::protected_trino_and_spark_refuse_without_backend_delegation -- --exact
    just shared-group-validation
    echo 'verified-engine-handoff-validation OK'

# Notebook-scoped helper state through REST/Connect, Valkey and a real browser.
ai-selection-validation:
    bash tests/ai-selection.sh

# Verified OIDC group transport through sessions and exact grant matching.
shared-group-validation:
    bash tests/shared-group.sh

# Pinned Polaris Generic Table metadata, REST and Connect browse behavior.
polaris-generic-adapter-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p aster-catalogs --test polaris_generic -- --list | grep -q ': test$'
    cargo test -p aster-server --test polaris_generic_browse -- --list | grep -q ': test$'
    cargo test -p aster-catalogs --test polaris_generic
    cargo test -p aster-server --test polaris_generic_browse
    echo 'polaris-generic-adapter-validation OK'

# Disposable Polaris 1.7 Generic Delta registration and real Spark row reads.
polaris-delta-validation:
    python3 tests/polaris-delta-live.py

# Disposable Trino 483 JWT/TLS, Iceberg, backend user policy and Aster route.
trino-data-policy-validation:
    python3 tests/trino-data-policy.py gate

# Actual Rust Spark Connect wire into a disposable authenticated fake proxy.
# This proves handoff only; protected production Spark remains disabled.
spark-proxy-handoff-validation:
    python3 tests/spark-proxy-wire.py gate

# Disposable Polaris Generic Delta rows with distinct RustFS IAM keys.
# Storage-policy proof only; protected Spark remains disabled.
spark-generic-storage-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 tests/spark-generic-alice.py gate
    echo 'spark-generic-storage-validation OK'

# Legacy checkout ownership, content revision preconditions and API parity.
notebook-ownership-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    report="$(mktemp)"
    trap 'rm -f "$report"' EXIT
    cargo test -p aster-server --test notebook_ownership -- --list | grep -q ': test$'
    cargo test -p aster-server --test notebook_ownership
    just notebook-validation
    cargo test -p aster-server --test contracts -- --tags @notebook-ownership 2>&1 | tee "$report"
    grep -Eq '[1-9][0-9]* scenarios \([1-9][0-9]* passed\)' "$report"
    bash tests/notebook-ownership-postgres.sh
    echo 'notebook-ownership-validation OK'

# Disposable team workspaces; this is local branch isolation, not GitHub Sync.
notebook-isolation-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p aster-server --test notebook_isolation -- --list | grep -q ': test$'
    cargo test -p aster-server --test notebook_isolation
    echo 'notebook-isolation-validation OK'

# Explicit Sync to a disposable bare repository; GitHub App proof is separate.
notebook-sync-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p aster-server --test notebook_remote_sync -- --list | grep -q ': test$'
    cargo test -p aster-server --lib remote_ref_ -- --list | grep -q ': test$'
    cargo test -p aster-server --lib pending_push_ -- --list | grep -q ': test$'
    cargo test -p aster-server --lib one_repository_id_cannot_be_assigned_to_two_team_policies -- --list | grep -q ': test$'
    cargo test -p aster-server --test notebook_remote_sync
    cargo test -p aster-server --lib remote_ref_
    cargo test -p aster-server --lib pending_push_
    cargo test -p aster-server --lib one_repository_id_cannot_be_assigned_to_two_team_policies
    echo 'notebook-sync-validation OK'

# Durable team target and audit CAS in disposable PostgreSQL; no GitHub activation.
notebook-team-target-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    report="$(mktemp)"
    trap 'rm -f "$report"' EXIT
    bash tests/notebook-team-target-postgres.sh 2>&1 | tee "$report"
    grep -q '^notebook-team-target-postgres OK$' "$report"
    echo 'notebook-team-target-validation OK'

# Fake scoped GitHub App API plus disposable authenticated HTTPS Git transport.
github-app-adapter-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p aster-server --test github_app -- --list | grep -q ': test$'
    cargo test -p aster-server --lib credential_helper_refuses_a_different_origin -- --list | grep -q ': test$'
    cargo test -p aster-server --test github_app
    cargo test -p aster-server --lib credential_helper_refuses_a_different_origin
    python3 tests/notebook-git-https.py
    echo 'github-app-adapter-validation OK'

# Admin-only encrypted shared registrations; grants and use are a later gate.
shared-model-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p aster-server --test shared_models -- --list | grep -q ': test$'
    cargo test -p aster-server --lib shared_models::tests -- --list | grep -q ': test$'
    cargo test -p aster-server --lib shared_models::tests
    cargo test -p aster-server --test shared_models
    bash tests/shared-model-postgres.sh
    bash tests/shared-model-chart.sh
    echo 'shared-model-validation OK'

# Fresh identity, scoped shared use, persisted choice and browser revocation.
shared-model-access-validation:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p aster-server --test shared_model_access -- --list | grep -q ': test$'
    cargo test -p aster-server --lib current_identity::tests -- --list | grep -q ': test$'
    cargo test -p aster-server --test shared_model_access
    cargo test -p aster-server --lib current_identity::tests
    just shared-model-validation
    just ai-selection-validation
    echo 'shared-model-access-validation OK'
