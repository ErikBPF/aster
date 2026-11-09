# AI registry implementation — accepted plan r2

## Outcome and seed

Implement the [accepted PL direction](ai-skills-models-pl.md) and preserve its
verbatim human seed. The AI tab exposes Skills, Models, and administrator-only
access management. Skills have immutable revisions with bounded instructions and
text reference files. Owner editing is separate from private/team/organization
use. Conversations use explicit revisions, never silent latest. No scripts,
remote reference fetching, automatic model selection, or new tenancy layer.

## Repository boundaries

Reuse current identity, team membership, scoped model names, shared registry,
keyring, grant revisions, and server-side credential proxying. Add domain ports
for skill storage and personal-model grants, each with memory and PostgreSQL
providers selected in providers.rs. Additive migrations retain old records.
Skill IDs are opaque; ownership comes from authentication, never request bodies.
Skill content and permission revisions are distinct. Text references carry
names/content, not filesystem paths to read. Bounds participate in AI context
limits. Credentials never enter skills or history.

All new endpoint declarations belong to proto/aster.proto and server api.rs.
Browser code uses Connect calls. Existing personal registration browser/REST
routes share the new policy checks. Company model management reuses its existing
admin registry and fresh identity. Personal registration/use is default-deny
unless admin granted subject or team; owners retain deletion. In configured OIDC
mode authority failure refuses shared skill and policy access. Dev identity stays
an explicit isolated fixture, never production membership evidence.

## Vertical slices and verification

1. **Versioned skills:** core SkillStore, immutable content/history, owner and
   audience policy, memory/Postgres providers, server registration/read/share
   operations. RED: private read refusal, team isolation, non-admin org publish,
   stale revision conflict, old content unchanged after edits, restart persistence.
   `cargo test -p aster-server --test ai_skills` and core policy tests; marker
   `AI_SKILLS_OK` only after success in the owning just recipe.
2. **Controlled models:** durable admin grants to subjects/teams with optimistic
   conflict checks; all personal registration/selection/generation paths enforce
   current capability. Reuse shared-model ACL management. Encrypt personal tokens
   with supplied keyring and migrate existing rows without plaintext fallback;
   missing keys refuse protected persistence. Destination policy must be checked
   before sending credentials. RED: unauthorized and revoked registration/use,
   saved selection bypass, admin mutation rejection, plaintext persistence, and
   credential disclosure. `cargo test -p aster-server --test ai_registration`
   plus existing shared-model suites; `PERSONAL_MODEL_POLICY_OK` after success.
3. **Pinned assistance and UI:** AI tab skill editor/history/sharing, personal
   model form gated by capability, admin shared-model/grant controls. Notebook
   and cell conversations select explicit skill revisions; revalidate every turn
   including prior skill dependencies. RED: version update leaves pin unchanged,
   explicit upgrade changes context, revocation makes zero upstream calls,
   malicious reference markup stays text, disabled controls cannot bypass API.
   Browser fixture plus server upstream-capture tests; `AI_REGISTRY_BROWSER_OK`.

Each slice owns tests and implementation; no compile error is counted as RED.
A minimal compiling seam may precede the assertion-failing check. Reuse existing
fixtures and HTTP test harnesses. Exact recipe names are added with tests, and
raw receipt paths recorded in this artifact.

## Delivery, validation and recovery

Local task worktree: ai-registry-plan at d31ca27. Preserve the uncommitted deployed
catalog patch in catalog-current-rv; integrate that exact reviewed diff before
final delivery without modifying its source worktree. No commits or pushes.
Apollo receives only explicit source snapshots, excluding credentials/build state,
with matching hashes. Run declared devenv, focused RED/GREEN, new browser suite,
`CARGO_BUILD_JOBS=8 devenv --no-tui shell -- just ci`, and disposable Compose
persistence checks. Deployment remains scoped to the previously authorized local Podman target;
Apollo remains build/test only.

Independent review must verify authorization, secret migration, reference bounds,
history revocation, accessibility and seed coverage. Freeze first-draft evidence,
rewrite away duplicate routes/policy, then rerun affected checks. Additive schema
allows prior metadata recovery; encrypted tokens require retained keys and a
compatible binary. Never roll back by exposing plaintext or discarding grants.
Independent critique below resolved the planning blockers. The
[RV report](ai-skills-models-rv.md) records implementation and execution evidence.

## Independent critique and accepted refinement (r2)

Fresh IP review confirmed the three slices and requested four corrections. The
accepted behavior contract now includes team-edit isolation, nonmember revision
refusal and saved-model revocation. Skills and personal policy must configure
fresh authority independently of enabling shared models; disabling the shared
registry cannot disable or bypass these checks. A legacy-token migration runs
transactionally with an explicit supplied keyset, clearing plaintext only after
successful encryption of every row. Missing/wrong keys refuse access; interrupted
migration must restart safely. Owner deletion remains permitted. Conversation
messages persist exact skill identity/revision dependencies, and every turn
revalidates both requested and historical dependencies before any upstream call.
Enforce aggregate instruction/reference/context limits, not only per-file limits.

Source baseline now includes the exact reviewed catalog patch copied from
catalog-current-rv; the original checkout remains untouched. No outstanding
planning decision blocks these slices. Proceed to assertion-failing regressions,
then implementation; retain source and execution receipts for the final RV.
