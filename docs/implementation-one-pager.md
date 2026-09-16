# aster — first testable draft (implementation one-pager)

**Stage / revision:** PL+IP+RV combined / r1
**Status:** implementation complete for the thin vertical and all queued slices; nothing deployed, nothing exercised against a live Trino, Polaris or Authentik
**Owner / date:** Erik / 2026-09-15
**Basis:** `docs/proposals/2026-09-14-aster-sql-notebook-platform.md`, `docs/behaviors/aster/platform.feature` (platform), commits `47efb44..3ff229d` (aster), `f8f2d9c` (platform-gitops). This page grants no execution authority.

## Outcome and why it matters

`aster` is a git-backed SQL notebook: the notebook is a text file committed to git, a cell runs on a Trino instance chosen from a pool, permissions come from Polaris at table level and Trino at row/column level, SSO is Authentik OIDC, and every query is audited. The first testable draft exists end to end in Rust: web UI, JSON API, TUI client, metadata controller, engine/catalog plugins, LLM assist, data contracts, and Kubernetes manifests.

## What is implemented

Five crates plus a virtual workspace, 24 Rust source files, ~4,270 lines.

| Crate | What it does | Key files |
|---|---|---|
| `aster-core` | domain only, no IO: engine/catalog/notebook traits, RBAC, grants, audit, HMAC sessions, LLM config, data contracts | `engine.rs`, `catalog.rs`, `notebook.rs`, `auth.rs`, `grants.rs`, `audit.rs`, `session.rs`, `llm.rs`, `contract.rs`, `config.rs`, `registry.rs` |
| `aster-engines` | `QueryEngine` implementations; `TrinoEngine` real, `SparkEngine`/`StarRocksEngine` plugin stubs | `lib.rs` (269) |
| `aster-catalogs` | `Catalog` implementations; `PolarisCatalog` real (Iceberg REST), Nessie/Unity stubs | `lib.rs` (247) |
| `aster-server` | axum HTTP: SSO, notebooks, catalog, contracts, LLM proxy, audit; Postgres or in-memory stores | `main.rs` (473), `web.rs` (602), `oidc.rs` (269), `ai.rs` (192), `gitstore.rs` (168), `store.rs` (158), `contracts.rs` (78) |
| `aster-controller` | reconcile loop: probe engine/catalog health into the metadata DB, prune expired audit | `main.rs` (161) |
| `aster-tui` | ratatui client over the same JSON API | `main.rs` (464) |

Behavior surface: `/` notebook list, `/notebooks/{id}` editor, `/catalog[/{id}/{ns}/{table}]`, `/contracts`, `/settings/llm`, `/login` `/callback` `/logout`, `/dev-login` (dev only); API `/healthz`, `/api/engines`, `/api/catalogs`, `POST /api/query`, `/api/audit`, `/api/notebooks[/{id}]`, `/api/catalogs/...`, `/api/llm`, `POST /api/ai`, `/api/contracts`.

Configuration is env-only (no `.env` file): `ASTER_BIND`, `DATABASE_URL`, `ASTER_ENGINES=id;kind;endpoint[;routing_group]`, `ASTER_CATALOGS=id;kind;endpoint[;catalog]`, `ASTER_GRANTS=subject:engine,...`, `ASTER_NOTEBOOK_DIR`/`ASTER_NOTEBOOK_BRANCH`, `ASTER_CONTRACTS_DIR`, `ASTER_SESSION_KEY`, `ASTER_OIDC_*`, `ASTER_RECONCILE_SECONDS`, `ASTER_AUDIT_RETENTION_DAYS`, `ASTER_SERVER`/`ASTER_SUBJECT`/`ASTER_ROLES` (TUI).

Notebook format is a deliberate choice: a text `.aster` file (`# aster notebook v1`, `# title:`, `-- cell <id> [engine=]`) that diffs and merges in git. Save = `git add` + `git commit` on a per-session branch, returning the revision hash.

## Slice status

| Slice | Status | Note |
|---|---|---|
| S0 prerequisite runtime | **not started** | no Trino/Polaris/gateway anywhere in the platform (D6) |
| S1 repo + identity scaffolding | **not started** | GitHub repo, Authentik client, OpenBao `lab/aster`, Harbor image |
| S2 server + SSO | implemented | OIDC+PKCE, HMAC-signed cookie, group→role; dev seam only while unconfigured |
| S3 git notebooks | implemented | branch-per-session, path-traversal guard |
| S4 query authz + audit | implemented | central `authorize` + per-engine grant check |
| S5 deployment | manifests written, not synced | digest placeholder `sha256:000…0` |
| S6 TUI | implemented | compiles, 3 unit tests; never run against a live server |
| S7 web UI | implemented | hand-written HTML/JS, not askama/HTMX/CodeMirror |
| S8 catalog tab | implemented | namespace/table/column browse |
| S9 engine pool routing | implemented | `X-Trino-Routing-Group` per instance |
| S10 AI assist | partial | registration + completion proxy shipped; evaluation metric not built |
| S11 data contracts | partial | JSON only; ODCS YAML and Cube unbuilt |

