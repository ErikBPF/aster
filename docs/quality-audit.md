# Quality audit — SOLID / KISS / DRY (2026-09-15)

Reviewed the workspace against SOLID, KISS and DRY with cited evidence, then
fixed what was unsafe or cheap. Everything below is either **fixed** (with the
test or command that proves it) or **deferred** with the reason. Rules that came
out of this audit are enforced in `CONTRIBUTING.md` and `AGENTS.md`.

## First audit — workspace, containers and chart (2026-09-15)

### Fixed

| Finding | Why it mattered | Fix | Evidence |
|---|---|---|---|
| `dev_login` built cookies from unvalidated query parameters and used `.parse().expect(...)` | CR/LF in `subject` panicked the worker — remote DoS on a public route | 404 unless the dev seam is active, charset/length validation, `Err` → 400 | `cookie_values_reject_header_smuggling`; compose smoke: 303 for `alice`, 400 for a smuggled value |
| `#[derive(Debug)]` on `LlmConfig` / `OidcConfig` | printed `api_key` and `client_secret` into any log line | hand-written `Debug` printing `<redacted>` | `the_api_key_never_serializes` plus the redacting `Debug` impls |
| `git`/`std::fs` called from `async fn` | every notebook save or list blocked a tokio worker | sync internals + `spawn_blocking` | `saves_and_reads_back_on_session_branch` (async API, blocking work off-thread) |
| `EngineHealth` / `CatalogHealth` duplicated, with two identical mappers in the controller | one concept, three places to edit | single `Health` with `as_str()`/`Display` | `health_labels_are_stable` |
| `ApiError` returned the raw error text for 5xx | leaked git stderr and upstream internals to clients | detail logged, `"upstream dependency failed"` returned | compose smoke: `POST /api/query` → `{"error":"upstream dependency failed"}` |
| Server and controller containers ran the server binary | compose `command` is appended to an exec-form `ENTRYPOINT`; the controller never ran | `entrypoint:` in compose, `command:` in the chart | compose smoke: controller log shows `reconcile pass complete` and `engines`/`catalogs` rows |
| Image lacked `git`; container died on SIGTERM | notebook store shells out to `git`; pods get SIGTERM | `git` + `ca-certificates` in the runtime image; `with_graceful_shutdown` | compose smoke: notebook commit written and read back; restart stopped the container cleanly |

### Deferred (recorded, not silently dropped)

| Finding | Reason to defer | Where it goes |
|---|---|---|
| `PgStore` implements `Grants` + `AuditSink` + `LlmStore` (three reasons to change) | three newtypes over one pool is churn without a second backend; behavior is already correct | when a second metadata store appears |
| `web.rs` mixes rendering, cookies and inline CSS/JS (602 lines) | splitting it changes no behavior; the page count is still small | when a third dynamic page lands, per the ponytail note already in the file |
| 13 handlers repeat `principal(...)?; authorize(...)?` and 4 repeat catalog lookup | an `AuthedUser` extractor plus `State::engine/catalog` helpers is the right fix, but it touches every handler in one diff | S2 follow-up, before more routes are added |
| `InMemoryAudit` is unbounded while Postgres caps at 500 | dev-only path; divergence is documented | with the same-bounds change as the extractor work |
| Public enums are not all `#[non_exhaustive]` (`Health` is) | additive, mostly mechanical | next core touch |
| `parse_engines` / `parse_catalogs` share one splitter implementation | generic extraction saves ~10 lines | next config change |
| Role parsing lives in three places (core serde, `parse_role`, OIDC mapping) | one `FromStr` for `Role` in core is the fix | with the extractor work |
| Chart had no `values.schema.json` | **done** — `charts/aster/values.schema.json` now pins the Harbor path and the digest shape | — |

## Second audit — shared state, RPC and sessions (2026-09-16)

Scope: the Valkey state plane (S12), working state (S13) and the generated RPC
surface (S14) — `crates/core/src/state.rs`, `crates/server/src/state.rs`,
`crates/server/src/api.rs`, `crates/server/src/lib.rs`, `crates/server/build.rs`
and the harness in `crates/server/tests/contracts.rs`.

### Fixed in this round (commit `075d3ba`)

