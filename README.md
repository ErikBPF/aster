# aster

Git-backed SQL notebook platform. Engine-agnostic query execution, pluggable
catalog/authorization backend, Rust-only.

The project name is `aster` (settled 2026-09-14; no Apache project named
"Apache Aster" — the nearest is Apache AsterixDB, which is distinct). This is a
temporary local repository until the GitHub repository is created through
`platform-iac` (see the platform proposal
`docs/proposals/2026-09-14-aster-sql-notebook-platform.md`).

## Design

- **Engine-agnostic.** Every query engine implements `QueryEngine` (`crates/core`).
  Shipping today: `TrinoEngine` (real, Trino HTTP protocol, optional
  `X-Trino-Routing-Group` for gateway pools). Registered stubs with the same
  interface: `SparkEngine`, `StarRocksEngine`. Adding an engine means one impl plus
  one config entry; no server changes.
- **Catalog/authorization-agnostic.** Every catalog implements `Catalog`. Shipping
  today: `PolarisCatalog` (Apache Polaris Iceberg REST). Stubs: `NessieCatalog`,
  `UnityCatalog`. The catalog decides data permissions; the app does not hardcode
  Polaris. Row filters and column masks are the engine's job (Trino), not the
  catalog's.
- **Git-backed notebooks.** Cells live in a text format (`# aster notebook v1`)
  that diffs cleanly; every save is a commit on the session branch.
- **Server plus controller.** The server (axum) serves the API, the server-rendered
  web UI and the JSON API the TUI uses; it is stateless apart from the notebook
  checkout. The controller owns migrations, engine/catalog health reconciliation and
  audit retention. Both are one binary each, same image.
- **Shared state plane.** Sessions and OIDC handshakes live in Valkey
  (`ASTER_STATE_URL`), keyed `aster:v1:<domain>:<entity>` with a TTL; the cookie
  carries an opaque id, so any replica resolves the same session and a restart does
  not sign anyone out. Postgres holds durable metadata, git holds notebooks.
- **Rust-only.** Workspace crates: `core` (domain + traits), `engines`, `catalogs`,
  `server`, `controller`, `tui`.
- **Multiprotocol.** `proto/aster.proto` is the only endpoint declaration; one
  registration serves gRPC (programs), Connect JSON (curl, browser) and gRPC-Web
  over the same listener. The `google.api.http` bindings in the proto are the
  machine-readable REST contract for a transcoding proxy. The older `/api/*` JSON
  handlers still back the server-rendered page and disappear once it moves.

## Layout

```
crates/core        domain types, QueryEngine/Catalog/NotebookStore/Grants/AuditSink/LlmStore
                   traits, registries, RBAC, text notebook format, data contracts,
                   semantic-model emission (Cube, ODCS)
crates/engines     Trino (real, pool routing) + Spark/StarRocks (stubs)
crates/catalogs    Polaris (real) + Nessie/Unity (stubs)
crates/server      axum API + server-rendered UI, RPC surface, OIDC login, git store
crates/controller  reconcile loop: health mirror + audit retention
crates/tui         ratatui client over the same API
proto/             aster.proto + the vendored google/api annotations it imports
crates/*/features/ behavior contracts, colocated with the crate they validate
contracts/         example data contracts; a real ODCS v3.2 yaml document
migrations/        metadata schema (grants, audit, engines/catalogs, llm configs)
docker/            dev + server + tui images
```

## Build and test

`devenv` is the only toolchain source: `devenv shell` provides Rust, protoc,
clippy and the container/Kubernetes tools, so a contributor needs Nix and
nothing else on the host. Every workflow is a `just` recipe, and `just ci` is
the gate.

```
devenv shell         # toolchain: cargo, rustfmt, clippy, protoc, just, kubectl…
just ci              # fmt + clippy -D warnings + tests + features + repo contract
just check           # cargo check --workspace --all-targets
just test            # cargo test --workspace
just features        # every .feature declares a Feature and a Scenario
just repo-check      # binding for features/repo-setup.feature
just providers-check # provider matrix and selection sites stay in sync
just build-dev       # container image with rustfmt + clippy
just build           # production image (server + controller)
just build-tui       # TUI image
just compose-up      # compose: valkey + postgres + server, mock engines
just chart-lint      # render charts/aster and validate with kubeconform
```

