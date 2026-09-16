# Unix philosophy, shared user/session state, and gRPC-native transport

Status: proposal for decision. Nothing here is implemented. Grounded 2026-09-16
against `/home/developer/projects/aster` and the platform ecosystem.

The goal is **the Unix philosophy itself, not CLI compliance**. Source: ESR,
*The Art of Unix Programming*, ch. 1 "Basics of the Unix Philosophy" (the 17
rules, read at https://cscie2x.dce.harvard.edu/hw/ch01s06.html), plus McIlroy's
formulation and Pike's rules in the same chapter. Rules are quoted as named
there; the mapping below is ours.

## 1. The 17 rules mapped to aster

| Rule | What it means here | Current state |
|---|---|---|
| Modularity — simple parts, clean interfaces | `core` defines ports (`QueryEngine`, `Catalog`, `NotebookStore`, `LlmStore`, `AuditSink`, `Grants`); adapters implement them | holds |
| Clarity over cleverness | boring handlers, explicit validation, no macro magic | holds; the hand-rolled `escape()` is deliberately dumb |
| Composition — connect programs | server ↔ controller ↔ TUI ↔ CLI over stable interfaces | **weak**: the only composition surface is the browser UI and HTTP handlers |
| Separation — policy from mechanism | policy (roles, grants, engine choice) in `core`/config; mechanism (Trino, Polaris, git, Postgres, Valkey) in adapters | partly violated: role parsing and cookie policy live in `web.rs`/`main.rs` |
| Simplicity | no CRD, no service mesh, no service per trait | holds |
| Parsimony — big program only when nothing else does | one `aster-server` process hosts web + API + OIDC + git + LLM proxy | **violated in spirit**: five jobs in one binary because it was convenient |
| Transparency | tracing, audit trail, `/healthz`, gRPC reflection | audit + tracing hold; the binary boundary is opaque |
| Robustness | graceful shutdown, fail closed when OIDC is configured but unreachable, bounded retries | holds after the SIGTERM fix |
| Representation — fold knowledge into data | ODCS contracts, text notebook format, engine pool config, proto schemas | mostly holds; roles/actions are still code |
| Least surprise | consistent env names, predictable REST shapes | holds |
| Silence — say nothing when nothing surprising | `tracing` writes diagnostics to stderr; the product is the response | holds for the server, unproven for tools (no tools yet) |
| Repair — fail noisily and early | reject unknown engine kinds at startup, `DATABASE_URL` required by the controller, 400 instead of panic | holds after the `dev_login` fix |
| Economy — conserve programmer time | reuse `openidconnect`, `sqlx`, devenv and chart patterns instead of hand-rolling | holds |
| Generation — write programs to write programs | proto codegen, helm templates, SQL migrations, generated charts | holds for YAML/SQL; HTML/JS is hand-written on purpose |
| Optimization — prototype before polishing | thin vertical first; no cache until measured | holds so far |
| Diversity — distrust "one true way" | engine- and catalog-agnostic traits are exactly this | holds |
| Extensibility — design for the future | plugin traits, `#[non_exhaustive]` enums, versioned interfaces | partly: interfaces are not versioned yet |

Two consequences worth stating plainly:

1. The rule that aster most clearly breaks is **Parsimony/Modularity**: one
   process performs web rendering, REST serving, OIDC, git storage and LLM
   proxying. Splitting it is the honest fix, but only where the parts have a real
   interface (see S14).
2. **Composition** is the rule aster gains most from. Today nothing can talk to
   aster except a browser or `curl`. The programmatic interface (gRPC) and a
   composable client are what turn aster into something other tools can chain.

## 2. Valkey as the shared user and session state plane

The purpose is not caching for its own sake: it is so that **user and session
state is global across every container of the app**. Today:

| State | Today | Problem |
|---|---|---|
| Session | self-contained HMAC cookie (`aster_session`) | cannot revoke, cannot list active sessions, cannot share anything about the user across containers beyond the cookie itself |
| OIDC handshake (state, nonce, PKCE verifier) | sealed cookie | fine, but one more thing that must be shared and short-lived |
| Working state (open notebook, selected engine, unsaved cell drafts, cursor) | browser memory only | lost on reload, invisible to TUI/CLI, impossible to resume on another pod |
| Grants, audit, LLM config | Postgres | durable, correct — stays |
| Notebook content | git | source of truth — stays |

Proposed: **Valkey is the shared state plane** for everything that is
per-user, per-session and ephemeral, so any container (server replica, TUI
gateway, CLI) reads and writes the same state.

| Key | Type | TTL | Contents |
|---|---|---|---|
| `aster:v1:session:<sid>` | hash | 8h sliding | subject, roles, created, last_seen, user agent |
| `aster:v1:oidc:<state>` | string | 5 min, single use | verifier, nonce, return path |
| `aster:v1:user:<subject>:state` | hash | 24h | open notebook, selected engine, cell drafts, cursor |
| `aster:v1:rl:<subject>:<bucket>` | counter | 1 min | rate limit for `/api/query` and `/api/ai` |
| `aster:v1:catalog:<id>:<ns>` | string (JSON) | 5 min + jitter | catalog metadata (secondary use, still derived data) |
| `aster:v1:health:<kind>:<id>` | string | 2× reconcile | health mirror written by the controller |

Consequences to accept:

- The cookie becomes an **opaque session id**; the server looks it up in Valkey.
  Valkey is then on the auth path, so availability matters: the documented
  behaviour is **fail closed** (no session lookup → sign in again), with the
  dev seam unaffected.
- Postgres keeps durable, queryable, auditable records (grants, audit, LLM
  configs, engine/catalog registry). Valkey never becomes the only copy of
  anything that must survive.
- Git remains the source of truth for notebook content; the Valkey working state
  is a draft buffer that a save promotes into a commit.
- Deployment: a **dedicated Valkey for aster** (D18) — its own deployment in the
  `aster` namespace from the official `valkey-io/valkey-helm` chart, auth via
  `ExternalSecret` from `lab/aster`, digest-pinned Harbor image, NetworkPolicy
  scoped to aster pods; compose and `k8s/local.yaml` gain a valkey service for
  local runs.

## 3. gRPC-native communication

Internal links move to gRPC; the browser stays HTTP/HTML. `tonic` 0.14.6 +
`tonic-prost` 0.14.6 with `protoc` from `pkgs.protobuf` (exported by devenv),
`tonic-health` for probes, `tonic-reflection` for `grpcurl`.

| Link | Today | Proposed |
|---|---|---|
| TUI → server | HTTP/JSON | gRPC (streaming results, shared session lookup in Valkey) |
| controller → server | via Postgres tables | gRPC health/state reporting; Postgres stays the durable record |
| `aster-ctl` → server | does not exist | gRPC client |
| browser → server | HTTP/HTML | unchanged (browsers do not speak gRPC) |
| server → Trino/Polaris | their HTTP protocols | unchanged — adapters translate at the edge |

Flag honestly: **no repo in this ecosystem speaks gRPC today** (no `.proto`, no
tonic/prost, no buf; gRPC appears only as OTLP to Jaeger). Adopting it adds a
protobuf toolchain, a reflection convention, gRPC health probes and a versioning
policy — a new operational standard, justified by the requirement and by
streaming/stateful clients, but it should be an accepted decision.

## 4. Slices

- **S12 — Valkey state plane.** `SessionStore` trait in core with Valkey and
  in-memory adapters; opaque session id cookie; OIDC handshake in Valkey; the
  old HMAC cookie path deleted or kept behind a flag. Contract:
  `crates/server/features/session-state.feature`.
- **S13 — User working state.** `aster:v1:user:<subject>:state` read/write API,
  used by the web page and TUI so a session resumes on any container. Contract:
  `crates/server/features/user-state.feature`.
- **S14 — proto and gRPC server.** `proto/aster.proto` (`NotebookService`,
  `QueryService`, `StateService`, `ReconcileService`); tonic served beside the
  axum router (`Routes::into_axum_router`) or on its own port; health and
  reflection wired; TUI migrated first.
- **S15 — controller over gRPC and `aster-ctl`.** Controller reports over
  `ReconcileService`; a composable client binary (stdout = product, JSONL) is
  added where the philosophy needs it — not for CLI-compliance points.
- **S16 — process split (only if S14 justifies it).** Separate the browser/HTML
  role from the API/protobuf role inside `aster-server` if the interface proves
  stable; otherwise keep one process and document why (Parsimony).

## 5. Decisions to record

- **ACCEPTED 2026-09-16** **D17** The Unix philosophy (17 rules, ESR) governs
  design decisions in aster; CLI ergonomics only follow from it.
- **ACCEPTED 2026-09-16** **D18** Valkey is the shared user and session state
  plane across all aster containers; Postgres remains the durable metadata store
  and git the notebook source of truth. A **dedicated Valkey for aster** (its own
  deployment in the `aster` namespace), not a shared instance with key prefixes.
- **ACCEPTED 2026-09-16** **D19** gRPC (tonic) is the native transport between
  aster programs; the browser stays HTTP.
- **ACCEPTED 2026-09-16** **D20** Sessions become opaque ids resolved in Valkey
  (revocable, shared). **Fail closed**: if the store is unavailable, the affected
  request is refused and the user signs in again; there is no HMAC fallback.

Remaining open question: does the working-state buffer need an explicit
"unsaved drafts" retention promise (how long, and who may see them)?
