# Catalog navigation — PL

## Human seed (verbatim)

> Lets explore a more navigational interface for catalogs. it should be a lateral
> bar with all catalogs and clicking on those we expand to schemas and table.
> Every level show on center-right information about its details, history,
> permissions and docs. It need to be md friendly and versioned. Tables can
> present their information and complement with usefull assets. Those are git
> controlled  we can do a full /pl /ip and /rv for it

## Accepted decisions (2026-11-01)

| Question | Choice |
|---|---|
| Asset location | Separate Git checkout configured by `ASTER_CATALOG_ASSETS_DIR` |
| Docs editing | Editable in the browser, saved as Git commits with revision checks |
| History | Asset Git history plus the caller's recent query audit events |
| Permissions | Effective metadata visibility, bound engines and grants, contract declaration; never raw secrets |
| Table assets | Markdown plus images (SVG/PNG/JPEG/WebP/GIF), read-only serving |

## Destination

`/catalog` becomes one navigable workspace:

- Left rail: every visible catalog; expanding a catalog loads its schemas;
  expanding a schema loads its tables.
- Center-right detail pane for the selected catalog, schema or table, with four
  tabs: **Details**, **Docs**, **History**, **Permissions**.
- Details show live metadata from the existing catalog inventory.
- Docs are Markdown stored in a Git checkout, rendered with the existing
  sanitized Markdown/Mermaid path, and editable in the pane. Every save is a Git
  commit with an author and a stale-revision conflict.
- History lists commits touching the entity document and the caller's recent
  audit events for that scope.
- Permissions show metadata visibility, catalog-bound engines and whether the
  caller holds each engine grant, plus any compiled contract declaration for the
  table. Admission is not claimed where it was not evaluated.
- Table docs can reference images in an adjacent `assets/` directory; only
  raster/SVG images are served.

## Non-goals

- No new catalog adapter or metadata source.
- No contract admission decision in the catalog pane; the Contracts panel keeps
  that authority.
- No arbitrary file upload, executable asset, HTML, script or remote iframe.
- No Git push or remote synchronization from the UI; commits stay local.
- No editing of physical metadata, ownership or grants from this pane.
- No replacement of the notebook or contract review flows.

## Constraints

- Existing `/catalog/{catalog}/{namespace}/{table}` links redirect into the new
  query-addressable workspace.
- Catalog metadata stays behind `require_catalog_metadata`; compiled-mode
  inventory keeps its existing admission.
- `aster-core` stays IO-free. The Git asset store lives in `aster-server`.
- No secret, raw binding credential or owner configuration value is rendered.
- Paths are validated per component; no traversal, symlink escape or hidden file.
- Git commits are local, hook-disabled and bounded; no push.

## Decision map

| Question | Decision |
|---|---|
| Where is the tree built? | Server-rendered shell + JSON tree/detail endpoints; lazy namespace/table expansion |
| How are namespaces addressed? | Existing opaque namespace token (base64 URL-safe of segments) |
| How are docs stored? | `catalogs/<catalog>/…/README.md` for catalog/schema, `.../tables/<table>.md` for tables |
| How are assets stored? | Adjacent `assets/<name>`; only image extensions served |
| What is a doc revision? | Git blob OID of the working file; saves require the expected blob |
| What is history? | `git log -- <path>` plus filtered audit events for the caller |
| What is permission? | Metadata visible + bound engine IDs and grant booleans + contract declaration, explicitly not admission |
| How is the UI state addressed? | `?catalog=&namespace=&table=` query parameters |
| What if no asset checkout is configured? | Docs tab says disabled; navigation and metadata still work |

## Risks

| Risk | Mitigation |
|---|---|
| Path traversal / symlink escape | Per-component validation, reject dot components, canonical containment check |
| XSS via Markdown | Existing marked + DOMPurify + Mermaid strict path; images only |
| Git commit spam | Explicit Save, not per-keystroke; commit only changed docs |
| Stale edits overwrite | Blob OID precondition, 409 on mismatch |
| Audit leakage | Filter by authenticated subject server-side before serializing |
| Ephemeral demo checkout | Document persistent volume requirement; demo uses a mounted directory |
| Large inventory | Lazy expansion, bounded table lists, no all-table prefetch |

## Open questions

None blocking. Exact tab visual density and image gallery layout are browser
details; behavior is fixed by the contracts.
