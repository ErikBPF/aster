# Proposed notebook feature set

The seven capabilities accepted in chat are deployed at localhost:8081; see the [delivery evidence](../notebook-refinement.md). Later additions remain proposals. The user's SQL-first creation rule remains the baseline: `%md` changes cell kind; no type chooser is proposed.

## Evidence

The running reference is Jupyter Notebook 7.6.1 at localhost:8083. Eight [actual screenshots](index.html) show notebook layout, the active cell, the cell context menu, an 820px view, and File/Edit/View/Run menus. The reference has a disposable example notebook and no host notebook mounts; no code was executed. Setup follows the [official Jupyter Docker Stacks instructions](https://jupyter-docker-stacks.readthedocs.io/en/latest/using/common.html).

Observed useful patterns: a distinct selected-cell gutter, visible move actions, a compact notebook toolbar, and selected-cell controls and a context menu offering duplicate/cut/copy/paste, split/merge, output controls and cell undo. Aster already has per-cell Run/AI, sequential Run all, execution prompts and assistant sessions. The proposal builds on those capabilities.

## Recommended sequence

| Priority | Feature | Observable result | Boundary |
|---|---|---|---|
| 1 | Recoverable cell editing | Undo/redo restores insertion, deletion and movement, including cell text and metadata; show a brief Undo action after deletion. | Keep notebook edits separate from irreversible conversation deletion. A remote revision conflict must still block overwriting newer work. |
| 2 | Active-cell navigation | One clear gutter marker identifies the cell targeted by shortcuts and assistant insertion; expose the existing shortcuts in a compact help panel. | Focus, selection and assistant insertion must agree after movement, deletion and Markdown transitions. |
| 3 | Duplicate, split and merge | Secondary cell menu gains Duplicate, Split at cursor, and Merge with next; operations can be undone. | Preserve `%md` semantics and metadata. Do not merge SQL and Markdown silently. Depends on priority 1. |
| 4 | Output management | Collapse/expand and clear a cell's displayed output; download displayed rows as CSV with an explicit truncation indicator. | Clearly distinguish clearing browser output from deleting stored exchange context. Export must not rerun the query or imply omitted rows are included. |
| 5 | Controlled notebook runs | Run from selected cell, visible queue progress and a stop-queue action; report failures beside the affected cell. | Current Run all continues after failures. Agree whether the new default stops on failure. Stopping queued work must not claim to cancel an in-flight engine query without backend support. |

## Keep the layout disciplined

Keep notebook Data and Compute together. Keep Run, AI and move arrows available on each cell. Keep destructive or less frequent editing operations in the cell menu. Keep Manage models directly reachable from the notebook, with skill/tool selection beside the assistant composer.

Do not add a named agent registry, a second model selector, a notebook-wide cell-type chooser, or inherited per-cell Data overrides in this iteration. The user clarified that “agents” means existing assistants and model registration. Start with recoverable cell editing and active-cell navigation; they make later editing operations safer without expanding execution permissions.


## Additional Jupyter features worth inheriting

Follow-up seed, verbatim: “Check other notebook features we should inherit from jupyter notebooks”. These recommendations combine the captured menus with official documentation; they are not implemented by this delivery.

| Order | Addition | Aster adaptation |
|---|---|---|
| Next | Heading outline and section collapse | Build an optional outline from `%md` headings. Clicking a heading scrolls to its cell; collapsed sections must still show running/error state. Keep the sidebar closed by default on small screens. |
| Next | Searchable command palette | Put existing and future secondary actions in one keyboard-accessible dialog. Reuse the same action handlers and permission checks as buttons. This directly addresses toolbar clutter. |
| Next | Notebook find/replace | Search cell source, with current-cell or whole-notebook scope. Preview replacements and make them undoable; do not replace rendered results or assistant history. |
| Later | Multiple-cell selection | Shift-select a range, then copy, move, run or delete together. Add only after operation-level undo and clear selected-cell state work. Keep visible arrows for single-cell movement. |
| Later | Revision history and restore | Expose existing Git-backed notebook revisions with a diff and restore-as-new-revision. Reuse Git instead of adding a competing checkpoint store; retain authorization and optimistic conflict checks. |
| Later | Richer SQL editing | Add line numbers, bracket matching and syntax highlighting, then improve the existing SQL completion with authorized catalog/schema context. Preserve screen-reader and keyboard access. |
| Later | Focus mode | Hide optional panels and collapse code/results for reading. Keep notebook scope and execution/error state discoverable; avoid another permanent toolbar row. |
| Conditional | Notebook interchange | Offer SQL/Markdown `.ipynb` import/export only when a concrete portability need exists. Preview unsupported cells, never silently convert Python to SQL, and never execute imported content or trust embedded HTML. |

Notebook 7 includes a built-in heading outline; its collaboration support requires an additional extension. These are distinct levels of adoption effort. [Official Notebook 7 features](https://jupyter-notebook.readthedocs.io/en/latest/notebook_7_features.html).

Jupyter's command system connects menus, shortcuts and a searchable palette. Its command inventory includes find/replace, cell selection, line numbers, split/merge and run-range operations. Aster should borrow discoverability and consistent actions, without recreating the full menu system. [Official command reference](https://jupyterlab.readthedocs.io/en/stable/user/commands.html).

Jupyter also supports code/output collapse, scrolling outputs, synchronized views and rich completion. For Aster, bounded output height and collapse are useful first; detached synchronized output windows can wait. Imported output needs sanitization, as in Jupyter's trust boundary. [Official notebook guide](https://jupyterlab.readthedocs.io/en/stable/user/notebook.html).

## SQL-specific adaptations and deferrals

Execution history should show the SQL, effective Data/Compute context, time and status. A stale-result marker should indicate edits made after a run. These are Aster recommendations inspired by notebook execution state, not claims that Jupyter provides SQL lineage. A future SQL inspection action should use supported EXPLAIN capabilities; Python breakpoint debugging is not a direct fit.

Defer arbitrary kernels, widgets, plugin execution, real-time simultaneous editing and presentation tooling. They expand runtime or concurrency boundaries beyond the present notebook UI problem. Keep current revision-conflict protection until a collaboration design is agreed.

Recommended first package: undo/redo, selected-cell clarity, heading outline and command palette. Then add find/replace and output controls. These changes make long notebooks easier to navigate without adding permanent cell buttons.


## Implementation stage — accepted

Human seed: “implement recomendations”. Delivery covers the seven recommendations summarized in chat: undo/redo operations, outline, command palette, find/replace, output controls, revision history/restore, and multi-cell operations. Duplicate/split/merge are included in recoverable editing. Rich SQL editor, interchange and focus mode remain later proposals.

Reuse the current task checkout. Changes reach browser actions/rendering/styles, real browser tests, the notebook storage port/Git adapter, protobuf history reads, authorized API handlers and ownership tests. No dependencies or schema migration are planned. History returns full historical notebook JSON to preserve cell metadata. The first draft proposed conditional REST restore; RV replaced that with a dedicated exact-revision RPC using the existing conditional storage path. Restore never refreshes/retries a stale revision. Team history uses the authorized workspace store. Undo retains cell nodes so existing output and assistant state survive; operation snapshots are bounded and do not roll back unrelated Data/Compute settings. Bulk actions preserve document order and never delete the final cell.

Tests first: missing command button and missing history endpoint assertions; then operation undo/redo, metadata preservation, range selection, Markdown boundaries, literal source replacement, output CSV quoting/truncation, ownership and revision validation, and stale restore refusal. Full Apollo/devenv CI and local Podman UI checks remain delivery gates. First-draft source baseline: `/tmp/aster-notebook-features-baseline/`. Independent plan review is in progress.


Implementation evidence update: independent review confirmed historical metadata, fixed restore preconditions, branch isolation, detached-cell targeting and source-preview invalidation as required boundaries. The initial ownership/history test failed at the missing endpoint; the browser test failed at the missing command entry. Receipts: `/tmp/aster-notebook-features-red.log`. Backend history ownership and historical metadata now pass in `/tmp/aster-notebook-features-green1.log`; browser and branch-isolation verification are ongoing. Undo retains at most 50 operations; history lists at most 50 notebook-changing commits. These bounds keep the first implementation explicit and bounded.


## RV revision and final gate

Frozen first draft: `/tmp/aster-notebook-features-draft1.tar`. Independent review found and source-verified four fixes: shared queue admission to prevent duplicate execution, preservation of legacy engines on duplicate/split, exact legacy context during restore, and assistant output context independent of collapse. A dedicated `RestoreNotebookRevision` RPC now reloads the historical document server-side and uses existing conditional-save/owner-update logic. This supersedes the initial plan to send historical JSON through ordinary REST save, whose omission-preserving behavior is correct for old clients but not exact restores.

Additional branch-isolation RED: `/tmp/aster-notebook-history-branch-red.log`; endpoint/legacy-engine RED: `/tmp/aster-notebook-features-rv-red.log`; collapsed-output RED: `/tmp/aster-notebook-features-output-red.log`; overlapping-queue RED: `/tmp/aster-notebook-features-queue-red2.log`. Two browser fixture defects were corrected without changing acceptance: explicit catalog/schema selection in size-based lists, and avoiding the shared `.out` class on both result container and prompt. No failing infrastructure check is counted as RED.

Final source review of the dedicated restore boundary found no further defect. Team/session/history tests and focused ownership tests have passed; expanded viewer-denial, stale restore and browser cases are in the final gate. `/tmp/aster-notebook-features-final-gate.log` and `/tmp/aster-notebook-features-image.log` are in progress. Deployment is gated on validation.

Expanded browser checks passed, including 390px command-dialog bounds: `/tmp/aster-notebook-features-mobile.log`. All four independent RV findings are closed by source review. Final full CI and image build remain in progress.


## Delivery result

All seven accepted capabilities are deployed. Full Apollo/devenv `just ci`, the expanded notebook browser suite and 37 registry browser flows passed. Independent RV findings are closed. Local health and six screenshot/smoke checks passed without JavaScript errors or notebook writes/query execution. [Delivery one-pager](../notebook-refinement.md) records exact receipts, limits and recovery. Earlier “in progress” entries above are retained as stage evidence, not current status.


## Feedback discovery — selection and shortcuts

Human seed, verbatim: “multiple selection does nothing. Commands button is strange. Lets explore keyboard shortcuts”.

Observed against localhost:8081: checking two cells produces two selected classes, but no selection count and no visible Run selected action. The first selected cell's down arrow swaps only that cell with its neighbor. No notebook writes or query executions occurred. Receipt: `/tmp/aster-selection-inspect.log`. Source confirms single-cell buttons call single-cell handlers, while bulk handlers are reachable only from the Commands dialog. This is an interaction/discoverability mismatch, not evidence that bulk handlers never work.

Proposed next behavior, not yet implemented or accepted: show a compact contextual selection bar with count, Run selected, up/down, duplicate, delete and clear selection. Keep per-cell actions explicitly single-cell. Replace the prominent Commands button with a small shortcut-help entry; retain Find action in notebook overflow and Ctrl/Cmd+Shift+P.

Use two explicit focus scopes inspired by Jupyter: editing preserves native text shortcuts and assistant-input behavior; Escape enters cell navigation, Enter edits the active cell. Cell navigation uses arrows to select, Shift+arrows to extend, Ctrl/Cmd+Shift+arrows to move the selection, Ctrl/Cmd+Enter to run the selection, A/B to insert SQL above/below, Z/Shift+Z for operation undo/redo, and ? for shortcut help. Shift+Enter runs then advances beyond the selected range. Text editing keeps existing Ctrl/Cmd+Enter, Shift+Enter and save behavior scoped to the editor. Opening a dialog or editing a text field must disable cell-level single-letter handlers. No Y/M cell-type bindings: SQL/%md remains authoritative.

Prefer this small set over copying every Jupyter binding; defer shortcut customization and clipboard/cut commands until the basic model is clear. Before implementation, test mouse/keyboard selection parity, editor/chat isolation, autocomplete Escape priority, repeat-key behavior, focus restoration, shortcut discoverability and browser/OS conflicts. Current verification was Chromium only. No application code or deployment changed in this exploration.

Reference: [Jupyter edit and command modes](https://jupyterlab.readthedocs.io/en/stable/user/notebook.html#edit-mode-vs-command-mode), [shared command/palette design](https://jupyterlab.readthedocs.io/en/stable/user/commands.html#command-palette).


## Keyboard/selection implementation — accepted

Acceptance, verbatim: “implement”. Scope is the preceding feedback proposal: visible contextual selection actions, shortcut help and overflow action search, explicit cell navigation versus editing, and the listed keyboard bindings. Existing text/editor/chat shortcuts and SQL/%md behavior remain authoritative. Implementation reaches the shared browser handlers/styles and real-server browser fixture; no API or data-format changes. First-draft baseline: `/tmp/aster-keyboard-baseline/`. Tests cover visible bulk movement/duplicate actions, keyboard parity, source-field isolation, autocomplete Escape priority, focus, repeat suppression and mobile bounds. Apollo/devenv tests and localhost:8081 deployment remain authorized.


Keyboard implementation evidence: `/tmp/aster-keyboard-red.log` reproduces Escape failing to leave the editor. Independent review identified startup parsing, asynchronous run focus and stale completion boundaries; the naming collision was fixed and held-response regressions reproduced both timing defects in `/tmp/aster-keyboard-focus-red.log` and `/tmp/aster-keyboard-completion-red.log`. The palette's native search input consumed Escape; explicit dialog dismissal fixed the reproduced failure in `/tmp/aster-keyboard-async-red.log`. Browser checks also exposed selection-bar layout movement during mouse clicks. The final draft uses a floating bottom bar, avoiding document reflow instead of special-casing focus timing. Source review closed the independent findings; final browser/full-CI and local deployment gates remain pending.


Final keyboard browser gate passed: `/tmp/aster-keyboard-green6.log` contains `NOTEBOOK_KEYBOARD_BROWSER_OK`, `NOTEBOOK_FEATURES_BROWSER_OK` and the existing AI/session browser completion marker. Earlier regression repairs remain in the numbered receipts. A final integration case showed native dialog closure resetting the split cursor; source/selection capture now restores the unchanged editor range before command dispatch. Independent Chromium reproduction confirmed that behavior. Rapid palette reopening ignores closed dialogs awaiting removal. Temporary diagnostic prints were removed. Full CI and image build are running in `/tmp/aster-keyboard-final-gate.log` and `/tmp/aster-keyboard-image.log`; local deployment is still gated on those outcomes.


## Keyboard delivery result

Accepted selection and shortcut improvements are deployed at localhost:8081. Full Apollo/devenv `just ci`, 37 registry flows and the keyboard/notebook/AI browser suites passed. Independent findings are closed. Local smoke passed movement/undo, shortcut help, overflow and 390px bounds, producing five screenshots with zero JavaScript errors and no notebook saves or query requests. [Current delivery report](../notebook-refinement.md) records image identity, receipts, recovery and Chromium-only limits. Earlier pending entries are historical stage evidence, not current status.
