# AI assistance

**S8 inventory boundary:** with a compiled bundle configured, unselected notebook
assistance omits physical metadata rather than bypassing schema-owner/object
admission. Select an admitted contract for metadata context. Old history carrying
only catalog-wide dependencies cannot prove object admission and refuses replay
in this mode; start a fresh conversation. Selected contract meaning remains
artifact context, not evidence that arbitrary SQL is authorized. The no-bundle
development path retains its historical behavior and is not r20 policy enforcement.

**Implemented:** A signed-in user registers named personal
OpenAI-compatible helpers. Each registration contains a base URL, model and
token. The server stores the token and proxies completion requests; list and
settings responses expose the name, safe URL and model, not the token. An
accepted helper URL is absolute HTTP(S) with no username, password, query or
fragment. Previously stored invalid URLs appear blank in API/settings and
cannot be used for generation until re-registered. A notebook can select one
of that user's helpers. The legacy AI endpoint asks for a reviewable draft and
can include current cell SQL and an explicitly selected authorized contract. Users
should review generated SQL before running it.

The web settings page at `/settings/llm` can add or replace registrations.
The legacy API uses `PUT /api/llm/<id>`, `GET /api/llm` and
`DELETE /api/llm/<id>`. `POST /api/ai` accepts a helper name; if omitted,
the first registered helper is chosen. An explicitly blank name is refused.
An editor may request a completion;
viewers cannot spend one. Registration is private to the authenticated
subject. These `/api/*` routes coexist with the newer
[Connect service](../proto/aster.proto).

The selected personal helper is saved separately for each authenticated user
and notebook, outside the Git file. `GET /api/notebooks/<id>/helper` and
`PUT /api/notebooks/<id>/helper`, with matching `GetNotebookHelper` and
`PutNotebookHelper` Connect calls expose that choice. A new notebook has no
implicit helper selection. A deleted helper remains visibly unavailable until
the user chooses a registered one; generation does not silently substitute
another helper. Valkey retains the choice across server process restarts while
its own persisted data remains available; the disposable memory provider is
volatile.

## Notebook conversation

**Implemented:** Open **Chat** in a notebook to expand a right sidebar, send
follow-up questions and reload the private history. A cell's **AI** action opens
a panel beside that cell: a separate conversation grounded on the cell's SQL and
its last run output, whose fenced SQL block offers **Replace this cell**. The two
histories are distinct — a cell conversation cannot read the notebook
conversation or another cell's, and the notebook conversation cannot read any
cell conversation — and each is scoped by authenticated subject. Selecting
another helper sends that notebook's history to the newly selected helper. Model
text is rendered inert; a fenced SQL block gets one explicit **Insert SQL as new
cell** button per reply (its last block) and never runs by itself, and
Ctrl/Cmd+Enter runs the focused cell or sends the focused chat message. The cell
panel scrolls rather than stretching the cell, and navigating away from a
notebook no longer asks for confirmation.

The notebook session also receives a bounded index of the notebook's cells — each
cell's id and SQL, at most 20 cells or 8 KiB — as untrusted per-turn context, so a
question about "the first cell" can see it. The index is rebuilt each turn, never
saved, and only the notebook session receives it; a cell conversation still sees
its own cell alone. S1 uses the admitted notebook snapshot rather than reloading
a bare ID from the global store. HTTPS capture checks same-ID notebooks across
two teams, two sessions and a personal workspace.

