# Catalog navigation — IP

Worktree: `apollo:/home/erik/Documents/erik/aster/worktrees/notebook-tabs-sessions-20261029`.

Base: `docs/plans/catalog-navigation-pl.md` and
`crates/server/features/catalog-navigation.feature`.

## Slices

### S1 — Git-backed catalog documents and assets

Ownership: `crates/server/src/catalog_assets.rs`, `lib.rs` state.

- `CatalogAssets` opens `ASTER_CATALOG_ASSETS_DIR` as a local Git checkout.
- Paths: `catalogs/<catalog>/README.md`,
  `catalogs/<catalog>/schemas/<segment>/…/README.md`,
  `.../tables/<table>.md`, adjacent `assets/<name>`.
- Validate every path component; reject dot components, separators, symlinks and
  unsupported asset extensions.
- Read current working doc and blob OID; save with expected blob OID, hook-free
  commit, subject author; list history; read historical revision; list/serve
  images with a byte cap.

Seams: pure path validation, store methods, temp Git repo tests.

### S2 — JSON tree, detail and permissions

Ownership: `lib.rs` routes, `web.rs` shell, `catalog_inventory.rs` reuse.

- `GET /api/catalog/tree?catalog=&namespace=` returns catalogs, namespaces or
  tables using the existing inventory authorization and namespace token.
- `GET /api/catalog/detail` returns level details, observed columns, effective
  permissions, docs metadata, asset list, asset history and caller-scoped query
  history.
- Permissions: metadata visibility, bound engines with grant booleans, and any
  compiled contract declaration. Contract admission stays unevaluated.
- Query history: server filters `AuditSink::events()` by authenticated subject
  and catalog/schema (table name in SQL) before serializing; no admin-only leak.

Seams: router JSON, stub catalog/engine/grants, audit filter.

### S3 — Docs API with revision checks

Ownership: `lib.rs` handlers.

- `GET /api/catalog/docs` returns current or historical Markdown plus blob OID.
- `PUT /api/catalog/docs` requires editor role, validates entity and 256 KiB
  bound, saves with expected revision, returns new document and history.
- Stale revision returns 409; missing asset checkout returns an explicit disabled
  payload rather than 500.

Seams: save/conflict/read-revision tests.

### S4 — Lateral tree and detail pane

Ownership: `web.rs` shell, `assets/app.js`, `assets/app.css`.

- `/catalog` renders a left rail + detail pane; old deep links redirect with
  query parameters.
- Tree: catalogs always; lazy schemas and tables; selection updates the URL.
- Detail tabs: Details, Docs, History, Permissions.
- Docs: sanitized Markdown/Mermaid preview, edit/save with revision, old revision
  view, image gallery.
- Permissions: visible/denied state, engine grant badges, contract declaration
  note. No secrets.
- History: asset commits and query events; docs commit click opens that revision.

### S5 — Deployment and docs

- Add `ASTER_CATALOG_ASSETS_DIR` to deployment configuration and document a
  persistent Git checkout; demo may use a mounted directory.
- Update `docs/catalogs-and-compute.md`, `docs/deploy.md`, provider matrix only
  if a port changes (it does not).
- Run `just ci`, catalog tests, and a browser check on Apollo.

## Verification

| Layer | Check |
|---|---|
| Store | temp Git repo: save, conflict, history, revision read, traversal, image allow/deny |
| Router | bound feature scenarios: tree, detail/permissions, docs, history, asset refusal |
| Browser | tree expansion, tabs, Markdown render, save, old revision, image, no secret text |
| Full | `just ci` on Apollo; deployed browser smoke |

## Rollback

- Code rollback is the branch; asset commits stay in the configured Git
  checkout. No catalog metadata or notebook data is touched.
- Removing `ASTER_CATALOG_ASSETS_DIR` disables docs while navigation remains.
- Old `/catalog/...` URLs redirect; reverting the handler restores the old pages.
