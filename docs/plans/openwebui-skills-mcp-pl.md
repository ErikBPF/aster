# Open WebUI skills and MCP adaptation — PL

## Outcome and seed

Human seed, verbatim: “lets check openwebui skills and mcp structure and lets base on it”.

Use Open WebUI as the structural reference for Aster's AI workspace. Preserve
Aster's accepted version pins, explicit upgrades, team sharing, admin-only
organization publication, and non-executable skills. The user additionally
accepted “Admin-managed remote MCP (Recommended)”: administrators register
remote HTTP servers and grant access to users or teams. A skill cannot grant
tool permissions. This is a planning record, not a deployed capability.

Feedback: `call_388e2340f59c44b6947a2d80bf97ec1f`, question 0, answered against
checkout `worktrees/ai-registry-plan` during discovery revision 1. Registration
scope is resolved; no review session was attached.

## Source comparison

Inspected upstream commit `c38885a1a737da4968f157092f304f3b5481d0bf`:

- [Skill files](https://github.com/open-webui/open-webui/blob/c38885a1a737da4968f157092f304f3b5481d0bf/backend/open_webui/utils/skill_files.py)
  require root `SKILL.md`, validate relative paths, support file manifests,
  import/export and bounded reads. Open WebUI supports binary assets; Aster's
  initial accepted scope remains text instructions and reference files.
- [Skill routes](https://github.com/open-webui/open-webui/blob/c38885a1a737da4968f157092f304f3b5481d0bf/backend/open_webui/routers/skills.py)
  separate content, access grants, history, import preview and version selection.
- [MCP client](https://github.com/open-webui/open-webui/blob/c38885a1a737da4968f157092f304f3b5481d0bf/backend/open_webui/utils/mcp/client.py)
  manages Streamable HTTP sessions, authentication, tool discovery and calls.
  [MCP administration](https://docs.openwebui.com/features/extensibility/mcp/)
  restricts registration to admins and scopes use through access controls.

Aster's `crates/core/src/skills.rs` already stores immutable revisions and current
sharing policy, but references use flat filenames and there is no portable
package metadata. `crates/server/src/conversations.rs` injects selected content;
it has no MCP tool execution loop. Therefore lazy skill loading and actual MCP
use require conversation orchestration, not just another settings form.

## Proposed structure and decision map

AI workspace: **Skills**, **Models**, **Connections**. Keep grants in admin
controls; expose only usable connections and their tools in conversations.

1. Portable skills: `SKILL.md` with name and description, nested text references,
   import preview, version comparison and export of an exact revision. Existing
   skills retain IDs, revisions and conversation references. Reject unsafe paths
   and oversized packages before persistence. Import never executes files.
2. Connections: separate registry for admin-managed remote MCP endpoints,
   credentials, enabled state and user/team grants. Use Streamable HTTP; no
   host subprocess/stdio support. LiteLLM remains the model gateway, not an
   implicit MCP gateway. Existing model origins do not authorize MCP endpoints.
3. Conversation runtime: bounded discovery and call loop, cancellation and
   visible tool results. Reauthorize before discovery and each call. Isolate
   sessions and credentials by user and connection. Treat server descriptions,
   outputs and skill contents as untrusted data, never authorization.
4. Progressive skill loading can follow that runtime: show accessible metadata,
   load exact pinned versions on demand. Existing conversation pins never
   change implicitly; disabled skills should not become implicitly available.

## Critique, evidence and remaining work

Bounded party/grill pass: product favors portable packages and a single AI
workspace; architecture requires separate content and capability registries;
security rejects deriving permissions from skill metadata; operations requires
revocation, reconnect and restart coverage. Recommended sequencing delivers
package compatibility before dynamic discovery; this is proposed for IP. OAuth, MCP apps,
sampling and elicitation remain deferred, not assumed requirements.

Open decisions: which credential modes ship first; which MCP
destinations are approved; what call/time/output limits apply; and how are
external side effects approved? Resolve these before enabling execution. A
server's read-only annotation alone must not authorize a mutation. Required evidence:
package round-trip and traversal rejection; unchanged historical pins; denied
discovery/calls after revocation; cross-user isolation; bounded tool failure;
PostgreSQL restart persistence and browser flows. Builds/tests remain on Apollo;
authorized deployment remains localhost:8081. No production code or runtime
configuration changed during this research. Existing behavior examples remain
in `ai-skills-models.feature`; new agreed MCP examples are unbound contracts.

Independent review reconciled: removed an unsupported upstream auto-upgrade
attribution, marked sequencing as proposed, and retained side-effect approval
as a PL decision. Added direct-user grant coverage. Authorized call success and
failure examples follow once execution approval semantics are settled. Static
diff/contract-shape checks passed; no runtime tests were run for this plan.
