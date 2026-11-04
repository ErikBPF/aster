# Catalog navigation — RV

Status: reviewed; blocking findings fixed; CI green. Static review only for
non-UI paths unless noted.

Reviewer: independent architect read of the uncommitted diff (session
`ses_e629618b0ffeEUR4LxMxpa3NO8`). Worktree:
`apollo:/home/erik/Documents/erik/aster/worktrees/notebook-tabs-sessions-20261029`.

## Blocking findings fixed

| Finding | Fix |
|---|---|
| Detail matched compiled contracts without admission and leaked team-gated IDs/paths | Contract candidates now pass the existing `contract_reads::read_for_principal` admission check; denied contracts return `null` and the pane says `admitted` only for a passing read |
| Parent-directory symlinks could escape the asset checkout | Every read/write canonicalizes the parent and requires the result to stay under the checkout root |
| Viewing a history revision overwrote the blob revision used for the next save | Revision view now keeps the working blob and adds a separate `viewRevision`; Edit is hidden in revision view |
| Historical reads accepted any blob OID in the checkout | `revision_document` now requires a commit OID and reads `<commit>:<path>` |

## Non-blocking findings fixed

- Asset responses carry `nosniff`, `Content-Disposition: inline` and
  `Content-Security-Policy: default-src 'none'; sandbox` for SVG safety.
- Query history returns the newest matching events, not the oldest.
- Detail fetch uses a request generation guard; a stale selection cannot render
  over a newer one.
- Table-name matching uses identifier boundaries, not raw substring matching.
- Failed Git add/commit restores the previous document bytes.
- A no-op save returns the current document without a failed commit.
- Saves require the blob OID; history commits remain separate.
- Chart values expose an optional `server.catalogAssetsDir`; empty keeps the
  Docs tab disabled.

## Evidence

| Check | Result |
|---|---|
| `just ci` on Apollo | `CI GREEN (fmt + clippy + tests + features + repo + providers)` |
| `cargo test -p aster-server --test contracts` | 19 features, 98 scenarios, 361 steps passed |
| `catalog_assets` unit tests | save/conflict/history/commit read/path traversal/image allow-deny pass |
| Browser check | lateral tree, schema/table expansion, four tabs, columns, docs save, asset image, history commit, no page errors |
| Local preview | `ASTER_CATALOG_ASSETS_DIR=/tmp/aster-cat-assets` on `127.0.0.1:8099` |

## Remaining risks

- The asset writer uses a process-local mutex. Two replicas sharing a checkout
  can race at Git level; use one writer or a shared lock before multi-replica.
- `ASTER_CATALOG_ASSETS_DIR` must exist as a Git checkout. The chart now
  provisions `catalogAssets` PVC and runs an init container to create the
  repository; a manually configured path still fails startup if it is not Git.
- SVG is served as an image with CSP sandbox; direct top-level navigation remains
  a browser-dependent surface.
- Viewer UI still shows Edit; the server returns 403. Hiding by role is deferred.
- No test seeds a team-gated contract and asserts the detail pane hides it.
- The browser feature file remains `@unautomated`; the committed server contract
  covers the tree, detail, permissions, docs and asset paths.

## Search and rail menu

- `/api/catalog/search` searches visible catalogs, schemas and table names and
  returns at most 50 navigable results.
- The catalog rail has a debounced search box and result list; selecting a result
  expands the tree and opens the entity.
- A persistent `☰ Hide menu` / `☰ Show menu` button closes the rail and expands
  the detail pane; the choice is stored in local storage.
- Deployed check found `polaris / sales / orders` and toggled the rail closed and
  open.

## Delivery

Committed on `feat/notebook-tabs-sessions` as
`feat(aster): add navigable catalog with versioned docs`. Built and rolled out to
the Apollo `aster` minikube profile with a 1 GiB `catalog-assets` PVC, Git init
container, `GIT_CONFIG` safe-directory entry and `/data/catalog-assets`. Seeded
`catalogs/polaris/README.md`. `/catalog/physical` and the old deep-link pages
remain for compatibility; `/catalog/inventory` serves the compiled inventory
projection used by the S8 checks. The search and menu update was rebuilt and
rolled out to the same stack. No push.
