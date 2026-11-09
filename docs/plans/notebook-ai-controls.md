# Notebook AI controls — delivery and review

## Outcome and accepted behavior

Human seed, verbatim: “in notebooks lets do the same check on how to enable/disable skills and mcps and use it here. Lets also add on use configs the possibility to make skill on/of by default”.

The user accepted the recommendations with “do as recomended”. Notebook and cell chats now support independently enabled skills with exact version pins. **My defaults** in AI settings saves personal selections for new/empty conversations; existing selections never silently upgrade. MCP connections start off and require explicit selection. Admin-approved reads run automatically; changing actions require confirmation of their exact arguments.

Implementation belongs to `feat/ai-registry-plan` in `worktrees/ai-registry-plan`. Existing unrelated changes remain intact. Builds and test suites run on Apollo through declared devenv. The authorized deployment target is the local Podman stack at localhost:8081.

## Changes and boundaries

Memory and PostgreSQL providers store per-subject defaults with revision checks. Presence-aware requests distinguish omitted/inherit from explicit empty/off and preserve the legacy single-skill API. Shared notebook/cell controls support multiple version selectors and late/default-fetch recovery.

Notebook MCP execution reserves persisted state before dispatch. Confirmation binds the authenticated user, conversation, session, random turn identity, server/tool definition and exact arguments. Standalone explorer calls cannot consume notebook confirmations. Concurrent resumes cannot dispatch twice; unknown outcomes are never automatically retried. Cancel clears continuation state but cannot undo completed external effects. Historical skill, MCP and contract access is rechecked before reuse.

Execution is bounded: eight skills, four connections, 32 tools, four calls, 32 KiB tool results and five-minute continuations. Skills remain instructions/reference files and confer no tool permission. No dependencies were added.

## Review and repair evidence

First draft: `/tmp/aster-notebook-ai-draft1.tar.gz`; discovery record: `/tmp/aster-notebook-ai-discovery.md`. Initial assertion-failing REDs cover missing defaults, dispatch and UI controls (`/tmp/aster-notebook-defaults-red.log`, `/tmp/aster-notebook-mcp-red.log`, `/tmp/aster-defaults-ui-red.log`, `/tmp/aster-notebook-mcp-ui-red.log`).

Independent review found protected-context continuation failure, stuck cell Send, and failed defaults reload recovery. Reproductions and fixes are recorded in `/tmp/aster-notebook-review-{backend,ui,cell}-red.log`. Further regressions reproduced explorer confirmation replay and capability exposure in Debug; fixes scope fingerprints and redact arguments/tokens (`/tmp/aster-notebook-confirmation-scope-red.log`, `/tmp/aster-notebook-debug-{red,green}.log`). Full CI also caught denied first requests creating empty conversations; fresh model admission now precedes creation (`/tmp/aster-notebook-ai-final.log`).

An intermittent catalog test failure exposed an unsynchronized response-capture boundary. The test now drains authorized responses before resetting receipts and asserts a positive control; revocation assertions remain unchanged. Three diagnostic runs passed without reproducing the precise callback ordering. This was a source-verified harness repair, not a catalog authorization change.

## Validation, deployment and limits

`just ci` passed on Apollo, including formatting, Clippy, workspace tests and repository checks. All 35 registry browser flows passed separately (`/tmp/aster-notebook-ai-ui-release.log`). `just conversation-postgres`, `just ai-registry-live-browser` and `just notebook-ai-browser` also passed (`/tmp/aster-notebook-ai-final-gate.log`, exit 0).

The real-app notebook browser uses controlled model/MCP endpoints to verify defaults, toggles, persisted confirmation, cancellation and continuation. New Gherkin scenarios remain explicitly unautomated; Rust/browser tests supply the executable evidence.

Deployed and healthy at localhost:8081; server/controller image IDs match the validated build. Local Playwright verified persisted defaults, multiple toggles, MCP initially off, explicit all-off and disabled Send without a model (`/tmp/aster-notebook-ai-local.log`; screenshot `/tmp/aster-notebook-ai-local.png`). Prior defaults were restored and temporary skills deleted; one empty private chat session remains. No required gate remains unmet. The image is `localhost/aster-server:notebook-ai-controls`, ID `5a9634f2691efffc765fc7fdca3cff038cfd12873ca9335e9027459f9218a7ad`. Previous image, Compose override and mode-0600 database dump are preserved. Schema additions are additive; resolve pending turns before downgrading because older code does not enforce pending state.

Live LiteLLM inference remains unverified: the local account has no registered model. No real changing MCP action, commit or push was performed.