A remote build host is optional, not required: `just remote-check` /
`remote-test` / `remote-ci` rsync the workspace to `ASTER_BUILD_HOST` (default
`cache-host`) and run there when a faster machine is at hand.

### Self-contained development stack

`just stack-bootstrap` brings up the whole compute path locally, on a throwaway
minikube cluster: aster (server, Postgres, Valkey) plus Trino through Trino
Gateway and Spark Connect (YuniKorn + the Spark operator). It pulls only public
upstream images — no Harbor, no platform host, no private DNS — and needs only a
container driver (podman or docker).

```sh
devenv shell
just stack-bootstrap        # minikube-start + build-minikube + stack-up local
just stack-up trino         # profiles: local (default) | trino | spark | apps
just stack-status
cargo build -p aster-server
ASTER_LIVE_CONTEXT=aster tests/live-backends.sh
just stack-down && just minikube-delete
```

`just build-minikube` builds `aster-server:local` from `docker/Dockerfile.server`
and side-loads it into minikube's containerd, so the server under test is the
working tree with no registry involved. A profile only starts the backends it
names; the engines it leaves out report unhealthy in `/api/engines` without
blocking the server.

### Live compute backends

`features/live-backends.feature` is bound by `tests/live-backends.sh`, which
runs a built server against a real Trino (through the Trino Gateway) and a real
Spark Connect. By default it targets the self-contained minikube stack above; it
needs a reachable cluster, a built binary and two port-forwards, so it is **not**
part of `just ci`:

```
cargo build -p aster-server
ASTER_LIVE_CONTEXT=aster tests/live-backends.sh   # or ASTER_SERVER_BIN=... tests/live-backends.sh
```

It points `ASTER_ENGINES` at `trino-gw;trino;http://127.0.0.1:<port>;lab` and
`spark-live;spark;http://127.0.0.1:<port>`, forwards
`svc/trino-gateway:8080` (namespace `trino`) and
`svc/spark-connect-aster:15002` (namespace `spark-jobs`), and asserts rows from
both engines, the gateway's routing group and absent-group fallback, the
refused-grant path and the audit rows. The Spark engine sends the routing
group's `lab` only to Trino; the Spark endpoint is rewritten to the client's
`sc://host:port` form automatically. Point it at another cluster with
`ASTER_LIVE_CONTEXT`, and at other namespaces/ports with the `ASTER_LIVE_*`
overrides.

Calling a gRPC method with `grpcurl` (no reflection needed, the proto is in the
repository):

```
grpcurl -plaintext -import-path proto -proto aster.proto \
  -H 'x-aster-subject: alice' 127.0.0.1:8080 aster.v1.Aster/ListEngines
```

## Semantic models

A table's metadata can be rendered as the file a team commits: a Cube model for
the semantic layer, or an Open Data Contract Standard document. The target is
named per request (`cube` or `odcs`), and when a data contract with the same
name is loaded it wins — its semantic types, required flags, owner and
`transformLogic` reach the output. Without one, catalog columns become
dimensions and every field is a plain ODCS `column`.

```
grpcurl -plaintext -import-path proto -proto aster.proto \
  -H 'x-aster-subject: alice' -d '{"catalog":"polaris-local","namespace":"sales","table":"orders"}' \
  127.0.0.1:8080 aster.v1.Aster/RenderSemantic   # add "target":"cube" or "odcs"
```

The response carries `content` and the suggested `path` (`model/cubes/<table>.yml`,
`contracts/<table>.yaml`). Emission is pure — no store, no clock — so the output
is reproducible; `crates/core/features/semantic-models.feature` covers it, and
the ODCS side is checked by parsing the rendered document back. A Cube measure
takes its aggregation from the contract's `transformLogic` and otherwise assumes
`sum`, saying so in a comment instead of guessing quietly.

## Env

- `ASTER_BIND` (default `0.0.0.0:8080`), `DATABASE_URL` (unset = in-memory grants,
  audit and LLM configs).
- Provider selection, each refusing an unknown name at startup:
  `ASTER_SECRET_STORE` (`env`|`memory`, default `env`), `ASTER_METADATA_STORE`
  (`postgres`|`memory`, default `postgres` when `DATABASE_URL` is set),
  `ASTER_STATE_STORE` (`valkey`|`memory`, default `valkey` when
  `ASTER_STATE_URL` is set), `ASTER_NOTEBOOK_STORE` (`git`). The full matrix is
  in [docs/provider-matrix.md](docs/provider-matrix.md).
