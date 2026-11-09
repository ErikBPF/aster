# Open WebUI-inspired AI workspace — delivery record

## Outcome and accepted behavior

Human seed: “implement improvements”. Reference: [PL](openwebui-skills-mcp-pl.md)
and [actual UI captures](openwebui-reference/README.md). Preserve existing user
work, immutable skill versions, explicit conversation upgrades, current ACLs,
and text-only skills. User accepted admin-managed remote MCP with user/team
grants, then “Automatically run admin-approved reads; confirm changing actions”
in question `call_43d4c7b24b484ac58cef1c9d963d3599` during this checkout's IP.

## Vertical delivery and evidence

1. Skills workspace: searchable rows with description, owner and scope;
   file navigation alongside instructions/reference editing; separate accessible
   sharing dialog; safe nested text paths; local Markdown/JSON package import
   into an unsaved private draft and JSON export of a selected saved version.
   Keep 32 KiB content and 16-reference bounds. No ZIP, URL fetching, binary
   attachments or bundled execution. Preserve source revision on export, never
   import IDs, ownership, permissions or revision counters. Owner: core skills,
   server handlers/Pg adapter/proto, app.js/app.css. RED: API round trip of
   description/nested paths and browser search/import/export/navigation flows.
   Commands: `just ai-registry-test` → `AI_REGISTRY_TEST_OK`;
   `just ai-registry-browser` → `AI_REGISTRY_BROWSER_OK`.
2. MCP registry: admin-only connection management; fresh user/team admission;
   disabled connections cannot be discovered or used; credentials encrypted and
   never returned. Metadata memory and PostgreSQL providers remain explicit.
   Endpoint destinations are separately approved; LiteLLM model allowlisting
   does not approve tool servers. Only remote Streamable HTTP, no stdio.
3. MCP use: distinguish explicit admin-approved read tools from all other tools.
   Server annotations cannot relax policy. Changing calls require a confirmation
   bound to the exact user, connection revision, tool and arguments; replay and
   revoked access fail before outbound I/O. Bound discovery, call time and output;
   isolate sessions. RED seams: router denial/confirmation regressions plus a
   disposable local MCP server; no real external mutations in verification.

## Review, rollout and recovery

Independent plan review precedes implementation; independent final review checks
acceptance and changes. Bounded grill: imported files must not collide with each
other or SKILL.md, asynchronous requests must not overwrite a newer draft,
historical views cannot become writable, and pending confirmations cannot
survive changed grants. Reuse existing widgets, encryption and identity helpers;
do not introduce a second authorization engine.

All builds/tests run on Apollo in the declared devenv. Final `just ci` must exit
zero with `CI GREEN`; real browser/PostgreSQL restart gate must print
`AI_REGISTRY_LIVE_BROWSER_OK`. Source receipts distinguish mocked UI, router
tests and actual runtime behavior. Deployment scope remains local Podman
localhost:8081; preserve its database and current image for rollback. Open WebUI
reference remains separate on 8082. No commits, pushes or cluster deployments.

The first draft is preserved in the source receipt
`/tmp/aster-openwebui-draft-1.tar.gz`. The sections below record review
reconciliation; final observed results belong in the RV record.

## Revision 2 — review reconciliation and execution boundary

User requested “add a donsetch as a mock one”. Donsetch is a synthetic local MCP
server with `inspect_catalog` and `draft_note`; the latter records only a mock
receipt. It is not a real external service or a model gateway. First MCP UI is an
explicit tool explorer in Connections; automatic notebook model tool selection
and progressive skill loading remain separate follow-up behavior. The explorer
runs admin-approved reads without another confirmation, while every other tool
requires visible confirmation. No inference call is needed to exercise it.

Independent review identified two editor defects. Assertion-failing browser
regressions reproduced delayed reads overwriting edits and hidden invalid files
blocking submission. Repairs preserve newer drafts, avoid the post-save reload,
and report validation errors explicitly. Sixteen browser flows passed at that checkpoint on
Apollo; the final regression suite expanded to 21 flows. Source/API RED: nested references returned 400; metadata/reference API
regression now passes. MCP list RED: absent RPC returned 404.

MCP confirmation design: metadata store owns single-use entries containing user,
SHA-256 binding of connection revision/tool definition/arguments, and five-minute
expiry. PostgreSQL consumes atomically with DELETE predicates; memory consumes
under a write lock. Changes to grants increment the same connection revision.
Admin read approval binds to a freshly discovered tool-definition hash; endpoint
or credential changes clear approvals. Tool annotations grant no authority.
Each call rechecks current membership and revision immediately before outbound
execution. Changing requests are never retried automatically after uncertainty.

Transport scope: Streamable HTTP 2025-03-26, JSON and SSE responses, bounded pages,
100 tools, 256 KiB responses and 20-second operations. No stdio, OAuth, sampling,
elicitation, resource fetching or MCP Apps. HTTP is the only transport adapter selected centrally; tests use a disposable
HTTP fixture. Donsetch uses an
explicit loopback-only development exception, rejected with production identity.

Gates: `just ai-registry-test` includes MCP router/confirmation regressions;
`just ai-registry-browser` covers visible confirmation and cancellation;
`just ai-registry-postgres-test` covers encrypted registry and confirmation
restart/consume behavior; `just ai-registry-live-browser` covers real UI/runtime.
The live mock adds no authority for external systems. Final RV must verify these
claims against actual receipts; this section records intended limits until green.

## Final review

Observed delivery, review dispositions and recovery are recorded in
[the RV one-pager](openwebui-improvements-rv.md). The
[deployed UI gallery](openwebui-improvements/index.html) contains synthetic demo
content captured from the authorized localhost:8081 stack.
