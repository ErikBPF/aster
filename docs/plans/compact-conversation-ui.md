# Compact conversation UI

## Accepted outcome

Human seed: “open operwebui againt and take some prints on how it is done on a conversation. Functionality is ok, but ui is too clutered”. User accepted the recommended layout with “do as recomended”.

Deployed to localhost:8081: a bottom-anchored message composer with Attach, Skills (count), and Tools (count). Searchable popovers hold selectors, versions and refresh. Attached context has a removable chip. Confirmation remains visible outside the popovers. Notebook and cell chats share controls; defaults, pinned versions, ACLs and execution semantics remain unchanged.

## Delivery scope

Implemented in the existing `feat/ai-registry-plan` worktree. Changes reach app.js shared controls and both composers, app.css layout, browser tests and the explicitly unautomated behavior contract. No backend, schema or dependency change was needed. Reference captures and rationale are in [the conversation comparison](openwebui-reference/conversation/README.md).

## Verification and recovery

Browser checks cover compact controls, full composer visibility, search, Escape/focus, explicit payloads and attachment removal. Real notebook/cell tests open selectors and retain confirmation/resume assertions. The prior `notebook-ai-controls` image and `compose.before-compact-conversation.yaml` remain available for rollback; no database changes are needed.

## First draft and independent review

First draft preserved in `/tmp/aster-compact-ui-draft1.tar.gz`. The new compact-control assertion failed against the old UI (`/tmp/aster-compact-ui-red.log`). The first GREEN run exposed two existing test selectors that needed the new open-menu step or an unambiguous defaults-search label; their behavioral assertions remain intact.

Independent reviewer `/root/compact_composer_review` found two verified regressions. Attachment editing stayed disabled after cancelling a cell MCP turn; the real browser failed on the disabled textarea (`/tmp/aster-compact-resume-red.log`). Rendering now synchronizes all controls through the existing busy-state function. The same notebook/cell test passed afterward (`/tmp/aster-compact-resume-green.log`). Pending confirmations clipped Send on short screens; a 390×500 assertion failed with only half the button in view (`/tmp/aster-compact-viewport-red.log`). A shared scroll wrapper now contains sessions, history, status and confirmation while the heading and composer remain fixed. The reviewer independently verified Send at 390×500, 600 and 700 (`/tmp/aster-compact-review-final.log` on Apollo).

All 36 registry browser flows and the actual notebook/cell browser passed after both fixes (`/tmp/aster-compact-review-green.log`). Exact pins, explicit off payloads, defaults, confirmation reload, cancellation and continuation remain covered. Keyboard Escape closes menus without closing the notebook panel and restores trigger focus. Review reports no remaining scoped blockers.

## Deployed verification and limits

Apollo `just ci` passed, including formatting, Clippy, workspace tests and repository checks (`/tmp/aster-compact-ci.log`). `just ai-registry-browser` passed 36 flows and `just notebook-ai-browser` passed the actual app against controlled model/MCP endpoints. All five changed implementation/test files matched local/Apollo hashes. The release image built successfully (`/tmp/aster-compact-image.log`).

The local server and controller both run `localhost/aster-server:compact-conversation`, image `d392db406b02651ece0306f2035c9d98d5ec9119023fe20672d789fd04787a64`; health returned 200. Deployed Playwright checks passed for both scopes, menu search/counts, Escape, attachment query/summary fetch and removal, and narrow viewport visibility (`/tmp/aster-compact-local.log`). [Six deployed screenshots](compact-conversation-ui/index.html) show the result. No inference or changing tool action was dispatched locally.

No required gate remains unmet; no commits or pushes were made. Live LiteLLM inference remains unavailable on the local account; controlled model/MCP endpoints cover behavior. Browser evidence covers Chromium, not other engines.