- `ASTER_ENGINES` = `id;kind;endpoint[;routing_group]` comma-separated;
  `ASTER_CATALOGS` = `id;kind;endpoint[;catalog]`. Kinds are `trino`, `spark`,
  `starrocks`, `mock` (engines) and `polaris`, `cube`, `openmetadata`, `nessie`,
  `unity`, `mock` (catalogs). For `polaris` the `catalog` field is the
  warehouse/prefix; for `cube` it is the base path (default `cubejs-api`) and the
  semantic model is browsed read-only from `/v1/meta`; for `openmetadata` it is
  an optional database fully-qualified name to narrow the schemas to, read from
  `/api/v1/databaseSchemas` and `/api/v1/tables`. `ASTER_DEFAULT_ENGINE`,
  `ASTER_DEFAULT_CATALOG`. `TRINO_ENDPOINT`
  and `POLARIS_ENDPOINT`/`POLARIS_CATALOG` still configure the single default
  entry.
- `mock` is a demo provider that answers with canned rows and metadata; it is for
  local work on the shell, never for a deployment.
- `ASTER_GRANTS` = `subject:engine,...` seeds grants at boot (dev).
- `ASTER_NOTEBOOK_DIR` (default `data/notebooks`), `ASTER_NOTEBOOK_BRANCH`
  (default `session`).
- `ASTER_CONTRACTS_DIR` (default `contracts`) — data contracts; `.json`, `.yaml` and `.yml` are read, and a real ODCS v3.2 document (name or `id`, `team.name`, `schema[].properties[]`, `semanticType`) is understood
  with a warning.
- `ASTER_STATE_URL` (Valkey URL, e.g. `redis://:password@valkey:6379`) — session
  and handshake state. Unset means per-process memory, so a second replica would
  not see the first one's sessions.
- `ASTER_SESSION_TTL_SECONDS` (28800), `ASTER_HANDSHAKE_TTL_SECONDS` (300).
  `ASTER_USER_STATE_TTL_SECONDS` (2592000).
- Identity: `ASTER_IDP_KIND` (`oidc`|`none`, default `oidc` when
  `ASTER_OIDC_ISSUER` is set), `ASTER_OIDC_ISSUER`, `ASTER_OIDC_CLIENT_ID`,
  `ASTER_OIDC_CLIENT_SECRET`, `ASTER_OIDC_REDIRECT_URI`, `ASTER_IDP_SCOPES`
  (default `openid,groups`), `ASTER_IDP_GROUPS_CLAIM` (default `groups`),
  `ASTER_OIDC_ADMIN_GROUP`, `ASTER_OIDC_EDITOR_GROUP`. `oidc` covers Authentik
  and Keycloak; the group claim may be a bare name or a full path
  (`/aster-admins`), so Authentik and Keycloak both map.
  Without an issuer the header/cookie dev seam (`x-aster-subject`, `x-aster-roles`,
  `/dev-login`) is accepted; `ASTER_DEV_LOGIN` keeps it on alongside SSO.
- Telemetry: `ASTER_METRICS_BIND` (default `0.0.0.0:9090`, empty disables the
  second listener) serves `/metrics` in OpenMetrics text on a port separate from
  the public one, so the ingress never exposes it. Per-request counters and a
  latency histogram are labelled by method, route and status, with the route
  taken from the matched pattern so cardinality stays bounded.
  `OTEL_EXPORTER_OTLP_ENDPOINT` exports spans over OTLP/HTTP; unset it and
  tracing stays on stdout. Sampling follows `OTEL_TRACES_SAMPLER` /
  `OTEL_TRACES_SAMPLER_ARG`, with `service.name` from `OTEL_SERVICE_NAME`.
- Controller: `ASTER_RECONCILE_SECONDS` (30), `ASTER_AUDIT_RETENTION_DAYS` (30).
- TUI: `ASTER_SERVER`, `ASTER_SUBJECT`, `ASTER_ROLES`.

There is no `.env` file in the repository; export these or set them in the
deployment.

## Working on the shell

