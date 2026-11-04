# Notebook sessions, notebook engine and cell kinds — IP

Worktree: `apollo:/home/erik/Documents/erik/aster/worktrees/notebook-tabs-sessions-20261029`
Branch: `feat/notebook-tabs-sessions` from `2f2304a`.

Base contract: `docs/plans/notebook-tabs-sessions-pl.md` and
`crates/server/features/notebook-sessions.feature`.

## Slices

### S1 — notebook document owns one engine and cell magic

Ownership: `crates/core/src/notebook.rs`, `crates/server/src/gitstore.rs`.

- Add `Notebook.engine`, serialized as `# engine: <id>` and parsed before cells.
- Keep legacy cell engines readable; `effective_engine()` prefers the notebook.
- Add `CellKind::{Sql, Markdown}` and `Cell::source()` stripping `%md`/`%sql`.
- Reject whitespace in a notebook engine id on save.

Seams: `Notebook::to_text`/`from_text` round-trip; `Cell::kind`/`source`.

Verification: core notebook tests, gitstore tests, `just notebook-validation`.

### S2 — one server-selected engine per run

Ownership: `crates/server/src/lib.rs` (`execute_query`, `run_attempt`), `api.rs`.

- Resolve the notebook engine after the admitted notebook read.
- Refuse a request that names a different engine.
- Refuse a run for a Markdown cell before any engine call.
- Keep the audit engine equal to the engine actually routed.

Seams: `RunQuery` REST/RPC with notebook+cell; audit event; stub engine calls.

Verification: `notebook-sessions.feature` engine and Markdown scenarios.

### S3 — server-owned conversation sessions

Ownership: `crates/core/src/conversation.rs`,
`crates/server/src/{conversations,api}.rs`, `migrations/0011_conversation_sessions.sql`,
`proto/aster.proto`.

- One row per subject + conversation scope + session id; existing scope rows are
  backfilled as their first session.
- Session phrase = first user message, whitespace collapsed, 60 chars; date =
  creation time in epoch milliseconds.
- `ListConversations`, `NewConversation`; `GetConversation` and `SendMessage`
  accept an optional session id.
- Latest = greatest `updated_at`; a session that receives a new turn becomes
  latest.

Seams: store trait (memory and PostgreSQL), Connect JSON handlers, transcript
capacity and revision checks.

Verification: core conversation tests, bound feature, PostgreSQL migration check.

### S4 — session tabs and exchange controls in the browser

Ownership: `crates/server/src/web.rs`, `crates/server/assets/app.js`,
`crates/server/assets/app.css`.

- Notebook chat and each cell chat render selectable session tabs with phrase and
  date; newest selected initially; "+ New session" starts one.
- Notebook chat fetches a chosen cell's SQL/result into its context.
- Cell chat fetches the notebook summary and cell index into its context.
- Notebook replies offer insert into the current cell or a new cell; cell replies
  keep replace-this-cell and add new-cell.

Verification: browser script (`just conversation-browser`, extended) plus the
server contract.

### S5 — engine toolbar, insertion rows, Markdown and Mermaid

Ownership: `web.rs`, `assets/app.{js,css}`, `assets/vendor/*`, `web::asset`.

- Single engine selector in the notebook toolbar; per-cell selectors removed.
- One `+ Cell` inserter below every cell and one `+ Add cell` below the cell
  list; cell type comes from the `%md` magic, not a creation choice.
- `%md` cells render sanitized Markdown; edit/preview toggle; no run/out.
- Mermaid fenced blocks render as diagrams with strict security.
- Pinned marked, DOMPurify and Mermaid are embedded in the binary and served
  from `/assets/*`; no CDN at runtime.

Verification: browser script checks DOM, no script execution, Mermaid SVG, and
engine selector placement; HTML injection fixture.

### S6 — docs and full gate

- Update `docs/ai.md`, `docs/deploy.md`/`docs/api-and-tests.md` where the route
  surface changed, and `docs/provider-matrix.md` if the conversation store
  contract description needs the session scope.
- Run `just ci` on Apollo; run Compose conversation checks when PostgreSQL is
  involved.

## Test plan

| Layer | Check | Failure signal |
|---|---|---|
| Format | core notebook round-trip and magic tests | engine/header or `%md` lost |
| Server | bound feature scenarios | latest session, isolation, engine refusal, Markdown refusal |
| Store | in-memory session ordering/phrase; PG migration backfill | old history disappears or phrase empty |
| Browser | tabs, insertion, Markdown/Mermaid, fetch buttons, current-cell insert | missing/inactive tab, executable HTML, wrong cell |
| Full | `just ci` | fmt/clippy/tests/features/repo contract |

## Rollback

- Code rollback is the worktree branch; no standing deployment is touched.
- Migration is additive: `conversation_sessions` can be dropped without touching
  `notebook_conversations`. Deploy the old binary and the old table still serves
  history.
- Notebook engine header is additive; older readers ignore unknown header lines
  only if they parse before cells (they do not). Roll back before saving new
  documents if an older binary must read the checkout; the new field is optional
  and old files remain valid.

## Risks

- **Migration phrase SQL**: `regexp_replace` on the first message is best-effort;
  the in-memory and PostgreSQL paths must agree on the 60-char phrase.
- **Mermaid asset size**: 2.5 MB embedded; served with immutable caching.
- **Sanitization**: Markdown and Mermaid output are untrusted; DOMPurify and
  Mermaid `securityLevel: strict` are mandatory.
- **Session sprawl**: list is scope-bounded; no automatic deletion in this slice.
