# Notebook interface — delivered

Human seed, verbatim: “lets also take some prints of notebook interface and propose improvements to cell creation buttons, catalog, schema, helper and ompute options”. User acceptance, verbatim: “do as recomended”.

## Outcome and scope

The accepted layout is deployed at localhost:8081. [Before/after screenshots](index.html) preserve the original observations, [static concept](concept.html), and nine new runtime captures. Work remains in `aster/worktrees/ai-registry-plan`, branch `feat/ai-registry-plan`; no commit or push was made.

Originally, creation buttons appeared in the notebook toolbar, every cell and a full-width bottom row. Cell headers repeated identifiers, catalog/schema selects, Run, AI, arrows and delete. Compute and model selection competed with primary notebook actions. The delivered layout keeps SQL and primary actions prominent while disclosing secondary choices.

## Accepted behavior and delivered changes

- **Creation:** one persistent bottom `+ Cell` plus between-cell insertion on hover/focus, visible on touch. Both offer SQL and Markdown and preserve exact insertion position. The final cell uses the bottom chooser without a duplicate insertion row. Existing keyboard shortcuts remain; Markdown creation shows its type without requiring a magic marker.
- **Data:** compact per-cell `Data: catalog / schema` opens searchable selectors with complete namespace paths. Opening/searching preserves selection. Explicit catalog changes clear schema with visible feedback. Loading, empty, unavailable and denied states remain distinguishable; no notebook-wide inheritance was added.
- **Compute:** notebook-wide selection moves into a popover. Selected targets include server-reported kind/health. Default is explicitly unresolved because the API does not identify its resolved target. Changing compute does not run SQL or rewrite data context.
- **AI model:** notebook and cell assistant headers expose the same canonical selector and persisted preference. The shared popover explains model-history forwarding, empty availability and the AI settings path. ACL-filtered choices and permission enforcement remain server-owned.
- **Cell actions:** type, Data, Run and AI remain visible; identifier, move up/down and delete move into an accessible popover. Execution prompts remain in the gutter.

## Review and evidence

Baseline source is preserved at `/tmp/aster-notebook-ui-before*`; the frozen first draft is `/tmp/aster-notebook-ui-draft1/`. Independent review found and verified five issues: clipped insertion, bubbling clicks restoring the preceding current cell, an unnecessary secure-context UUID dependency, overwritten empty-catalog feedback, and the missing history warning at the new cell-model entry point. All were corrected. A further regression exposed search inputs consuming Escape; dismissal now restores focus without closing the assistant.

RED receipts: `/tmp/aster-notebook-ui-red.log` (duplicate toolbar creation), `/tmp/aster-notebook-ui-clipping-red.log`, `/tmp/aster-notebook-ui-current-cell-red.log`, and `/tmp/aster-notebook-ui-escape-red.log`. The first browser draft also exposed an empty fixture assumption, corrected by creating draft cells before insertion checks. One independent preview log was discarded because overlapping runs reused its filename; review closure is static, with runtime evidence from the clean parent runs.

Apollo `/tmp/aster-notebook-ui-final-gate2.log` records passing `just notebook-ai-browser`, all 36 `just ai-registry-browser` flows, and full `just ci` with exit zero. Coverage includes namespace filtering, explicit catalog reset/403 feedback, insertion order/current-cell targeting, shared model selection, 390px bounds, existing skill/MCP controls and session deletion. The `.feature` remains marked unautomated; these browser checks are separate runnable evidence.

## Deployment, limits and recovery

`just image-build aster-server:notebook-layout` succeeded on Apollo. Local server/controller both run image `aed6758aeb8b4e08d18369210b33c425dd62bf5c8b051d04d6db0563ea0654b9`; Donsetch restarted successfully and `/healthz` returned `ok`. `/tmp/aster-notebook-ui-local.log` records nine captures, zero notebook writes/query requests and zero JavaScript errors. The existing notebook was preserved; only an unsaved Markdown draft was added for screenshots.

Live inference remains untested because this local account has no registered model. Other browser engines and alternative compute providers were not exercised. No schema migration was introduced. Recovery uses `localhost/aster-server:pre-notebook-layout` and the saved `compose.before-notebook-layout.yaml` in the local stack configuration directory. No required implementation or validation work remains.