The web shell needs no infrastructure: the `mock` providers answer with canned
rows and metadata, and with no identity provider configured the dev seam signs
you in as `alice`.

```sh
ASTER_ENGINES='mock-local;mock;local' \
ASTER_CATALOGS='mock-local;mock;local' \
just run
# then open http://localhost:8080, or drive it headless:
curl -c /tmp/c -b /tmp/c -L 'http://localhost:8080/dev-login?subject=alice&roles=editor'
```

The notebook page renders one editor per cell, a per-cell engine choice filled
from `/api/engines`, and a result grid; `⌘/Ctrl+Enter` runs the focused cell,
`Shift+Enter` runs it and moves on, `Run all` runs the notebook in order,
`⌘/Ctrl+S` saves, and every save is a git commit on the session branch. Each cell
carries the Jupyter `In [n]` / `Out [n]` counters, and the notebook toolbar picks
the AI helper the notebook asks. The layout, styles and script live in
`crates/server/assets/` and are compiled in with `include_str!`.

A user may register several OpenAI-compatible helpers, each under a short name,
and choose one per notebook:

```sh
curl -X PUT localhost:8080/api/llm/lab-qwen -H 'content-type: application/json' \
  -d '{"base_url":"http://llm.local:4000/v1","model":"qwen","api_key":"…"}'
curl localhost:8080/api/llm          # id, base_url and model; never the token
curl -X DELETE localhost:8080/api/llm/lab-qwen
```

`POST /api/ai` takes an optional `helper` name and falls back to the first
registered one; the settings page lists, adds and removes helpers.

## Identity providers

The server signs users in through an `IdentityProvider` port
(`crates/core/src/identity.rs`). `oidc` implements it, and Authentik (the fleet
IdP) and Keycloak differ only in configuration: Authentik defines a `groups`
client scope, so the default `ASTER_IDP_SCOPES=openid,groups` fits, while
Keycloak needs a client scope of that name or `ASTER_IDP_SCOPES=openid`. Group
values may be bare names or full paths (`/aster-admins`), so both IdPs map.

### Evaluating with Keycloak

`docker compose up -d keycloak` starts Keycloak 26.7.4 in dev mode and imports
`keycloak/aster-realm.json`: realm `aster`, groups `aster-admins` and
`aster-editors`, users `root` (admin), `alice` (editor) and `bob` (viewer), and
the confidential client `aster` with the group-membership mapper. The
credentials in that file are development values. Keycloak has no Harbor proxy
yet, which is why this service is local-only.

Run the server on the host, not in compose: the issuer it discovers has to be
the one the browser sees.

```sh
docker compose up -d keycloak
ASTER_BIND=127.0.0.1:8080 \
ASTER_IDP_KIND=oidc \
ASTER_OIDC_ISSUER=http://localhost:8090/realms/aster \
ASTER_OIDC_CLIENT_ID=aster \
ASTER_OIDC_CLIENT_SECRET=aster-dev-secret \
ASTER_OIDC_REDIRECT_URI=http://localhost:8080/callback \
ASTER_IDP_SCOPES=openid \
ASTER_IDP_GROUPS_CLAIM=groups \
ASTER_METRICS_BIND=127.0.0.1:9090 \
just run
```

Open http://localhost:8080: `/` redirects to `/login`, which redirects to
Keycloak. Sign in as `alice` for editor, `root` for admin, `bob` for viewer.

Headless check of the same wiring:

```sh
curl -s http://localhost:9090/metrics | grep aster_http_requests
curl -s -X POST http://localhost:8090/realms/aster/protocol/openid-connect/token \
  -d grant_type=password -d client_id=aster -d client_secret=aster-dev-secret \
  -d username=alice -d password=aster-dev-alice |
  jq -r .id_token | cut -d. -f2 | base64 -d | jq .groups
```

## Status

Thin vertical is implemented: SSO (or the dev seam) -> git-backed notebook ->
pooled engine execution with per-subject grants -> audit row, plus catalog
browsing, the controller, the TUI, LLM-assisted cells, data contracts, shared
session/working state in Valkey and the gRPC/Connect RPC surface in
`proto/aster.proto`. The deployment manifests live in `platform-gitops` under
`apps/platform/aster` and stay unsynced until the prerequisite runtime (proposal
D6) and the published Harbor image exist.
