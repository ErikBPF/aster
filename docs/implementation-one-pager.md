# aster — first testable draft (implementation one-pager)

**Stage / revision:** PL+IP+RV combined / r2 (closing record for S0–S14)
**Status:** the thin vertical and every queued slice are implemented, tested and container-validated; nothing is deployed and nothing has been exercised against a live Trino, Polaris or identity provider
**Owner / date:** Erik / 2026-09-16
**Basis:** platform proposal `docs/proposals/2026-09-14-aster-sql-notebook-platform.md`, `docs/unix-grpc-valkey-plan.md` (D17–D20 accepted 2026-09-16), `docs/quality-audit.md`. This page grants no execution authority.

## Outcome and why it matters

`aster` is a git-backed SQL notebook: a notebook is a text file committed to git, a cell runs on an engine chosen from a pool, permissions come from the catalog at table level and the engine at row/column level, sign-in is OIDC, and every attempt — including every refusal — is audited. The first testable draft exists end to end in Rust: web UI, JSON API, TUI client, metadata controller, engine/catalog plugins, LLM assist, data contracts, a shared state plane and a generated multiprotocol RPC surface.

## What is implemented

Six crates in one workspace (`crates/core`, `engines`, `catalogs`, `server`, `controller`, `tui`), plus `proto/aster.proto` as the single endpoint declaration.

| Area | What it does |
|---|---|
| `aster-core` | domain only, no IO: engine/catalog/notebook/store traits, RBAC and grants, audit, session and handshake ports, working state, LLM config, data contracts, `Health` |
| `aster-engines` / `aster-catalogs` | `TrinoEngine` and `PolarisCatalog` against the real APIs; Spark/StarRocks/Nessie/Unity rejected at config parse rather than stubbed into silence |
| `aster-server` | axum: OIDC login, notebook editor, catalog browse, contracts, LLM proxy, session/working-state routes, `/api/*` JSON plus the Connect/gRPC surface |
| `aster-controller` | one reconcile loop: migrations, engine/catalog health into Postgres, audit retention |
| `aster-tui` | ratatui client over the same JSON API, resumes where the subject left off |
| State | Valkey (`redis` 1.7): sessions, single-use OIDC handshakes, per-subject working state; in-memory adapters for dev and tests |
| Surface | `/healthz`, `/api/{engines,catalogs,query,audit,notebooks,llm,ai,contracts,state}` and `aster.v1.Aster/*` over Connect JSON, gRPC and gRPC-Web from one registration |
| Config | env-only, no `.env` file: `ASTER_BIND`, `DATABASE_URL`, `ASTER_STATE_URL`, `ASTER_SECRET_STORE`/`ASTER_METADATA_STORE`/`ASTER_STATE_STORE`/`ASTER_NOTEBOOK_STORE` (provider names; see `docs/provider-matrix.md`), `ASTER_ENGINES=id;kind;endpoint[;routing_group]`, `ASTER_CATALOGS=id;kind;endpoint[;catalog]`, `ASTER_GRANTS`, `ASTER_NOTEBOOK_DIR`/`_BRANCH`, `ASTER_CONTRACTS_DIR`, `ASTER_OIDC_*`, `ASTER_*_TTL_SECONDS`, `ASTER_RECONCILE_SECONDS`, `ASTER_AUDIT_RETENTION_DAYS`, TUI `ASTER_SERVER`/`ASTER_SUBJECT`/`ASTER_ROLES` |

Notebook format is deliberate: a text `.aster` file (`# aster notebook v1`, `# title:`, `-- cell <id> [engine=]`) that diffs in git. Save is `git add` + `git commit` on a per-session branch and returns the revision hash.

## Slice status

| Slice | Status | Note |
|---|---|---|
| S0 prerequisite runtime | **not started** | no Trino/Polaris/gateway in the platform (D6) |
| S1 repo + identity scaffolding | **dropped (standalone)** | the GitHub repo exists; the Authentik client, OpenBao `lab/aster` and the Harbor image are out of scope because aster runs only on minikube |
| S2 SSO | implemented | OIDC+PKCE; opaque session id in Valkey; dev seam only while no issuer is configured |
| S3 git notebooks | implemented | branch-per-session, path-traversal guard, co-located unit tests |
| S4 query authz + audit | implemented | one `execute_query` shared by REST and RPC; refusals audited with `ok:false` |
| S5 deployment | manifests written, not synced | digest placeholder `sha256:000…0`; chart carries the same placeholder |
| S6 TUI | implemented | resumes the subject's working state; no live-server run yet |
| S7 web UI | implemented | hand-written HTML/JS by choice; catalog + contracts + AI pages included |
| S8 catalog tab | implemented | namespace → table → column, catalog errors rendered inline |
| S9 engine pool routing | implemented | `X-Trino-Routing-Group` sent only when a group is configured |
| S10 AI assist | partial | registration + completion proxy shipped; notebook evaluation metric not built |
| S11 data contracts | partial | ODCS-shaped JSON only; YAML and Cube unbuilt (D12) |
| S12 state plane | implemented | Valkey adapter, single-use handshake, fail-closed on store error |
| S13 working state | implemented | `GET|PUT /api/state`, web resume link, TUI resume |
| S14 RPC surface | implemented | one proto → Connect JSON + gRPC + gRPC-Web on the existing router |
| Provider matrix | implemented | `SecretStore` port added; every store domain selected in `crates/server/src/providers.rs`; `docs/provider-matrix.md` checked by `tests/provider-matrix.sh` in `just ci` |