## Evidence and limits

| Claim | Evidence | Limit |
|---|---|---|
| Compiles and lints clean | `just remote-check`, `remote-clippy -D warnings`, `fmt-remote` on cache-host | toolchain via `nix shell`; cache-host DNS to crates.io was down, so checks ran `--offline` |
| 33 tests pass | `just remote-test`: core 19, engines 1, server 8, controller 2, tui 3 | unit-level; no integration harness |
| 11 behavior contracts exist | `features/*.feature`, `just features-check` = `features OK: 11 file(s)` | `@unautomated`; no Gherkin runner bound |
| Audit survives restart | Postgres 17 container; row present before and after server restart | local container, single node |
| Forged cookies rejected | tampered payload and foreign-key tests in `session.rs` | no key rotation |
| XSS neutralized | notebook titled `<script>alert(1)</script>`: page contains 0 raw tags, 2 escaped | hand-rolled `escape()`; no CSP header yet |
| Unauthorized engine refused | 403 naming the missing grant, header and cookie clients | grants seeded by env var, not yet a UI |
| Controller reconciles | 4 passes against Postgres; `engines`/`catalogs` rows updated | no leader election; `Recreate` single writer |
| Manifests are structurally valid | `kubeconform -strict`: 10 valid, 0 invalid, 2 skipped | never applied; no cluster diff run |
| Token never returned to clients | `api_key` is `skip_serializing`; asserted by test | stored plaintext at rest (D10) |

## Concerns by topic

**Security.** SSO path is written but never exercised against a real IdP — only the discovery-failure path was smoke-tested. The `groups` claim is read from the already-validated ID token payload rather than a mapped claim. `ASTER_SESSION_KEY` has a development default. The dev seam (`/dev-login`, `x-aster-subject`) is fail-safe (it disables itself when `ASTER_OIDC_ISSUER` is set) but must be deleted in S5. LLM tokens rest unencrypted. No CSP, no rate limiting, no CSRF token on the form POST to `/settings/llm`.

**Infrastructure.** The whole runtime is a prerequisite that does not exist (D6): Trino, Trino Gateway, Polaris, and the metadata Postgres have no home-lab deployment. Manifests assume `nfs-slow` RWX storage and a `vault-discovery` ClusterSecretStore. The network policy egress rule is namespace-open until the runtime labels exist.

**Experience.** The web UI is server-rendered HTML with ~60 lines of vanilla JS; the SQL editor is a plain textarea with no highlighting or completion. The TUI has no live-server run yet. Engine selection is a free-text input, not a dropdown fed by the pool.

**Data.** Notebooks are per-session branches of one local git repo — no remote, no push, no per-user isolation (D9). Row/column enforcement is documented as Trino's job, but no Trino access-control rules exist. Contracts are JSON-only; no Cube semantic layer.

**Operations.** No metrics, no tracing export beyond `tracing_subscriber`, no backups for the notebook PVC. Audit retention is a fixed 30-day delete.

## Decisions

| ID | Decision | Status |
|---|---|---|
| D2 | name `aster` | settled (trademark check: Teradata marks lapsed/EU-expired) |
| D4 | per-user git repos, branch per session | partly implemented (single local repo) |
| D5 | thin vertical first | done |
| D8 | custom text-first notebook format | settled by implementation |
| D13/D14 | engine- and catalog-agnostic plugin traits | settled |
| D15/D16 | all-Rust, container-first, scaffold | settled |
| D6 | where Trino/Polaris/Gateway run | **open — blocks S0/S5** |
| D7 | Postgres placement | open |
| D9 | GitHub App vs PAT for push | open |
| D10 | LLM token custody | open |
| D11 | Trino row/column rule source | open |
| D12 | ODCS/Cube role | open |

## What continues / what waits

Continues without blockers: binding the 11 feature files to a runner, the S10 evaluation metric, and UX polish on the editor. Waits on the human: D6 (runtime), D9 (GitHub identity), D10 (token custody). Waits on S1: Harbor image publish, then the S5 digest pin and the first Argo sync.

## Risks and recovery

**Source location.** The `aster` repository lives at `/tmp/opencode/aster` — volatile, not pushed anywhere. Until S1 creates the org repo, a tmp wipe loses 7 commits. Recovery: push to the org repo or copy out; the platform proposal and one-pager record the design independently.

**Zero-risk rollout.** Nothing is deployed, no cluster resource was applied, no secret was written. Rollback is deletion of the manifests commit. The only host-side artifacts are images `aster-dev:latest`/`aster-server:local` on cache-host and the two stale `sqlbook-*` volumes that still need `docker volume rm`.

**Design risk.** Engine and catalog traits are proven only against stubs; the Polaris and Trino paths have never talked to a real server, so protocol-level surprises (auth scopes, vended credentials, pagination) remain unverified.
