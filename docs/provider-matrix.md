# Provider matrix

Every domain that talks to an external system is a trait in `aster-core` plus
one implementation per provider. Selection is by name (`ASTER_*_STORE`,
`ASTER_ENGINES`, `ASTER_CATALOGS`), never by reaching for a concrete type at the
call site, and an unknown name is refused at startup instead of failing on the
first request.

`tests/provider-matrix.sh` (run by `just ci`) enforces this file: every
`pub trait` in the workspace must appear in the table below, every feature path
must exist, and every selection site must refuse unknown names.

## Abstracted domains

| Domain | Trait | Providers | Selected by | Contract |
|---|---|---|---|---|
| Query engine | `QueryEngine` (`crates/core/src/engine.rs`) | `trino`, `spark` (stub), `starrocks` (stub) | `engine_from_config` on the `kind` field of `ASTER_ENGINES` | `crates/engines/features/engine-pool.feature` |
| Catalog | `Catalog` (`crates/core/src/catalog.rs`) | `polaris`, `cube` (semantic layer, read-only), `nessie` (stub), `unity` (stub), `mock` (demo) | `catalog_from_config` on the `kind` field of `ASTER_CATALOGS`; `catalog` carries the warehouse/prefix or, for Cube, the base path | `crates/catalogs/features/polaris-catalog.feature`, `crates/catalogs/features/cube-catalog.feature` |
| Notebook store | `NotebookStore` (`crates/core/src/notebook.rs`) | `git` | `providers::notebooks` on `ASTER_NOTEBOOK_STORE` | `crates/server/features/notebooks.feature` |
| Secret store | `SecretStore` (`crates/core/src/secrets.rs`) | `env`, `memory` | `providers::secret_store` on `ASTER_SECRET_STORE` | `crates/core/features/secrets.feature` |
| Identity provider | `IdentityProvider` (`crates/core/src/identity.rs`) | `oidc` (Authentik and Keycloak differ only in configuration), `none` | `providers::identity` on `ASTER_IDP_KIND` | `crates/server/features/sso.feature` (draft; the group/claim mapping is covered by the unit tests in `crates/server/src/identity.rs` and the Keycloak eval in the README) |
| Engine grants | `Grants` (`crates/core/src/grants.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE` | `crates/core/features/authorization.feature` |
| Audit sink | `AuditSink` (`crates/core/src/audit.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE` | `crates/server/features/audit-persistence.feature` |
| LLM endpoint store | `LlmStore` (`crates/core/src/llm.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE` | `crates/server/features/ai-assist.feature` |
| Sessions | `SessionRegistry` (`crates/core/src/state.rs`) | `valkey`, `memory` | `providers::state` on `ASTER_STATE_STORE` | `crates/core/features/session-state.feature` |
| OIDC handshakes | `HandshakeStore` (`crates/core/src/state.rs`) | `valkey`, `memory` | `providers::state` on `ASTER_STATE_STORE` | `crates/core/features/session-state.feature` |
| Working state | `UserState` (`crates/core/src/state.rs`) | `valkey`, `memory` | `providers::state` on `ASTER_STATE_STORE` | `crates/core/features/working-state.feature` |

Adding a provider = a new implementation plus one arm in the selection
function (`crates/engines/src/lib.rs`, `crates/catalogs/src/lib.rs`, or
`crates/server/src/providers.rs`). No other crate changes.

## Deliberately not abstracted

Each row names what would force the abstraction, so the decision is on record
instead of implicit.

| Domain | Today | Abstract when |
|---|---|---|
| Object storage | nothing stores blobs; notebooks are a git working tree and contracts are files | a second notebook/blob backend is wanted — add the arm in `providers::notebooks` |
| Metadata DB internals for the controller | the controller issues its two statements directly (`crates/controller/src/main.rs`) | a second metadata database must be supported |
| Clock | `aster-core` ports already take `now`; the server layer calls `SystemTime::now()` | a server-level scenario must freeze time |
| Telemetry | process-wide `tracing` subscriber, chosen in each binary | a second exporter or sink is required |
| Migration runner | idempotent `migrations/*.sql` applied with `sqlx::raw_sql` at boot | the schema starts evolving (move to `sqlx::migrate!`) |
| Notebook text format | one `.aster` format (D8); `to_text`/`from_text` in `crates/core/src/notebook.rs` | a second notebook format must round-trip |
| HTTP client | one client per component (engines, catalogs, server, OIDC) | providers need different transport policy (proxies, per-host limits) |
| Role parsing | three call sites (core serde, server `parse_role`, OIDC mapping) | a role is added — collapse into one `FromStr` first |
| Non-secret configuration | environment via `AppConfig::from_env` | values must come from a file or an API |
