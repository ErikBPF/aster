# Quality audit — SOLID / KISS / DRY (2026-09-15)

Reviewed the workspace against SOLID, KISS and DRY with cited evidence, then
fixed what was unsafe or cheap. Everything below is either **fixed** (with the
test or command that proves it) or **deferred** with the reason. Rules that came
out of this audit are enforced in `CONTRIBUTING.md` and `AGENTS.md`.

## Fixed

| Finding | Why it mattered | Fix | Evidence |
|---|---|---|---|
| `dev_login` built cookies from unvalidated query parameters and used `.parse().expect(...)` | CR/LF in `subject` panicked the worker — remote DoS on a public route | 404 unless the dev seam is active, charset/length validation, `Err` → 400 | `cookie_values_reject_header_smuggling`; compose smoke: 303 for `alice`, 400 for a smuggled value |
| `#[derive(Debug)]` on `LlmConfig` / `OidcConfig` | printed `api_key` and `client_secret` into any log line | hand-written `Debug` printing `<redacted>` | `the_api_key_never_serializes` plus the redacting `Debug` impls |
| `git`/`std::fs` called from `async fn` | every notebook save or list blocked a tokio worker | sync internals + `spawn_blocking` | `saves_and_reads_back_on_session_branch` (async API, blocking work off-thread) |
| `EngineHealth` / `CatalogHealth` duplicated, with two identical mappers in the controller | one concept, three places to edit | single `Health` with `as_str()`/`Display` | `health_labels_are_stable` |
| `ApiError` returned the raw error text for 5xx | leaked git stderr and upstream internals to clients | detail logged, `"upstream dependency failed"` returned | compose smoke: `POST /api/query` → `{"error":"upstream dependency failed"}` |
| Server and controller containers ran the server binary | compose `command` is appended to an exec-form `ENTRYPOINT`; the controller never ran | `entrypoint:` in compose, `command:` in the chart | compose smoke: controller log shows `reconcile pass complete` and `engines`/`catalogs` rows |
| Image lacked `git`; container died on SIGTERM | notebook store shells out to `git`; pods get SIGTERM | `git` + `ca-certificates` in the runtime image; `with_graceful_shutdown` | compose smoke: notebook commit written and read back; restart stopped the container cleanly |

## Deferred (recorded, not silently dropped)

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

## Adding a second plugin (the risk map that matters)

| To add a… | Touch |
|---|---|
| engine | one type + match arm in `crates/engines/src/lib.rs`; nothing in `core`, `server` or `controller` call sites |
| catalog | one type + match arm in `crates/catalogs/src/lib.rs` |
| notebook store / audit sink / grants / LLM store | `build_state` in `crates/server/src/main.rs` and `AppState` — the remaining known seam |
| role | `core/auth.rs`, `server` role parsing, `server/oidc.rs` mapping, TUI default — the worst seam, deferred above |

## Verification

`just ci` (format, clippy `-D warnings`, tests, `features`) plus `just chart-lint`
and the compose stack: build, up, notebook commit, audit row, restart persistence,
controller reconcile. Evidence for the live run is recorded in the platform
one-pager, not duplicated here.