Beyond that reference material the notebook session reaches cell material through
five explicit exchange operations, each scoped to the requesting principal's
notebook and none
reading a cell conversation: `FetchQuery` (one cell's SQL and the content
revision it was read at), `FetchResult` (one cell's last recorded result),
`FetchSummary` (the notebook summary and the cell index), `SendSummary`
(replace the summary, 8 KiB) and `UpdateQuery` (replace one cell's SQL against
the expected content revision, refused as a conflict when stale). A cell run
through `/api/query` or `RunQuery` records its result when the request names the
notebook and cell, keeping at most 100 rows per cell, and a run trims a trailing
statement terminator so a cell ending in `;` executes. Exchange material is
bookkeeping, stored apart from the Git notebook document and the conversations:
`ASTER_EXCHANGE_DATABASE_URL` selects `postgres` and installs only exchange
tables, `ASTER_EXCHANGE_STORE` overrides that selection, and disposable `memory`
is the default.

`GetConversation` and `SendMessage` are Connect RPCs. A completed user/assistant
pair is saved atomically with a revision check. If a model or storage call
fails, the composer keeps the draft; a stale revision returns a conflict and
the user must explicitly resend after seeing current history. The server
owns the local conversation identifier. Each upstream attempt, including for
OpenCode Go helpers, uses a fresh identifier and the full admitted message history.
Prompts and attached SQL are limited to 8 KiB each, replies to 32 KiB, and
history to 200 messages or 256 KiB of content and dependency identities. Overflow is refused without truncating
saved messages.

Conversation storage defaults to the metadata store. With PostgreSQL metadata,
history survives server restart. To delegate storage, provide a separate
`ASTER_CONVERSATION_DATABASE_URL` through the deployment's secret/environment
mechanism; it selects `postgres` unless `ASTER_CONVERSATION_STORE` explicitly
selects `metadata` or disposable `memory`. The dedicated database creates
only conversation tables. Changing the database does not copy old history.

Without an explicit contract selection, the existing global and per-catalog
metadata guards still apply. Conversation discovery may match table names only
in adapters that declare bounded source reads; unsupported optional adapters are
labelled omitted. Legacy `/api/ai` keeps its no-enrichment fallback. Neither path
discovers contracts by name or scans compiled artifacts for prompt matches.
Neither helper access nor query grants authorize contract disclosure.

**Existing check coverage (not rerun for r8):** Router checks cover history, scope, failure, conflict and the
retrieval-scoped reference material above; the
`just conversation-postgres` recipe checks default and delegated persistence
against two disposable databases. `just conversation-browser` exercises the
sidebar and inert SQL with a fake helper, and `just cell-panel-visual` measures
the cell panel at 28.1 percent of the cell against a 30 percent ceiling. The
[notebook session exchange file](../crates/server/features/notebook-session-exchange.feature)
is bound to those router checks. A manual Build-host check of
`deepseek-go` and saved history is recorded in the platform stage record, not
run by this repository's CI. The
[conversation Gherkin file](../crates/server/features/conversations.feature)
is `@unautomated` despite these independent checks.

## Authorized ODCS-first assistance (S5 candidate)

Full Build-host gate: twelve S5 tests, three bound scenarios/three steps, PostgreSQL
contract/catalog ledger persistence, full CI and isolated Compose GREEN. Both P2s
from independent S5 RV `ses_f05b08380ffePmVosl4XKql3ZF` are fixed and verified;
no new reviewer sign-off is claimed. Exact hashes and blockers are in the receipt.

Choose a compiled contract and exact schema object in the notebook's contract
panel before sending a notebook or cell question. The UI sends only
`contractSelection: {path, sha256, object}`; it never trusts a cached projection
as admission. The same optional shape works on `POST /api/ai` and `SendMessage`.
The shared preparation path carries selected meaning, field definitions,
roles/expressions, declared grain/relationships, provenance and explicit bindings.
Missing or ambiguous bindings remain explicit; helper instructions forbid guessed
identifiers, joins, aggregates and execution. Fixture draft checks verify plumbing,
not arbitrary model accuracy. Cube is not required.

Q5 was explicitly accepted on 2026-10-01. Denied/revoked/unverifiable selected
context and oversized mandatory context refuse before helper calls. Optional
denied, unavailable or oversized observations are labelled omitted while keeping
authorized meaning. Denied observations make zero catalog calls. Bounded
observation is an opt-in catalog port; existing live adapters currently omit it
rather than performing unbounded IO. Their ordinary S3 browse behavior is unchanged.

Every history read/replay rechecks saved contract dependencies against current
team membership, grants and the compiled manifest, and rechecks admission for
catalogs whose observations were actually sent. Catalog provenance also covers
unselected legacy grounding: an authorized contract cannot launder a now-denied
observation through older replies. Refusal leaves stored messages
unchanged and requires a fresh conversation (for example, a new notebook).
Legacy untracked history cannot establish dependency authorization. Reference
blocks are transient; only exact contract/catalog dependency identities and manifest provenance
are stored alongside normal user/assistant messages. A helper reply may naturally
contain selected meaning and is therefore subject to the same replay admission.
Every helper attempt uses a fresh upstream session identifier. Local conversation
IDs remain stable, and admitted history is sent explicitly. Client-supplied IDs
and provider state from failed/conflicting turns cannot resurrect cached context.
Failed history admission clears displayed notebook/cell replies while preserving
the composers and stored messages. Older in-flight reads cannot restore that cache.
This includes a denied send from an already loaded cell panel: both failed reads
and failed sends invalidate its transcript and read generation. Dependency limits
cover the union of saved history and the proposed exchange before contacting the
helper; repeating an existing selection does not consume another slot.
To disable enrichment, remove the compiled-bundle configuration while retaining
the S5 history reader. Grounded or untracked histories then remain closed; fresh
unselected conversations retain the safe fallback. Downgrading to a pre-S5 reader
over grounded histories is not a safe rollback because it ignores dependencies.

Conservative hard defaults reuse existing input/history precedents; constants live
in `ai_context.rs`, `ai.rs` and the bounded preparation call:

| Material/work | Limit |
|---|---|
| Prompt / attached SQL | 8 KiB each |
| Selected reference, JSON-escaped, with label reserve | 16 KiB |
| Whole serialized helper JSON body, including history | 384 KiB |
| Mandatory string / object properties / depth / nodes | 4 KiB / 64 / 16 / 1,024 |
| Unique contract/object selections, history plus proposed exchange | 8 total |
| Unique observation catalogs, history plus proposed exchange | 32 total; admission rechecked without catalog IO |
| Stored transcript capacity | 200 messages / 256 KiB content plus dependency identities |
| Current grants file per admission | Existing 1 MiB regular-file cap |
| Compiled source | Existing 4 MiB per document, 32 MiB bundle, 128 documents |
| Selected optional observation | 256 KiB total source bytes, 2 source reads, 1 second; 64 columns / 4 KiB projection |
| History admission plus selected preparation | 5 seconds |
| Legacy optional discovery | 32 metadata calls, each declaring at most 256 KiB total source bytes; 5 seconds |
| Helper response / reply / request deadline | 128 KiB / 32 KiB / 60 seconds |

Mandatory context is refused rather than silently truncated. Optional material is
omitted, and JSON serialization is capped while writing, including escaping.
The [RV receipt](plans/odcs-ai-catalog-rv.md) records the current gate status;
`just odcs-ai-context-validation` is the success-only Build-host gate.

## Shared models

**Admin-only registry, opt-in:** With `ASTER_SHARED_MODELS_ENABLED=1`,
administrators can register, list and remove shared model metadata through
`/api/admin/shared-models`. This registry is separate from personal helpers.
It encrypts tokens before PostgreSQL persistence, binds ciphertext to the
registration's ID, URL and model, and accepts only origins in
`ASTER_SHARED_MODEL_ALLOWED_ORIGINS` over HTTPS. The application keyset comes
from the secret store as `ASTER_SHARED_MODEL_KEYS`; the selected version is
`ASTER_SHARED_MODEL_ACTIVE_KEY`. A missing key, or one that cannot decrypt an
existing row, prevents the registry from starting. The
[chart](../charts/aster/README.md) is disabled by default and reads the keyset
from a separately supplied Kubernetes Secret.
Grant replacement uses `PUT /api/admin/shared-models/{id}/grants` with an
expected revision; reads and audit events have adjacent admin routes. A stale
revision conflicts without changing the grants. In an OIDC deployment,
administrator routes require a freshly checked current identity. Set
`ASTER_SHARED_MODEL_AUTHORITY_ENABLED=1` with a signed user UUID claim,
an HTTPS Authentik API origin, stable admin/editor group UUIDs and a dedicated
read token from `ASTER_AUTHENTIK_READ_TOKEN`. The chart can configure this
authority while `useEnabled` remains false. Without it, SSO admin management
fails closed; isolated cookie-free development login remains a test seam.

For key rotation, first deploy a keyset containing both old and new versions
and select the new active version. Restart and verify registry access, then an
administrator calls `POST /api/admin/shared-models/rekey` with
`Content-Type: application/json` and `{}` to re-encrypt all rows
transactionally. Only after the call and a restart with the new keyset
succeed may the old version be removed. Back up the database and keyset before
rotation; losing every key that can decrypt a row makes its token unusable.

**Planned:** Grant-based shared use through existing groups or roles, a
scope-qualified helper selector, and immediate denial after authoritative
membership removal. Admin registrations are not listed to ordinary users and
cannot be selected for generation yet. The
[shared-model contract](../crates/server/features/shared-ai-models.feature)
remains partly `@unautomated` until those paths are bound and proved.
The identity transport retains an optional signed user UUID, and an opt-in
Authentik API adapter has passed disposable direct/inherited group and
first-request removal checks. No deployed read-after-write revocation proof
exists; cached session groups and roles cannot authorize shared use.
