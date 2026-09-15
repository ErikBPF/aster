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
  Shipping today: `TrinoEngine` (real, Trino HTTP protocol). Registered stubs with
  the same interface: `SparkEngine`, `StarRocksEngine`. Adding an engine means one
  impl plus one config entry; no changes to the server.
- **Catalog/authorization-agnostic.** Every catalog implements `Catalog`. Shipping
  today: `PolarisCatalog` (Apache Polaris Iceberg REST). Stubs: `NessieCatalog`,
  `UnityCatalog`. The catalog decides data permissions; the app does not hardcode
  Polaris.
- **Rust-only.** Workspace crates: `core` (domain + traits), `engines`, `catalogs`,
  `server` (axum, JSON API), `tui` (ratatui). Web client is a later crate.

## Layout

```
crates/core      domain types, QueryEngine/Catalog/NotebookStore traits, registries, RBAC
crates/engines   Trino (real) + Spark/StarRocks (stubs)
crates/catalogs  Polaris (real) + Nessie/Unity (stubs)
crates/server    axum JSON API wiring the registries
crates/tui       ratatui client
docker/          dev + server + tui images
```

## Build and test (no local toolchain required)

Everything runs in containers because the dev host has Docker but no Rust.
`just` is optional; the raw `docker build`/`docker run` commands are equivalent.

```
just check    # cargo check --workspace
just test     # cargo test --workspace
just clippy   # clippy -D warnings
just build    # production server image
just up       # docker compose: postgres + server
```

## Env

Copy `.env.example` to `.env`. Engine and catalog endpoints are declared in the
server config; `POSTGRES_*` is used by the server and compose.

## Roadmap

See the platform proposal. Thin vertical first: SSO login -> git-backed notebook ->
one SQL cell on one engine -> audit row. The engine/catalog pluggability in this
scaffold is the substrate for the later pool, catalog tab, and semantic layer.
