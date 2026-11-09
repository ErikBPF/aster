# AI assistance

**S8 inventory boundary:** with a compiled bundle configured, unselected notebook
assistance omits physical metadata rather than bypassing schema-owner/object
admission. Select an admitted contract for metadata context. Old history carrying
only catalog-wide dependencies cannot prove object admission and refuses replay
in this mode; start a fresh conversation. Selected contract meaning remains
artifact context, not evidence that arbitrary SQL is authorized. The no-bundle
development path retains its historical behavior and is not r20 policy enforcement.

The AI settings page at `/settings/llm` has Skills, Models, Connections and administrator
controls. Personal model registration requires an administrator grant to the
current user or one of their current teams. Revocation blocks new registration,
updates and generation, including previously selected models. Owners can still
remove retained registrations. Company models use their separate administrator
managed ACLs; personal registration permission does not grant company model use.

Personal registrations contain a name, base URL, model and token. Responses never
return tokens. Production destinations must be approved HTTPS origins in
`ASTER_PERSONAL_MODEL_ALLOWED_ORIGINS`. PostgreSQL tokens are encrypted using
`ASTER_PERSONAL_MODEL_KEYS` and `ASTER_PERSONAL_MODEL_ACTIVE_KEY`. Without keys,
metadata and owner deletion remain available, but registration and use are disabled.
When keys are supplied, startup migrates legacy plaintext tokens transactionally;
a failed migration preserves the old records. Keep the keyset available across
restarts. Memory storage is disposable.

Do not run old plaintext writers alongside this version. After migration, an old
binary cannot use the encrypted credentials. Recovery requires a compatible
binary and the retained keyset, or restoration of a pre-migration database backup;
never recover by writing tokens back as plaintext.

`ASTER_AI_AUTHORITY_ENABLED=1` enables fresh identity and role checks for skills
and personal-model policy independently of company model configuration. Production
requires the configured Authentik identity reader and stable role group UUIDs;
cached session membership cannot grant permission. An isolated development-login
fixture is supported for testing.

New registry operations use the [Connect service](../proto/aster.proto). Existing
`/api/llm` endpoints enforce the same policy. Editors can request completions;
viewers cannot. Model responses remain suggestions to review before execution.

Notebooks expose **Manage models** directly in the toolbar; it opens the Models
tab for registration under the existing admin-managed permissions. Notebook and
cell assistant headers expose the shared notebook **AI model** choice.

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
follow-up questions and reload the private history. Both the notebook and each
cell keep server-owned sessions: the newest is selected by default, older
sessions remain selectable tabs labelled by a short phrase from the first user
message and the creation date, and **+ New** starts an empty session. The **×**
beside a tab deletes that exact session after confirmation. Deleting the active
session selects another remaining session; deleting the last leaves a fresh
draft, persisted only when you send or choose **+ New**. A session with an
unresolved tool turn must be resolved or cancelled before deletion. Stale
revisions refuse deletion so another tab cannot silently lose newer messages.
Notebook content and cells are preserved. Session
history is scoped to the authenticated subject and to the notebook or cell;
listing never returns another scope. A cell's **AI** action opens a panel beside
that cell: a separate conversation grounded on the cell's SQL and its last run
output. Notebook chat can fetch a chosen cell's SQL or last result into its
composer, and cell chat can fetch the notebook summary and cell index. Each fenced SQL block in notebook and cell replies has a query preview and an explicit cell destination. **Replace cell SQL** records an undoable replacement; **Create SQL cell** creates an undoable SQL cell. Markdown destinations keep their content and receive a new SQL cell immediately after them. Deleted targets are refused; moved targets or changed cell types require reviewing the updated destination and clicking again. The two histories
are distinct — a cell conversation cannot read the notebook conversation or
another cell's, and the notebook conversation cannot read any cell conversation
— and each is scoped by authenticated subject. Selecting another helper sends
that notebook's history to the newly selected helper. Model text is rendered
inert; each fenced SQL block gets explicit transfer controls and never runs by itself, and Ctrl/Cmd+Enter runs the focused cell or
sends the focused chat message. The cell panel scrolls rather than stretching
the cell, and navigating away from a notebook no longer asks for confirmation.

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

