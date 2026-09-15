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
- **Rust-only.** Workspace crates: `core` (domain + traits), `engines`, `catalogs`,
  `server`, `controller`, `tui`.

## Layout

```
crates/core        domain types, QueryEngine/Catalog/NotebookStore/Grants/AuditSink/LlmStore
                   traits, registries, RBAC, text notebook format, data contracts
crates/engines     Trino (real, pool routing) + Spark/StarRocks (stubs)
crates/catalogs    Polaris (real) + Nessie/Unity (stubs)
crates/server      axum API + server-rendered UI, OIDC login, git store, Postgres store
crates/controller  reconcile loop: health mirror + audit retention
crates/tui         ratatui client over the same JSON API
features/          behavior contracts; one .feature per implemented behavior
contracts/         example ODCS-shaped data contracts
migrations/        metadata schema (grants, audit, engines/catalogs, llm configs)
docker/            dev + server + tui images
```

## Build and test

The dev host has Docker but no Rust and no cargo; builds run on `cache-host` through
`nix shell`, driven by `just`. Local equivalents with a container image exist too.

```
just sync            # rsync the workspace to cache-host
just remote-check    # cargo check --workspace --all-targets
just remote-test     # cargo test --workspace
just remote-clippy   # clippy -D warnings
just fmt-remote      # format on cache-host, pull the result back, verify
just features-check  # every features/*.feature has a Feature and a Scenario
just build-dev       # container image with rustfmt + clippy
just build           # production image (server + controller)
just build-tui       # TUI image
just up / just down  # compose: postgres + server
```

## Env

- `ASTER_BIND` (default `0.0.0.0:8080`), `DATABASE_URL` (unset = in-memory grants,
  audit and LLM configs).
- `ASTER_ENGINES` = `id;kind;endpoint[;routing_group]` comma-separated;
  `ASTER_CATALOGS` = `id;kind;endpoint[;catalog]`.
  `ASTER_DEFAULT_ENGINE`, `ASTER_DEFAULT_CATALOG`. `TRINO_ENDPOINT` and
  `POLARIS_ENDPOINT`/`POLARIS_CATALOG` still configure the single default entry.
- `ASTER_GRANTS` = `subject:engine,...` seeds grants at boot (dev).
- `ASTER_NOTEBOOK_DIR` (default `data/notebooks`), `ASTER_NOTEBOOK_BRANCH`
  (default `session`).
- `ASTER_CONTRACTS_DIR` (default `contracts`) — JSON contracts; yaml is skipped
  with a warning.
- `ASTER_SESSION_KEY` (signs session and handshake cookies), `ASTER_OIDC_ISSUER`,
  `ASTER_OIDC_CLIENT_ID`, `ASTER_OIDC_CLIENT_SECRET`, `ASTER_OIDC_REDIRECT_URI`,
  `ASTER_OIDC_ADMIN_GROUP`, `ASTER_OIDC_EDITOR_GROUP`. Without an issuer the
  header/cookie dev seam (`x-aster-subject`, `x-aster-roles`, `/dev-login`) is
  accepted; `ASTER_DEV_LOGIN` keeps it on alongside SSO.
- Controller: `ASTER_RECONCILE_SECONDS` (30), `ASTER_AUDIT_RETENTION_DAYS` (30).
- TUI: `ASTER_SERVER`, `ASTER_SUBJECT`, `ASTER_ROLES`.

There is no `.env` file in the repository; export these or set them in the
deployment.

## Status

Thin vertical is implemented: SSO (or the dev seam) -> git-backed notebook ->
pooled engine execution with per-subject grants -> audit row, plus catalog
browsing, the controller, the TUI, LLM-assisted cells and data contracts. The
deployment manifests live in `platform-gitops` under `apps/platform/aster` and stay
unsynced until the prerequisite runtime (proposal D6) and the published Harbor
image exist.
