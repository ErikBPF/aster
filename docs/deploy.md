# Run and deploy Aster

Enter `devenv shell` in the repository root. Nix provides Rust, `just`,
Podman/Kubernetes tooling and `protoc`; [the justfile](../justfile) owns the
commands. Aster is a standalone application and the default stacks use mock
or public upstream services. Do not point the development identity seam at a
public endpoint.

| Path | Commands | What it proves |
|---|---|---|
| Bare server | `ASTER_BIND=127.0.0.1:8080 ASTER_ENGINES='mock-local;mock;local' ASTER_CATALOGS='mock-local;mock;local' ASTER_CATALOG_BINDINGS='mock-local;mock-local;mock-local;unprotected' just run` | Browser/API against in-process metadata and mock providers. Open `http://127.0.0.1:8080/dev-login`. No durable database or live query engine. |
| Compose | `just compose-up`; `just compose-logs`; `just compose-down` | Server, controller, PostgreSQL, Valkey and development Keycloak with mock catalog/engine. Compose creates named volumes for notebooks, database and state. |
| Full local stack | `just stack-bootstrap local`; `just stack-status` | Dedicated minikube with Aster, Polaris/RustFS, Trino/Gateway and Spark Connect. Resource defaults are 12 CPUs and 64 GiB; override `ASTER_STACK_CPUS` and `ASTER_STACK_MEMORY` for a smaller host. |
| Helm chart | `just chart-lint`; `just minikube-start`; `just build-minikube`; `just chart-install` | Standalone private development chart with its own PostgreSQL, Valkey and development login. Set reachable engine/catalog endpoints in chart values. [Chart README](../charts/aster/README.md) owns the supported values and OIDC setup for shared deployment. |

The DevSpace stack and standalone Helm chart are alternative installations in
the same local namespace. Choose one on a cluster. `just stack-down local`
purges the DevSpace profile; `just chart-uninstall` removes the Helm release.
Inspect state before removing volumes or deleting the minikube profile.
For the DevSpace stack, `kubectl -n aster port-forward svc/aster-server 8080:80`
exposes the UI on the workstation's loopback interface. Compose binds its API,
metrics, database, Valkey and Keycloak ports to loopback by default; set
`ASTER_PORT` and the other `ASTER_*_PORT` values only when another binding is
deliberately needed.

### Native benchmark notebook demo

The isolated Authentik demo has a checked-in overlay under `k8s/benchmark/`.
It updates an **existing** demo release and preserves its Secret references,
PostgreSQL ownership metadata, notebook PVC and identity proxy. It does not
provision Authentik users or replace the standard stack-bootstrap workflow.

Inside the declared devenv, with `KUBECONFIG` set to the isolated target and
`ASTER_KUBE_CONTEXT=aster-demo` (mutation recipes reject other contexts):

- `just benchmark-backend-up`: validate and deploy the native synthetic-only
  Trino `tpch`/`tpcds` backend.
- `just benchmark-bundle-check`: check every captured table/column against its
  separate pinned ODCS artifact, then validate the bundle through runtime intake.
- `just trino-catalog-check` and `just benchmark-ui-check`: focused protocol/UI
  checks; the UI check uses fixture HTTP responses, not live Trino.
- `ASTER_BENCHMARK_TRINO=http://localhost:18090 just trino-benchmark-live`: with
  a scoped Trino forward, verify actual adapter metadata for every tiny table.
- After CI, chart/Compose checks and `just build-minikube`, set
  `ASTER_DEMO_REGISTRATION` to the existing external registration record and run
  `just benchmark-deploy sha256:<loaded-image-digest>`. This preserves identity
  secrets by reference; never put credential values in the registration argument
  or chart values. Keep the prior image and Helm revision for rollback.
- On Build-host, set `ASTER_DEMO_EVIDENCE` and run `just benchmark-demo-e2e`. It opens
  scoped loopback forwards, reads demo login credentials directly into memory,
  exercises real login/notebooks/catalogs, temporarily revokes then restores
  Alice's exact group list, and writes non-secret receipts. Run again after a
  server restart with `ASTER_DEMO_NOTEBOOK=<receipt notebook>` to prove persistence.

The bundle covers 8 TPC-H and 25 TPC-DS tables (including `dbgen_version`). It is
synthetic descriptive material, not business SLAs or measured quality. Browser
catalog/schema selection routes SQL and persists with the cell; it is not SQL
target authorization. Keep this backend isolated from real data.

## Configuration boundaries

