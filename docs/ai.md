# AI assistance

**Implemented:** A signed-in user registers named personal
OpenAI-compatible helpers. Each registration contains a base URL, model and
token. The server stores the token and proxies completion requests; list and
settings responses expose the name, safe URL and model, not the token. An
accepted helper URL is absolute HTTP(S) with no username, password, query or
fragment. Previously stored invalid URLs appear blank in API/settings and
cannot be used for generation until re-registered. A notebook can select one
of that user's helpers. The legacy AI cell action asks for SQL and
can include current cell SQL and relevant data-contract summaries. Users
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
text is rendered inert; a fenced SQL block gets an explicit **Insert SQL as new
cell** button and never runs by itself.

The notebook conversation reaches cell material only through five explicit
exchange operations, each scoped to the requesting principal's notebook and none
reading a cell conversation: `FetchQuery` (one cell's SQL and the content
revision it was read at), `FetchResult` (one cell's last recorded result),
`FetchSummary` (the notebook summary and the cell index), `SendSummary`
(replace the summary, 8 KiB) and `UpdateQuery` (replace one cell's SQL against
the expected content revision, refused as a conflict when stale). A cell run
through `/api/query` or `RunQuery` records its result when the request names the
notebook and cell, keeping at most 100 rows per cell. Exchange material is
bookkeeping, stored apart from the Git notebook document and the conversations:
`ASTER_EXCHANGE_DATABASE_URL` selects `postgres` and installs only exchange
tables, `ASTER_EXCHANGE_STORE` overrides that selection, and disposable `memory`
is the default.

`GetConversation` and `SendMessage` are Connect RPCs. A completed user/assistant
pair is saved atomically with a revision check. If a model or storage call
fails, the composer keeps the draft; a stale revision returns a conflict and
the user must explicitly resend after seeing current history. The server
owns the upstream conversation identifier, including for OpenCode Go helpers.
Prompts and attached SQL are limited to 8 KiB each, replies to 32 KiB, and
history to 200 messages or 256 KiB. Overflow is refused without truncating
saved messages.

Conversation storage defaults to the metadata store. With PostgreSQL metadata,
history survives server restart. To delegate storage, provide a separate
`ASTER_CONVERSATION_DATABASE_URL` through the deployment's secret/environment
mechanism; it selects `postgres` unless `ASTER_CONVERSATION_STORE` explicitly
selects `metadata` or disposable `memory`. The dedicated database creates
only conversation tables. Changing the database does not copy old history.

Each turn carries retrieval-scoped reference material to the helper: for every
data contract whose table the question or attached cell SQL names, the contract
summary (owner, description and field semantic types), the catalog's live column
schema for that table, and the Cube semantic model emitted for it. The material
is bounded, transient context for that turn only — it never enters saved
history — and is labelled untrusted, never instructions.

**Verified:** Router checks cover history, scope, failure, conflict and the
retrieval-scoped reference material above; the
`just conversation-postgres` recipe checks default and delegated persistence
against two disposable databases. `just conversation-browser` exercises the
sidebar and inert SQL with a fake helper. A manual Build-host check of
`deepseek-go` and saved history is recorded in the platform stage record, not
run by this repository's CI. The
[conversation Gherkin file](../crates/server/features/conversations.feature)
is `@unautomated` despite these independent checks.

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