## Evidence and limits

| Claim | Evidence | Limit |
|---|---|---|
| Gate is green | `just ci` on `cache-host`: fmt, clippy `-D warnings`, tests, `features`, `repo-check`, `providers-check` | one host, one toolchain |
| 42 Rust tests pass | libtest: core 20, server 12 (+1 ignored Valkey test), tui 3, controller 2, engines 3, catalogs 2 | unit and in-process level |
| Behavior files execute | core cucumber **5 features / 21 scenarios / 90 steps**; server harness **4 features / 23 scenarios / 97 steps**; repo contract by script | 12 of 22 files are still `@unautomated` drafts |
| Every external dependency is a provider port | `providers-check`: all ten `pub trait` ports classified in the matrix, every contract path present, every selection site refusing an unknown name; `providers.rs` tests | the domains deliberately not abstracted are listed with their trigger, not hidden |
| Both protocols behave the same | live compose run: Connect `ListEngines` 200, Connect `RunQuery` refusal `permission_denied`, `grpcurl` over h2c returns the same data; REST `POST /api/query` refusal 403 naming the grant | no live engine behind either |
| Refusals are audited | live: two `bob` rows with `ok:false` in Postgres after REST and Connect refusals; `query-authorization.feature` asserts it in-process | engine id `(unrouted)` when no engine resolves |
| Sessions live outside the process | live: a record minted with `valkey-cli` was accepted by the server, an unknown id was refused, `last_seen` rewritten | single Valkey instance, no failover |
| Notebook save is a commit | in-container: `git log` shows `alice: update sales` on `session/alice` | local repo only; remote push gated on D9 |
| Audit survives a restart | Postgres: row listed before and after stopping and starting the server | single node |
| Secrets stay server-side | `api_key` skips serialisation and has a redacting `Debug` (tested); no `Debug` on the OIDC config prints its secret | tokens rest unencrypted (D10) |
| Chart and manifests are valid | `just chart-lint`: 15 resources, 13 valid, 2 skipped (no CRD schema); platform `apps/platform/aster` kubeconform-valid | never applied; zero-digest placeholder |

## Decisions

Settled and implemented: D1 ecosystem, D2 name, D3 routing group, D4 per-user repos (single local repo so far), D5 thin vertical, D8 text notebook format, D13 engine-agnostic, D14 catalog-agnostic, D15 all-Rust and container-first, D16 scaffold, **D17** Unix philosophy as the design rule, **D18** Valkey owns user and session state globally on a dedicated instance, **D19** internal transport is gRPC generated from one proto — connectrpc alone, not tonic plus connectrpc, so there is one type set — **D20** fail closed when the state store is unavailable.

Still open and blocking: **D6** where Trino/Gateway/Polaris run (blocks S0 and the live query path), D7 Postgres placement, D9 GitHub App vs PAT for remote notebook push, D10 LLM token custody, D11 Trino row/column rule source, D12 ODCS/Cube position.

## What continues / what waits

Continues without a human decision: binding the remaining 12 drafts (the harness selects by tag, so removing `@unautomated` is the whole change), the S10 evaluation metric, editor polish, and Cube definition generation. Waits on the human: D6, D9, D10. S1 (Authentik, OpenBao, Harbor) and the S5 Argo sync are out of scope: aster is a standalone experiment that runs on minikube.

## Risks and recovery

**Nothing is deployed.** No cluster resource was applied and no secret was written; rollback is deleting the manifests commit. Host-side artifacts are the `aster-*` images on `cache-host` plus two stale `sqlbook-*` volumes that still need `docker volume rm`.

**Design risk.** The engine and catalog plugins have never talked to a real Trino or Polaris, so protocol-level surprises (auth scopes, vended credentials, pagination) remain unverified, as does the whole SSO path against a real IdP. The thin slice and the D6 split are the mitigation: every component runs with no engine, no catalog and no issuer, so the dev loop never depends on the missing runtime.

**Source location.** The canonical checkout is `/home/developer/projects/aster` on a `main` branch. It is not pushed anywhere yet; an earlier scratch copy under `/tmp` was lost to tmp cleanup and restored, which is why the working copy no longer lives in a temporary directory. Until S1 creates the org repository, the working copy is the only copy.
