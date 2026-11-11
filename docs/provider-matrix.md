# Provider matrix

External domains use ports in `aster-core`; the team Git verifier is local to
`aster-server`. Provider selection is by configured name or explicit opt-in,
and unknown store, engine and catalog names are refused at startup.

`tests/provider-matrix.sh` (run by `just ci`) enforces this file: every
`pub trait` in the workspace must appear in the table below, every feature path
must exist, and every selection site must refuse unknown names.

## Abstracted domains

| Domain | Trait | Providers | Selected by | Contract |
|---|---|---|---|---|
| Query engine | `QueryEngine` (`crates/core/src/engine.rs`) | `trino`, `spark` (Spark Connect), `starrocks` (stub) | `engine_from_config` on the `kind` field of `ASTER_ENGINES`; protected Trino additionally needs token-file/CA-file paths and a protected catalog binding | `crates/engines/features/engine-pool.feature`; live `tests/trino-data-policy.py` gate |
| Catalog | `Catalog` (`crates/core/src/catalog.rs`) | `polaris`, `trino` (native metadata), `cube` (semantic layer, read-only), `openmetadata` (metadata source, read-only), `nessie` (stub), `unity` (stub), `mock` (demo) | `catalog_from_config` on the `kind` field of `ASTER_CATALOGS`; `catalog` carries the warehouse/prefix, exact Trino catalog alias (required), Cube base path, or OpenMetadata database filter | `crates/catalogs/features/polaris-catalog.feature`, `crates/catalogs/features/trino-catalog.feature`, `crates/catalogs/features/cube-catalog.feature`, `crates/catalogs/features/openmetadata-catalog.feature` |
| Notebook store | `NotebookStore` (`crates/core/src/notebook.rs`) | `git` | `providers::notebooks` on `ASTER_NOTEBOOK_STORE` | `crates/server/features/notebooks.feature` |
| Notebook ownership | `NotebookOwners` (`crates/core/src/notebook.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE` | `crates/server/features/notebook-ownership.feature` |
| Team Git verifier | `TeamGitVerifier` (`crates/server/src/team_git.rs`) | GitHub App, disposable fake | `ASTER_TEAM_GIT_ENABLED=1` selects the App-backed team target route with PostgreSQL and a server-owned team policy; workspace activation remains off | `crates/server/features/notebook-isolation.feature` |
| Secret store | `SecretStore` (`crates/core/src/secrets.rs`) | `env`, `memory` | `providers::secret_store` on `ASTER_SECRET_STORE` | `crates/core/features/secrets.feature` |
| Identity provider | `IdentityProvider` (`crates/core/src/identity.rs`) | `oidc` (Authentik and Keycloak differ only in configuration), `none` | `providers::identity` on `ASTER_IDP_KIND` | `crates/server/features/sso.feature` (draft; the group/claim mapping is covered by the unit tests in `crates/server/src/identity.rs` and the Keycloak eval in the README) |
| Current identity | `CurrentIdentityProvider` (`crates/core/src/identity.rs`) | Authentik API adapter, mutable fake in tests | `ASTER_AI_AUTHORITY_ENABLED=1`, `ASTER_SHARED_MODEL_AUTHORITY_ENABLED=1`, `ASTER_TEAM_GIT_ENABLED=1`, or `ASTER_TEAM_POLICY_FILE` opts into per-request UUID/group reads; default off. Requires an OIDC signed UUID claim, HTTPS API origin and a dedicated Aster read token. Shared-model authority additionally needs stable admin/editor group UUIDs. Team-policy registration needs no GitHub credentials. Live removal/read-after-write proof is deployment-specific. | `crates/server/src/current_identity.rs` fake API tests; `crates/server/features/authentik-catalog-team.feature` startup test; [isolated real Authentik revocation proof](plans/odcs-ai-catalog-rv.md); `crates/server/features/shared-ai-models.feature` remains partly unbound |
| Engine grants | `Grants` (`crates/core/src/grants.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE` | `crates/core/features/authorization.feature` |
| Audit sink | `AuditSink` (`crates/core/src/audit.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE` | `crates/server/features/audit-persistence.feature` |
| LLM endpoint store | `LlmStore` (`crates/core/src/llm.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE` | `crates/server/features/ai-assist.feature` |
| Skill registry and personal defaults | `SkillStore` (`crates/core/src/skills.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE` | `crates/server/features/ai-skills-models.feature` (draft; API and browser coverage recorded in the implementation plan) |
| Personal model capability | `PersonalModelPolicyStore` (`crates/core/src/personal_model_policy.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE`; default deny | `crates/server/features/ai-skills-models.feature` (draft) |
| Shared model store | `SharedModelStore` (`crates/core/src/shared_models.rs`) | `postgres`, `memory` | `providers::metadata` on `ASTER_METADATA_STORE`; administrator routes additionally require `ASTER_SHARED_MODELS_ENABLED=1`, an approved HTTPS origin and a versioned application key | `crates/server/features/shared-ai-models.feature` |
| Sessions | `SessionRegistry` (`crates/core/src/state.rs`) | `valkey`, `memory` | `providers::state` on `ASTER_STATE_STORE` | `crates/core/features/session-state.feature` |
| OIDC handshakes | `HandshakeStore` (`crates/core/src/state.rs`) | `valkey`, `memory` | `providers::state` on `ASTER_STATE_STORE` | `crates/core/features/session-state.feature` |
| Workspace hosting | `WorkspaceHosting` (`crates/core/src/workspace.rs`) | GitHub App, scoped fake API in tests | `providers::workspace_hosting`, `ASTER_WORKSPACE_HOSTING=github-app` | `crates/server/features/git-web-workspaces.feature` (draft); exact repository and PR retry integration tests |
| Workspace checkout | `WorkspaceCheckout`, `WorkspaceCheckouts` (`crates/core/src/workspace.rs`) | Git CLI, temporary Git fixture | `providers::workspace_checkouts`, `ASTER_WORKSPACE_CHECKOUTS=git` | `crates/server/features/git-web-workspaces.feature` (draft); explicit draft/commit unit test |
| Workspace recovery | `WorkspaceRecovery` (`crates/core/src/workspace.rs`) | `s3`, object-store memory fixture in tests | `providers::workspace_recovery`; explicit HTTPS endpoint, bucket, region and secret-store credentials; opt-in `ASTER_WORKSPACES_ENABLED=1` | `crates/server/features/git-web-workspaces.feature` (draft); archive and object adapter unit tests |
| Working state | `UserState` (`crates/core/src/state.rs`) | `valkey`, `memory` | `providers::state` on `ASTER_STATE_STORE` | `crates/core/features/working-state.feature` |
| Semantic format | `SemanticFormat` (`crates/core/src/semantic.rs`) | `cube` (semantic layer), `odcs` (data contract) | `semantic::format` on the `target` a render request names | `crates/core/features/semantic-models.feature` |

Store, engine and catalog providers need an implementation plus one arm in
their selection function (`crates/engines/src/lib.rs`,
`crates/catalogs/src/lib.rs`, or `crates/server/src/providers.rs`).

Catalog `secret://KEY` references pass through `catalog_from_config_with_secrets`
before the same central kind registry. The server supplies its selected
`SecretStore`; the controller supplies `EnvSecrets` for its environment-based
configuration. The bound S6 fixture is `crates/server/features/odcs-s6-bound.feature`.
There is no additional secret provider or live-target activation.

Native Trino uses `information_schema` through `/v1/statement`, with `X-Trino-User:
aster` and optional resolved bearer `token`; OAuth `credential` is unsupported.
Each metadata operation shares a 10-second deadline, 4 MiB response budget and
64-request ceiling, plus 10,000 metadata rows. Exceeding a ceiling or receiving
incomplete/error metadata fails the entire observation. Continuations must retain
the configured origin; redirects are disabled. Native table descriptors prove
registration, not row-read authorization. Caller-budgeted AI schema observation
remains unavailable. Separate IDs `tpch` and `tpcds` can select their respective
native aliases at the same Trino endpoint; no benchmark table list is hard-coded.

## Deliberately not abstracted

Each row names what would force the abstraction, so the decision is on record
instead of implicit.

| Domain | Today | Abstract when |
|---|---|---|
| Metadata DB internals for the controller | the controller issues its two statements directly (`crates/controller/src/main.rs`) | a second metadata database must be supported |
| Clock | `aster-core` ports already take `now`; the server layer calls `SystemTime::now()` | a server-level scenario must freeze time |
| Telemetry | process-wide `tracing` subscriber, chosen in each binary | a second exporter or sink is required |
| Migration runner | idempotent `migrations/*.sql` applied with `sqlx::raw_sql` at boot | the schema starts evolving (move to `sqlx::migrate!`) |
| Notebook text format | one `.aster` format (D8); `to_text`/`from_text` in `crates/core/src/notebook.rs` | a second notebook format must round-trip |
| HTTP client | one client per component (engines, catalogs, server, OIDC) | providers need different transport policy (proxies, per-host limits) |
| Role parsing | three call sites (core serde, server `parse_role`, OIDC mapping) | a role is added — collapse into one `FromStr` first |
| Non-secret configuration | environment via `AppConfig::from_env` | values must come from a file or an API |


## Notebook conversation memory

`ConversationStore` owns private per-user/per-notebook transcripts. Each scope
(notebook or notebook+cell) holds zero or more sessions keyed by a server-owned
session id; listing is newest first. Pending MCP turns use atomic revision
transitions in the same store, including confirmation and interrupted-operation
state. Exact-session deletion checks owner, scope and revision atomically and
refuses pending tool turns. PostgreSQL removes the legacy source row in the
same transaction, serialized with bootstrap so history cannot be reimported.
Implementations:
`InMemoryConversations` and `PgConversations`, selected by
`providers::conversations`. The default `metadata` reuses the metadata provider's
store/pool; `postgres` uses `ASTER_CONVERSATION_DATABASE_URL` and installs only
conversation tables; `memory` is disposable. Setting the secondary URL selects
PostgreSQL unless `ASTER_CONVERSATION_STORE` explicitly overrides it.
Unknown providers and missing dedicated URLs fail startup. PostgreSQL installs
`conversation_sessions` and backfills the earlier single-conversation rows as
their first session (phrase from the first user message, original timestamp).

Contract: `crates/server/features/notebook-sessions.feature` and the browser
draft `notebook-cells-ui.feature`.

## Notebook session exchange

`ExchangeStore` owns the notebook session's ephemeral view of cell material: one
summary per notebook and each cell's last recorded result. Implementations:
`InMemoryExchanges` and `PgExchanges`, selected by `providers::exchanges`.
Setting `ASTER_EXCHANGE_DATABASE_URL` selects `postgres`, which installs only
exchange tables and never touches the notebook documents; `ASTER_EXCHANGE_STORE`
overrides that selection explicitly, and `memory` is disposable and is the
default, so an unset URL never writes exchange rows into the metadata store.
Unknown providers and missing dedicated URLs fail startup. At most 100 rows are
kept per cell and a summary is limited to 8 KiB.

Contract: `crates/server/features/notebook-session-exchange.feature`.
Results and summaries are bookkeeping, not documents: clearing the store loses
them and never touches a notebook or a stored conversation.

`McpStore`: metadata provider (`memory` → `InMemoryMcp`, `postgres` → `PgMcp`), selected in `providers::metadata`; encrypted registrations and revisioned access metadata. Contract: `crates/server/features/ai-mcp-connections.feature` (unbound).

`McpTransport`: `http` → `HttpMcp`, selected in `providers::mcp_transport`; tests use disposable loopback HTTP fixtures. Contract: `crates/server/features/ai-mcp-connections.feature` (unbound).

`McpCallGuard`: in-process current-identity/revision policy (`CurrentCall`); no configurable external provider. Checked immediately before HTTP tool dispatch.

Notebook Git history uses the existing `NotebookStore` port: bounded revision listing and exact revision reads. Restore reuses conditional saves and owner revision updates; no separate checkpoint provider is introduced.
