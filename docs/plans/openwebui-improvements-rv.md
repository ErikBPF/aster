# AI workspace and Donsetch — review and delivery

## Outcome and why

The AI workspace now has a searchable skills library, descriptions, a file-based
editor, separate sharing controls, and portable text packages. Connections adds
an admin-managed remote MCP registry and an explicit tool explorer. This follows
the user's “implement improvements” seed, the Open WebUI reference investigation,
and “add a donsetch as a mock one”. The accepted execution rule remains:
“Automatically run admin-approved reads; confirm changing actions”. See the
[delivery record](openwebui-improvements-ip.md) and
[reference captures](openwebui-reference/README.md).

Donsetch is a synthetic server: `inspect_catalog` reports fixture tables;
`draft_note` returns a simulated receipt. Neither accesses external systems.
The current MCP integration is the Connections explorer, not automatic notebook
model tool selection. Skills remain instructions and references; they cannot
execute bundled code or grant tool access. Existing version pins and explicit
upgrades remain intact.

## Changes and verified review findings

Independent UI/conformance and security reviews inspected the implementation.
Verified regressions covered delayed reads overwriting skill edits, hidden invalid
reference fields blocking useful validation, delayed connection saves replacing
newer drafts, stale tool responses after navigation, and old tools remaining
available while another connection loaded. Repairs preserve draft state and
invalidate stale requests. A separate round-trip regression covered JSON escaping
making a valid exported package exceed the import envelope; serialized imports
allow 256 KiB while decoded content remains bounded to 32 KiB.

Security review reproduced a revoked connection still executing after delayed
rediscovery. The transport now checks current authority and connection revision
immediately before dispatch. Confirmations bind user, connection revision, tool
definition and exact arguments, expire after five minutes, and are consumed
atomically. Read approvals bind the discovered tool definition. Credentials are
encrypted with separate MCP keys and never returned to the browser. Failed
initialization releases known sessions; uncertain action outcomes remain visible
and are never automatically retried.

The second draft removes the redundant editor reload and separates connection
registration from tool exploration. HTTP is the only transport adapter; tests
use disposable HTTP fixtures. No extra transport abstraction or dependency was
added for a hypothetical provider.

## Evidence and limits

Apollo runs the repository's declared devenv. The final browser suite passed 21
flows, and the real browser/PostgreSQL test passed skills, sharing, model policy,
MCP reads, confirmations, encryption and restart/replay checks. Focused router
checks passed seven tests; four PostgreSQL tests passed, including concurrent
single-use consumption. A transport regression passed cleanup and uncertain
outcomes. Gherkin files marked unautomated are behavior records, not claimed
executed scenarios.

First-draft receipt: `/tmp/aster-openwebui-draft-1.tar.gz`. RED logs are retained
under `/tmp/aster-openwebui-*-red.log` and `/tmp/aster-mcp-*-red.log`;
`/tmp/aster-skill-roundtrip-red.log` records the import/export assertion. Final
validation and image logs are `/tmp/aster-openwebui-final-validation.log` and
`/tmp/aster-openwebui-image-final.log`. Final gate and deployment results are
recorded below only after observation.

## Decisions, recovery and continuing work

Deployment authorization covers local Podman localhost:8081. Open WebUI remains
separate on 8082. Donsetch uses an explicit development-only loopback exception,
refused with production identity. Remote MCP currently supports Streamable HTTP
2025-03-26 with JSON/SSE, bounded discovery/results and timeouts; stdio, OAuth,
MCP Apps and autonomous notebook tool selection remain outside this delivery.

The prior image is retained as `localhost/aster-server:pre-openwebui-improvements`.
The deployment directory retains a mode-0600 database dump and pre-change config
backups. Keep encryption keys during rollback; additive MCP tables can remain.
No commits, pushes, cluster deployment or external mutating MCP actions occurred.

## Final observed gates

- Apollo `just ci`: `CI GREEN` (format, Clippy, workspace tests, features,
  repository, providers and local-context checks). Final browser and real-runtime
  checks passed in the same validation run. No required gate remains unmet.
- All 55 paths in `/tmp/aster-openwebui-source-final.sha256` matched Apollo.
  `git diff --check` passed. Server and controller run image
  `3244340d4c62c1dc61a0daaa5e285625dae1c46dbd70e4172e7a3203f09dcc39`.
- Local Compose deployment succeeded; `/healthz` returned `ok`. Donsetch runs in
  the server network namespace. Only `inspect_catalog` was approved as a read;
  `draft_note` still requires confirmation. Access is granted to demo user alice.
- Local Chromium reported `LOCAL_OPENWEBUI_OK`: skill editor, mock read,
  confirmation, cancellation, replay denial, ACL denial and screenshots. See
  [the deployed gallery](openwebui-improvements/index.html).
  `/tmp/aster-local-openwebui-check.log` retains the receipt. No real external
  MCP endpoint was tested; the mock establishes local execution behavior only.

## Follow-up — explicit skill actions

Human correction: “skill needs buttons for edit, manage versions and permissions”.
Each library row now exposes those three actions. Edit opens the latest version
and focuses its name; non-owners cannot use it. Manage versions opens a labelled
history dialog with current/read-only indicators and a button for each saved
version. Permissions opens the existing sharing controls against current
metadata. The editor also exposes Manage versions and Permissions. No version
deletion, permission widening, or automatic pin changes were introduced.

The new browser regression failed on the missing Edit button before production
changes (`/tmp/aster-skill-actions-red.log`), then passed navigation, historical
read-only behavior, latest sharing controls and non-owner edit denial. The full
browser suite passed 22 flows (`/tmp/aster-skill-actions-green.log`). An existing
sharing test needed its locator scoped to the editor because Permissions now
also exists in the library; its sharing assertions remain unchanged. Runtime,
CI and deployment receipts will follow after observation.

Follow-up gates passed: Apollo `just ai-registry-live-browser` reported
`AI_REGISTRY_LIVE_BROWSER_OK`; `just ci` reported `CI GREEN` in
`/tmp/aster-skill-actions-validation.log`. Five changed source/test paths matched
Apollo (`/tmp/aster-skill-actions-source.sha256`), and `git diff --check` passed.
The deployed server/controller image is now
`4b7e1e09f743cfe105c2ebd72e023887b926899cfc476707e78e34b3cc77053a`.
Local Chromium verified all three actions and captured the
[skill library](openwebui-improvements/skills-actions.png) and
[version dialog](openwebui-improvements/skill-versions.png):
`SKILL_ACTIONS_LOCAL_OK` in `/tmp/aster-skill-actions-local.log`.

The first Compose replacement was blocked by Donsetch's shared network namespace.
Removing and recreating only that disposable sidecar allowed replacement; the
Compose temporary server/controller names were restored. Health and an approved
Donsetch read both passed afterward. Future replacements must remove this sidecar
before replacing the server, then recreate it against the new namespace. Previous
image `localhost/aster-server:pre-skill-actions` and override
`compose.before-skill-actions.yaml` remain available for rollback. No required
follow-up gate remains unmet; no commits or pushes were made.

## Superseding skill-management refinement

The November 8 follow-up consolidates version history and deletion inside Edit
and clarifies reference file paths. Current behavior and verification are in
[skill-management RV](skill-management-rv.md); older screenshots and button
layouts above are historical receipts, not the current interface.