| Setting | Meaning |
|---|---|
| `ASTER_BIND`, `ASTER_METRICS_BIND` | API listener (default `0.0.0.0:8080`) and separate metrics listener (default `0.0.0.0:9090`). Bind to loopback when using dev login on a remote host. |
| `DATABASE_URL`, `ASTER_METADATA_STORE` | PostgreSQL metadata or process memory. PostgreSQL holds grants, audit, personal helpers and default chat history. |
| `ASTER_STATE_URL`, `ASTER_STATE_STORE` | Valkey or process memory for sessions, handshakes and working state. |
| `ASTER_CONVERSATION_DATABASE_URL`, `ASTER_CONVERSATION_STORE` | Optional independent PostgreSQL for private notebook chat; `metadata` is default, `postgres` selected when a dedicated URL is supplied, `memory` is disposable. |
| `ASTER_NOTEBOOK_DIR`, `ASTER_NOTEBOOK_BRANCH` | Git-backed notebook checkout and one configured local branch. |
| `ASTER_NOTEBOOK_MODE` | Explicit `local` mode keeps owner-checked local notebooks usable alongside contract-team policy. Team/GitHub workspace activation remains separate; existing default restrictions are retained. |
| `ASTER_ENGINES`, `ASTER_CATALOGS`, `ASTER_DEFAULT_ENGINE`, `ASTER_DEFAULT_CATALOG` | Engine/catalog inventory and defaults; syntax and current capabilities are in [catalogs and compute](catalogs-and-compute.md). |
| `ASTER_CATALOG_BINDINGS` | Comma-separated `browse_catalog_id;engine_id;native_sql_catalog;unprotected|protected` bindings. The policy field is required. `unprotected` is for isolated development fixtures. A protected Trino query needs the explicit token/CA configuration and enforcing backend policy described in [catalogs and compute](catalogs-and-compute.md); protected metadata and Spark remain closed. The binding alone does not establish per-user data policy. |
| `ASTER_GRANTS` | Development seed in `subject:engine` form; not a substitute for backend per-user data policy. |
| `ASTER_CONTRACTS_DIR` | Contract files loaded at startup. |
| `ASTER_IDP_KIND`, `ASTER_OIDC_*`, `ASTER_IDP_*`, `ASTER_DEV_LOGIN` | `none` selects development identity; `oidc` requires an issuer and client settings. With OIDC, do not enable the development seam on a public listener. |
| `ASTER_SHARED_MODEL_AUTHORITY_ENABLED`, `ASTER_SHARED_MODEL_USE_ENABLED`, `ASTER_AUTHENTIK_*` | Optional fresh Authentik authority for SSO shared-model administrators; ordinary shared use is a separate opt-in and remains off by default. The read token comes from a dedicated Secret, not chart values. Live revocation proof is required before enabling ordinary use. |
| `ASTER_TEAM_POLICY_FILE` | Server-owned JSON mapping team IDs to `member_claim`, `maintainer_claim` (distinct stable Authentik group UUIDs), and `allowed_repositories`. Enables the existing team registry and fresh Authentik identity without GitHub credentials or shared models. Requires OIDC, signed user UUID, HTTPS `ASTER_AUTHENTIK_API_ORIGIN` and Secret-backed `ASTER_AUTHENTIK_READ_TOKEN`; optional `ASTER_AUTHENTIK_CA_FILE` adds the isolated issuer CA. Mutually exclusive with `ASTER_TEAM_GIT_ENABLED`. Notebook targets remain inactive until verified; registration alone grants no Git access. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Optional trace export; without it, tracing remains on stdout. |

The chart takes secret data from its generated or named Kubernetes Secret; its
[values guide](../charts/aster/README.md) describes the external database path.
The disposable stack manifests include development credentials and are not
production secret templates. Do not put helper keys, database passwords or
OAuth credentials in a values file, committed manifest or support receipt.
Build-host is one named development host using the local minikube stack, not a
runtime dependency or a deployment target for every contributor.

## Identity and terminal client

**Authentik is the ODCS catalog demo identity platform.** The older Compose
Keycloak fixture only exercises generic OIDC login; it cannot supply Aster's
fresh Authentik membership checks. The isolated Build-host deployment and its
first-test evidence are recorded in [the current receipt](plans/odcs-ai-catalog-rv.md).

Without `ASTER_OIDC_ISSUER`, Aster accepts the development header/cookie
identity seam. The Compose Keycloak realm in
[`keycloak/aster-realm.json`](../keycloak/aster-realm.json) is a local OIDC
evaluation fixture. To use it, point the host-run server at the realm issuer,
set the client ID, client secret and redirect URI through the environment, and
leave `ASTER_DEV_LOGIN` unset. Authentik uses the same `oidc` provider with
its own issuer and client settings. `ASTER_OIDC_ADMIN_GROUP` and
`ASTER_OIDC_EDITOR_GROUP` map verified group claims to app roles; this does not
yet create per-user engine or catalog data identities.
`ASTER_IDP_USER_UUID_CLAIM` optionally names a signed top-level ID-token claim
carrying a stable UUID. An absent or malformed claim leaves the session without
that UUID. The claim alone does not enable shared-model access or fresh group
revocation: the independent HTTPS API reader and registered team mapping are
also required.

OIDC login is browser-bound: `/login` places its unpredictable per-attempt state
in a host-only, HttpOnly, SameSite=Lax cookie lasting five minutes. `/callback`
requires exactly one matching cookie **before** consuming the shared handshake.
Missing, mismatched or duplicate bindings neither consume the attempt nor clear
another pending login. Matching success, provider refusal, exchange failure or
expired handshake clears the binding; the shared store still enforces its own
expiry and atomic single use. PKCE and ID-token nonce validation are unchanged.
Cookie Secure handling uses the configured `ASTER_OIDC_REDIRECT_URI`, never
Host/Forwarded headers. HTTPS (and unknown callback configuration) uses a
`__Host-` binding cookie with Path=/, Secure and no Domain. Explicit HTTP demo
callbacks use the non-prefixed cookie without Secure. Session creation/deletion
uses the same configured Secure policy.

Demo passwords remain in Secret `aster-authentik-demo-login`, namespace `aster`,
on the isolated `aster-demo` context. Consumed local `*.secrets.json`
handoffs must be deleted. Use an authorized Kubernetes secret viewer through
Build-host for interactive credential access; do not dump or copy values into chat,
logs or a new handoff file.

The TUI uses the older JSON API and sends development identity headers from
`ASTER_SUBJECT` and `ASTER_ROLES` (`alice` and `editor` by default). With the
server running in development mode, `cargo run -p aster-tui` connects to
`ASTER_SERVER` (default `http://127.0.0.1:8080`). It is not an OIDC login
client; those headers must not be enabled on a public deployment.
