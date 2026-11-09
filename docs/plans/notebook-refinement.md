# Notebook selection and keyboard navigation

## Outcome and scope

Human seed, verbatim: “multiple selection does nothing. Commands button is strange. Lets explore keyboard shortcuts”. Acceptance, verbatim: “implement”. The accepted change makes selected-cell actions visible and separates cell navigation from text editing. Existing SQL/%md creation, notebook-level Data, assistants, models and earlier editing capabilities remain supported. Checkout: `aster/worktrees/ai-registry-plan`, branch `feat/ai-registry-plan`; unrelated changes are preserved. No commits or pushes were made.

## Behavior and implementation

Selecting cells now shows a floating bottom toolbar with the selected count, Run selected, move up/down, Duplicate, Delete and Clear selection. Its fixed position avoids shifting the document during checkbox or editor clicks. Per-cell controls continue to act on that cell. The prominent Commands button is replaced by Shortcuts and a notebook overflow menu. Find action remains available there and through Ctrl/Cmd+Shift+P; outline, source search and history remain accessible.

Escape leaves the editor for cell navigation; Enter edits the active cell. Arrows select, Shift+arrows extend selection, and Ctrl/Cmd+Shift+arrows move it. Ctrl/Cmd+Enter runs selected cells; Shift+Enter runs and advances beyond the selected range. A/B insert SQL above/below, Z/Shift+Z undo/redo cell operations, and ? opens help. Single-letter actions apply only on the cell navigation surface. Editor, assistant and dialog typing retain their own behavior; autocomplete receives the first Escape. Holding insertion keys does not create repeated cells.

Changes use shared browser handlers and styles, with no new dependency, API or data-format change. The existing real-server browser fixture exercises both new behavior and prior notebook/AI capabilities. The appended feature-file scenarios describe behavior; they are not independently bound automated tests.

## Review and evidence

First draft: `/tmp/aster-keyboard-draft1.tar`; baseline: `/tmp/aster-keyboard-baseline/`. Independent review found a startup naming collision, delayed execution stealing focus, and stale autocomplete reopening after navigation. Verified fixes preserve newer focus/selection and invalidate old completion responses. Additional browser failures exposed modal Escape handling, toolbar reflow during clicks and native dialog closure resetting the cursor before Split. The final implementation explicitly closes dialogs and restores an unchanged editor selection before dispatch. Independent Chromium reproduction confirmed the cursor issue; final source review left no verified finding open.

RED receipts include `/tmp/aster-keyboard-red.log`, `/tmp/aster-keyboard-focus-red.log`, `/tmp/aster-keyboard-completion-red.log` and `/tmp/aster-keyboard-async-red.log`. Numbered repair receipts retain failures without changing acceptance. `/tmp/aster-keyboard-green6.log` passes the keyboard, existing notebook features and AI/session browser suites, including controlled delayed responses and 390px bounds. Temporary diagnostics were removed. Local and Apollo production hashes match; `git diff --check` passed.

## Deployment and recovery

Full Apollo/devenv `just ci` passed (format, Clippy, workspace tests, behavior features, repository/provider and local-context checks), together with 37 registry browser flows. Receipt: `/tmp/aster-keyboard-final-gate.log`. Server and controller now run image `eea64965012973c4d77c66ff61ee6edbe9957c0a2389a44bd07cb6c4c0af545f` at localhost:8081; health returned `ok`. Deployment receipt: `/tmp/aster-keyboard-deploy.log`. Writers were stopped for coordinated notebook and metadata backups; the private backup path is recorded in `/tmp/aster-notebook-keyboard-backup-path`. The previous image is retained as `localhost/aster-server:pre-notebook-keyboard`.

