# Notebook sessions, notebook engine and cell kinds — PL

Status: discovery complete; decisions below are accepted. Execution moved to
`apollo:/home/erik/Documents/erik/aster/worktrees/notebook-tabs-sessions-20261029`
so builds and tests do not use the laptop CPU.

## Human seed (verbatim)

> lets do a /pl /ip and /rv on on aster lets improve the tabs experience inside
> a notebook. Lets also add butons to fetch and set query/result between
> notebook agent and cell agents
>
> We should be able to tart new sessions cell and notebook wise. After the first
> we should be able to be on "latest" or in a older one. Older ones should be
> identificated via a small phrase and date
>
> lets make engine notebook-wide
>
> on notebook we should have a button on insert sql on current cell
>
> lets also add cell creation buttons in the space bellow every cell. like
> jupyter notebook. Lets also accept markdown sintax. All cells are by default
> "%sql" but  they can receive a "%md" and become markdown. It should also
> accept mermaid graphs

Follow-up seed: "move whole work to apollo. We shouldnt be using this laptop
cpu".

## Accepted decisions (human, 2026-10-29)

| Decision | Choice |
|---|---|
| Meaning of "tabs" | AI session tabs inside notebook and cell chat panels |
| Older session labels | Auto phrase from the first user message plus date |
| Fetch/set semantics | Attach query/result material to chat context; set buttons insert SQL into the current cell |
| Markdown/Mermaid | Vendor pinned marked, DOMPurify and Mermaid assets; sanitize rendered HTML |
| Notebook engine | Persist on the notebook and enforce server-side |

## Destination

Aster notebooks gain:

1. Server-owned AI conversation sessions, scoped per notebook and per cell. The
   latest session is the default; older sessions remain selectable tabs labelled
   by an auto phrase and date; a new session can be started from either panel.
2. Explicit exchange controls: notebook chat can fetch a chosen cell's SQL and
   last result into its composer; cell chat can fetch the notebook summary; a
   notebook reply can be inserted into the current cell.
3. One engine per notebook, persisted in the notebook document and enforced for
   every cell run.
4. One Jupyter-style `+ Cell` control below each cell, plus `+ Add cell` below
   the whole list. The cell becomes Markdown by typing `%md`.
5. `%sql` (default) and `%md` cell magic. Markdown cells render sanitized
   Markdown, including Mermaid fenced blocks, and never execute SQL.

## Non-goals

- No cell-as-tab layout and no change to two-browser-tab conflict handling.
- No model-generated or user-edited session titles.
- No cell conversations in team workspaces (existing boundary).
- No collaborative editing, drag-and-drop reordering or realtime cursors.
- No provider call is made merely to create, list or title a session.
- No Markdown execution, embedded HTML, remote content or script in rendered cells.

## Constraints

- Aster stays standalone: no Harbor, no private registry, no CDN at runtime.
- `aster-core` stays IO-free.
- Conversations and exchanges remain private to the authenticated subject and
  never cross notebook or cell boundaries.
- The Git notebook format stays text-first and readable; existing v1/v2 files
  load unchanged.
- Every behavior change lands with a `.feature` contract and a failing-first test.
- Builds and tests run on Apollo inside the declared devenv.

## Decision map

| Question | Decision | Evidence |
|---|---|---|
| Where do sessions live? | Conversation store, one row per subject+scope+session | `crates/server/src/conversations.rs`, `migrations/0005_conversations.sql` |
| How is "latest" ordered? | `updated_at` descending; first is latest | Store contract |
| What is the phrase? | First user message, whitespace-collapsed, 60 chars max | Human decision |
| What is the date? | Session creation time, rendered locally as short date | Human decision |
| How are old conversations preserved? | Backfill the existing row as its first session | Migration |
| Where is the engine stored? | Notebook document (`# engine:` header) | Human decision |
| What happens to per-cell engines? | Legacy files still parse; UI writes notebook engine only; a conflicting run is refused | Human decision |
| How is Markdown selected? | First non-empty line `%md`; `%sql` accepted and stripped; no prefix means SQL | Human decision |
| Who renders Markdown? | Browser: marked + DOMPurify + Mermaid, pinned and served by aster | Human decision |
| How do fetch/set work? | Existing exchange RPCs plus local current-cell insertion | `proto/aster.proto`, `docs/ai.md` |
| What if a session has no message? | Phrase "New session"; date = creation date | Design |

## Behavior examples

Bound server contract: `crates/server/features/notebook-sessions.feature`.

Browser-only contract (draft): `crates/server/features/notebook-cells-ui.feature`.

Key examples:

- A new notebook session becomes the latest; the previous session remains
  selectable and is labelled by its first message and creation date.
- The same notebook and cell, for two subjects, never share sessions.
- A cell run after a notebook engine is saved uses that engine; a request that
  names a different engine is refused.
- A `%md` cell cannot run SQL; its notes render inertly, and Mermaid fenced
  blocks render as diagrams rather than executable content.
- Notebook chat can fetch cell `q1`'s SQL and result into its context; inserting
  a reply writes the current cell's editor without running it.

## Risks and recovery

| Risk | Mitigation |
|---|---|
| Session migration loses history | Additive table, backfill from existing rows, keep old table untouched |
| Markdown/Mermaid XSS | DOMPurify sanitizes Markdown; Mermaid strict security; no raw HTML |
| Large Mermaid asset | Embedded asset route; no runtime CDN; browser cache headers |
| Session sprawl | Per-scope list only; no unbounded prefetch; existing transcript caps apply |
| Engine enforcement breaks TUI | TUI reads notebook engine, falling back to legacy cell engine; never silently routes elsewhere |
| Unsaved edits vs `UpdateQuery` | Insertion updates the editor locally; exchange writes are explicit and revision-checked |

## Open questions

None blocking. Visual density of the session tab strip and the exact short date
format are implementation choices covered by the browser draft.