| Finding | Why it mattered | Fix | Evidence |
|---|---|---|---|
| A refused query was never audited, though the contract promised every attempt is | an access-denied probe left no trace — the trail an operator needs most | `execute_query` runs the role check, engine resolution, grant check, execution and one audit row; refusals land with `ok: false` | `query-authorization.feature` now asserts an audit event for a refused caller; 23 scenarios green |
| REST and RPC each re-implemented engine resolution, grant check, timing and `AuditEvent` construction | three legs drifting apart; the divergence above was a symptom | one `crate::execute_query` used by the REST handler and the RPC leg | `just ci` on cache-host; RPC parity asserted in `rpc-surface.feature` |
| `/api/engines`, `/api/catalogs`, catalog namespaces and tables answered anonymous callers while their RPC twins required a principal | engine and catalog topology leaked, and `api-surface.feature` passed an identity the handler ignored | all four require `principal` + `ReadNotebook` | new scenario "An unidentified caller cannot read the inventory" |
| A Connect failure mapped `Unauthorized` to 401 | a signed-in caller without an engine grant looked unauthenticated, while REST called the same state 403 | `Unauthorized` → `permission_denied`; only `caller` answers 401, documented on `connect_error` | `the call fails as permission denied naming the missing grant` |
| `env_seconds` accepted negative or zero TTLs | `-1` cast to a multi-century Valkey expiry | non-positive values fall back to the default | `env_seconds` filter; server TTLs come from compose/chart values |
| An expired session left its subject-index entry behind | the subject's set grew for every session it ever created and `list` paid a `GET` per stale member | expired `get` removes the index entry as well as the key | added to the `#[ignore]` Valkey test (needs a live Valkey) |
| A result row that failed to encode became `[]` | a fabricated empty row silently changed the answer | log and fail the call as internal | `api.rs` row loop |
| Session cookie carried no `Secure`; the IdP's error text was echoed to the client | sniffable session id before TLS termination; reflected provider text | `Secure` once SSO is configured (never on the plain-HTTP dev seam); provider error logged, generic refusal returned | `web.rs` login callback |

### Checked and refuted

| Suspicion | Verdict |
|---|---|
| Session and handshake expiry disagree at the deadline | not a defect: both are valid only while `stored + ttl > now`, so both die at the deadline |
| Secret leakage through logs or serialisation | none found: `api_key` skips serialisation and has a redacting `Debug` with a test, `OidcConfig` redacts, the Valkey URL password is never logged |

### Deferred (recorded, not silently dropped)

| Finding | Reason to defer | Where it goes |
|---|---|---|
| A session-store outage bounces browsers to `/login` instead of an error page | the API paths already fail closed with 403; the page path only loses the distinction, and the store error is logged | when the web layer gets an error page (S7 follow-up) |
| The session cookie's `Max-Age` is fixed at login while `get` refreshes the idle deadline | an active browser is still dropped at the original ceiling — a documented ceiling, not a leak | re-issue the cookie on refresh if a longer ceiling is wanted |
| `AppState` exposes all fields `pub` so tests can build it | acceptable while the harness lives outside the crate boundary; a test-only builder is the alternative | with the next store added to `build_state` |
| `to_message` / `from_message` clone every field; `contracts` is deep-cloned per request | small, measured cost; ownership moves are a mechanical cleanup | next touch of `api.rs` |
| Public enums are not all `#[non_exhaustive]` (`Health`, `SessionRecord` are) | additive and mostly mechanical | next core touch |
| `connectrpc` codegen through `build.rs` shells out to `protoc` | devenv and both images provide it; bake a descriptor only if CI ever builds without Nix | if the image build leaves devenv |
| `group`/`user_agent` shape of a handshake payload is positional (`verifier:nonce`) | two fields, one producer and one consumer | if a third field is added |

## Adding a second plugin (the risk map that matters)

| To add a… | Touch |
|---|---|
| engine | one type + match arm in `crates/engines/src/lib.rs`; nothing in `core`, `server` or `controller` call sites |
| catalog | one type + match arm in `crates/catalogs/src/lib.rs` |
| notebook store / audit sink / grants / LLM store / user state | `build_state` in `crates/server/src/lib.rs` and `AppState` — the remaining known seam |
| role | `core/auth.rs`, `server` role parsing, `server/oidc.rs` mapping, TUI default — the worst seam, deferred above |
| RPC endpoint | `proto/aster.proto` plus one method in `crates/server/src/api.rs`; the route, client and JSON encoding are generated |

## Verification

`just ci` (format, clippy `-D warnings`, tests, features, repo contracts) on
cache-host, `just chart-lint`, and the compose stack: build, up, notebook commit,
audit row, restart persistence, controller reconcile. Live evidence is recorded
in the platform one-pager, not duplicated here.