With `ASTER_SHARED_MODEL_USE_ENABLED=1` and the current-identity authority
configured, models appear only for authorized users under
scope-qualified `shared/<id>` references. Generation checks current identity and
grants again; a saved selection cannot bypass revocation. The
[shared-model contract](../crates/server/features/shared-ai-models.feature)
retains its explicit automation boundaries. Deployment-specific identity
revocation still requires validation against the configured identity service.

## Versioned skills

A skill contains a name, description, instructions and up to 16 named text
reference files, with a combined 32 KiB content limit. The workspace supports
search and scope filtering, a file-focused editor and a separate sharing dialog.
Choose **Edit** to manage a skill's instructions, reference files, saved versions,
permissions and deletion together. The Versions section opens historical content
read-only; editing the current version saves a new immutable revision. Existing
conversation pins do not change automatically. Non-owners can inspect shared
skills but cannot edit or delete them.

Reference assets are text files inside the skill package. **File name** is a name
with an extension, such as `checklist.md`; **Folder** is optional and defaults to
`references` for new files. The preview shows the complete package path, such as
`references/checklist.md`. Leave Folder blank for a root file beside `SKILL.md`.
Existing root and nested paths are preserved. The navigator displays collapsible
folders and filenames; the selected file shows its full package path in the
editor. **Add file** sits in the navigator. File name, Folder and File content
labels are unnumbered, and saving keeps the current file selected. On narrow
screens the bounded navigator stacks above the editor. These are logical package paths,
not local filesystem locations; traversal, duplicate paths, and file/directory
collisions remain invalid. Text files cannot execute bundled code.

**Delete skill** is inside Edit and requires confirmation. The owner must delete
against current content and sharing revisions. All version content is removed;
the identifier is retained as a tombstone so old pins cannot resolve to unrelated
replacement content. Existing conversation messages remain visible, but any
conversation that used the deleted skill must start a new session before further
turns. There is no restore or individual-version deletion UI.
Reference names can use safe relative paths such as `references/checklist.md`;
absolute paths, traversal, duplicates and file/directory collisions are refused. Bundled code is never executed. Saving creates an
immutable version; concurrent edits conflict instead of overwriting a version.
Owners edit their skills and can share them with a team they belong to.
Administrators can publish their skills to the organization. Sharing grants use,
not edit access, and the current sharing policy applies to every old version.

Notebook and cell conversations can enable up to eight skills independently,
each pinned to an exact version. Use the checkbox to turn a skill on or off and
its version selector to upgrade explicitly. **My defaults** in the AI workspace
stores personal selections for new conversations, including new cell sessions.
Defaults survive restart with PostgreSQL metadata and never change existing
sessions. Saving defaults does not share a skill or grant access. Combined
selected skill guidance is limited to 64 KiB. Subsequent turns retain their
selected versions until the user changes them.
A published update never silently changes an existing conversation. Every turn
rechecks access to all skill versions used in its history; if access was revoked,
start a new session. Clearing the current selection does not remove historical
access requirements. Each installation is one organization.

Registry validation uses `just ai-registry-test`, `just ai-registry-browser`,
`just notebook-ai-browser` (real Rust notebook/cell UI with controlled model/MCP endpoints),
`just ai-registry-postgres-test` and `just ai-registry-live-browser`. See the
review and local deployment record for observed
results and runtime configuration limits. The notebook controls validation
records multiple selections, personal defaults and notebook MCP execution.


## MCP connections and tool explorer

The Connections tab lists remote MCP servers available to the current user.
Administrators manage endpoints and direct-user/configured-team grants. Disabled
connections are unavailable. Metadata edits use revision checks and are audited.
Bearer tokens are write-only, encrypted with `ASTER_MCP_KEYS` and the active
`ASTER_MCP_ACTIVE_KEY`; missing keys disable registration and incorrect keys fail
startup. `ASTER_MCP_ALLOWED_ORIGINS` is a separate HTTPS origin allowlist; model
origins do not authorize tool servers. Keep old key versions until stored rows
have been rewritten with the new active key. No bulk MCP rekey endpoint exists.

Select **Tools for …** to discover tools, inspect their argument schema and run
one with JSON arguments. An administrator can approve a specific current tool
definition as read-only. Such calls run without another confirmation. Every
other tool requires confirmation showing the connection, tool and exact
arguments. Confirmation expires after five minutes, is bound to the user and
connection revision, and is consumed atomically once. Definitions and current
membership are rechecked immediately before dispatch. A changed schema requires
new read approval. Changing the endpoint or credentials clears read approvals.
Skill text and server annotations never grant execution authority.