Local smoke passed selection movement/undo, help, overflow and 390px layout with zero JavaScript errors and zero notebook saves/query requests: `/tmp/aster-keyboard-local.log`. Five [screenshots](notebook-refinement/index.html#keyboard-selection) document the deployed UI; desktop selection/help and mobile layout were visually inspected. No required delivery work remains.

## Limits

Browser validation uses Chromium; native macOS and other browser shortcut conflicts have not been checked. Shortcut customization and clipboard/cut commands remain deferred. Operation undo remains page-local; stopping a queue does not cancel its in-flight query. Historical scope and receipts remain in the [feature record](notebook-refinement/feature-set.md); the previous delivery report is retained at `/tmp/aster-notebook-refinement-before-keyboard.md`.

## Mock assistant E2E — deployed and verified

Human seed, verbatim: “create a mock assistant and do a end to end asking for a query generations, sanding it to query engine and using skills on aster”. Clarification: “Keep it available at localhost:8081”. Scope: a deterministic local assistant, versioned instructions/reference file, browser generation, explicit insertion and execution on the existing synthetic engine, and a retained demonstration notebook.

The existing stack only allowed LiteLLM destinations. `ASTER_MOCK_ASSISTANT=1` adds exactly one loopback URL for personal models under isolated development identity; SSO refuses this option and HTTPS allowlists remain unchanged. The mock service shares the Aster server network namespace and receives no standing credentials. No inference, external tool execution or production queries are involved.

RED: `/tmp/aster-mock-assistant-red.log` records an assertion failure admitting the exact demo destination; `/tmp/aster-mock-assistant-live-red.log` records the deployed API refusing mock registration before deployment. The focused Rust test now passes, including alternate URL/port/path/credentials refusals and non-development refusal. A fixture self-check verifies both instruction and reference markers are required in system context; user text alone cannot trigger the successful fixture response.

Independent review found no production boundary defect. It identified two evidence gaps, both repaired: explicitly select version 1 rather than the latest skill version, and await a successful save plus read back persisted SQL. Full Apollo/devenv `just ci` and the image build passed. Live `devenv --no-tui shell -- just mock-assistant-e2e` passed against localhost:8081. The runner creates an isolated demo notebook while preserving existing notebooks, skills and defaults. It asserts zero query requests during generation and insertion, then exactly one request with the generated SQL after Run.

Recovery: previous image retained as `localhost/aster-server:pre-mock-assistant`; deployment stopped writers for private notebook/metadata backups; `/tmp/aster-mock-assistant-backup-path` records the backup location. Limits: mock model output is deterministic and mock engine rows are synthetic, so this validates application plumbing rather than SQL evaluation or model quality.

Live outcome: skill-off guidance requested enabling the skill; selecting version 1 sent both instructions and `references/orders.md`. The assistant returned fenced SQL without execution. Explicit insertion sent no query; Run sent exactly one request with matching SQL and engine `mock-local`, returning six synthetic rows. A successful notebook PUT and subsequent GET proved persisted SQL. Conversation history retained the exact skill ID/revision; there were zero JavaScript errors.

Model `personal/mock-assistant`, private skill `Demo orders SQL` version 1, and the [demo notebook](http://localhost:8081/notebooks/mock-assistant-demo-1794181671) remain available for development user Alice. [Screenshots and receipt](notebook-refinement/index.html#mock-assistant) record the observed flow. Server/controller image: `09b272bc65272ee1d63e3b9145d4864633170e8aca80647f901b3d62b076b7cd`; mock service running; health `ok`. Receipts: `/tmp/aster-mock-assistant-gate.log`, `/tmp/aster-mock-assistant-deploy.log`, `/tmp/aster-mock-assistant-live.log`. Independent review closed both evidence gaps; no required work remains. No real model inference, SQL semantics or non-Chromium browser coverage is claimed.

## Assistant transfer hardening — deployed and verified

Human seed: “in the assistant we should have the capacity to send query to cell.” Follow-up: “should we do any hardening?” Acceptance: “do all”. Accepted scope: recoverable SQL replacement, explicit destinations without last-cell fallback, independent controls for every SQL block, Markdown preservation, and regression coverage proving transfer never executes a query. Existing backend execution authorization remains authoritative.

A shared renderer now emits inert prose, SQL previews and per-query destination controls for both assistants. The destination retains its cell identity across focus changes. Deleted targets refuse mutation; changed position or type refreshes the displayed destination/action and requires another click. Markdown receives a SQL cell after it. New-cell creation and replacement use existing operation Undo/Redo; running destinations refuse transfer. The mock E2E selects its destination explicitly.

RED: `/tmp/aster-transfer-red.log` observed one action group for a two-query reply. Initial browser GREEN: `/tmp/aster-transfer-green.log`, including existing keyboard, notebook and AI/session flows. Independent review identified stale destination labels after reordering and stale Create actions after Markdown-to-SQL conversion; these are covered by added regression cases. Review RED `/tmp/aster-transfer-review-red.log` reproduced silent transfer after reordering. Final notebook browser regression passed in `/tmp/aster-transfer-final.log`, including both assistant scopes and stale type changes. Independent review closed both findings by source. The separate conversation suite, full CI, local deployment and persisted mock E2E remain required gates. Reviewed source snapshot: `/tmp/aster-transfer-reviewed.tar`.

Additional integration evidence: the standalone conversation fixture was stale after earlier registry/session/UI changes. Repair attempts in `/tmp/aster-transfer-final2.log` through `final5.log` updated development-only model admission, visible model selection, status/action labels, current conversation state, and empty-state assertions. The same authorization, no-execution, stale-reply, inert-rendering and draft-preservation requirements remain; only obsolete fixture assumptions changed. `conversation-browser OK` now appears in `/tmp/aster-transfer-final6.log`, followed by the full CI run. These setup failures are not production RED evidence.

Final delivery: full Apollo/devenv `just ci` passed in `/tmp/aster-transfer-final6.log`; the standalone conversation suite and notebook keyboard/features/AI/transfer browser suites passed. Review also confirmed that fixture updates preserve security assertions. Earlier pending stage entries above are historical.

Local deployment stopped writers for coordinated backup, retained the previous image as `localhost/aster-server:pre-assistant-transfer`, and restarted server/controller plus both loopback fixtures. Backup receipt: `/tmp/aster-assistant-transfer-backup-path`; deployment log: `/tmp/aster-transfer-deploy.log`. Server/controller image `b71a021c38a5c4e654bf8aa84fab5b6f9a6a8de8fe5cd3747d5e57dd7a21f022` is running and health returned `ok`.

`/tmp/aster-transfer-live.log` records the deployed transfer regression and `just mock-assistant-e2e` passing. Multiple blocks, focus changes, actual reordering, deletion, Markdown preservation, stale Create actions and Undo/Redo passed without query dispatch. The separate mock flow selected skill v1, generated SQL, chose its destination, inserted without execution and sent exactly one matching request after Run; six synthetic rows returned, saved SQL was read back, and there were no JavaScript errors. [Screenshots and receipt](notebook-refinement/index.html#assistant-transfer) document the deployed UI. No required work remains.

Limits: operation Undo/Redo is page-local; browser coverage is Chromium; mock results do not establish real inference or SQL semantics. The standalone conversation fixture uses fixed development port 7778 and requires network isolation for parallel instances. No commits or pushes were made.

## PR handoff

The user requested “create prs”. Implementation and operational handbooks are published in [PR #4](https://github.com/ErikBPF/aster/pull/4), branch `feat/ai-registry-plan`, targeting `main`. The companion `docs/aster-ai-notebook-evidence` branch targets that implementation branch and adds design/review records, screenshots and their handbook links. Shared API/UI changes remain together as the tested implementation; no untested intermediate application states were manufactured for the split.

Validation evidence above applies to the published code. Packaging additionally passed staged whitespace checks and relative-link checks for every new Markdown/HTML evidence artifact. Earlier source-review findings are closed. Hosted checks are separate from the observed Apollo/local gates. Both PRs remain unmerged; land implementation first, then retarget the documentation PR to `main`.