The HTTP adapter supports MCP Streamable HTTP protocol `2025-03-26`, JSON and
SSE replies, up to eight discovery pages/100 tools/256 KiB, and a 20-second
transport operation limit. Argument JSON is bounded to 32 KiB and validated
against the tool schema. Redirects and automatic changing-call retries are
refused. If the action outcome is unknown, verify the remote system before
retrying. The AI workspace retains an explicit tool explorer. Notebook and cell composers
also list available connections, initially off in each new session. Enable a
connection to offer its tools to the selected model; this never overrides admin
grants or connection policy. Up to four connections and 32 tools can be offered,
with at most four calls per turn. The model endpoint must support OpenAI-compatible
function calling. Admin-approved reads run automatically. Other calls display
the exact connection, tool and JSON arguments for confirmation.

Pending tool turns are stored with the conversation and survive reload/restart.
Confirmation is bound to the user, notebook/cell, session, revision and exact
persisted action. Concurrent resumes cannot dispatch the same action twice.
Unknown outcomes stop the turn; verify the remote outcome before starting a new
request. Cancellation does not undo completed actions. A turn expires after five
minutes; individual model requests are limited to 30 seconds, and notebook tool
results to 32 KiB. Disabling connections prevents new calls, but historical
source access remains required before further model requests. There are no
personal MCP defaults. Before downgrading to a version without tool-turn support,
resolve pending turns; old versions do not enforce the new pending state.
OAuth, stdio, sampling, elicitation,
resources and MCP Apps are not supported.

`ASTER_MCP_DEMO=1` allows the Donsetch mock only with isolated development
identity. Its default endpoint is `http://127.0.0.1:7777/mcp`;
`ASTER_MCP_DEMO_ENDPOINT` may select another loopback HTTP port with the same
`/mcp` path for disposable tests. `tests/fixtures/donsetch-mcp.py` provides a
synthetic catalog read and simulated note receipt, with no external changes.
No real tool server is pre-approved by this demo setting.

Skills can import a local Markdown file or a JSON package with `name`, optional
`description`, and `files: [{path, content}]`, including exactly one root
`SKILL.md`. Imports are unsaved private drafts; package IDs, owners, access and
revision counters are ignored. Export uses the selected saved version, even if
newer edits remain unsaved. JSON input may be up to 256 KiB to accommodate
escaping; decoded skill content retains the 32 KiB limit. ZIP archives, binary
files and URL imports are not supported. Imports reject malformed UTF-8,
unpaired Unicode surrogates, whitespace-only names, duplicate or colliding paths,
and invalid package shapes before replacing a draft. Name and description limits
are 160 and 512 UTF-8 bytes respectively. Export is disabled while a different
version loads. A UTF-8 BOM is accepted; the text editor normalizes CRLF line endings
to LF, and metadata is trimmed when saved. Export itself uses stored version text.

### Compact conversation controls

Notebook and cell composers keep the message box beside an Attach, Skills and
Tools toolbar. Skills and Tools show enabled counts and open searchable menus;
exact version selection, history and refresh stay inside the skill menu.
Attachment details open through Attach, with a removable context chip after
selection. Pending tool confirmations remain in the conversation, with exact
arguments available before approval. Personal defaults remain in AI settings.

See the layout delivery record for browser
coverage and deployment status.

## Local mock assistant

`ASTER_MOCK_ASSISTANT=1` permits exactly `http://127.0.0.1:7778/v1` for personal models, only with isolated development identity. SSO startup refuses this option; approved HTTPS origins remain unchanged. The fixture `tests/fixtures/mock-assistant.py` binds loopback in the server network namespace and implements the OpenAI chat-completions shape without external calls or inference. Register model ID `aster-mock-assistant` with an empty token. Keep this flag disabled outside the local demo.

`just mock-assistant-e2e` explicitly targets localhost:8081, registers the mock assistant and Demo orders SQL skill for the development administrator, creates a separate demo notebook, verifies the disabled-skill response, enables version 1, inserts the generated SELECT and explicitly runs it on `mock-local`. It retains the model, skill, notebook and conversation for manual exploration. Receipts and screenshots go to `/tmp/aster-mock-assistant-e2e/`. The mock engine returns synthetic rows; this proves the application workflow, not model quality or SQL evaluation semantics.
