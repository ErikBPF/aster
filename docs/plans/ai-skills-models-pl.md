# AI skills and model access — PL accepted direction r4

## Human seed (verbatim)

On ai tab user should be able to register skills. Those can also be versioned. Those can be shared with team. Admins can share skills with whole org

We should be able to register company wide models that respect admin managed ACLs. If alowed, users can register their own models with URLs, names and tokens

## Outcome and accepted behavior

The AI tab manages reusable skills and available models, with explicit ownership,
sharing scope and access status. Skills contain versioned instructions and
reference files; bundled code is not executed. Conversations pin a selected skill
version and upgrade only by explicit user action. Administrators grant personal
model-registration permission to selected users or teams. Company model access
is governed by administrator-managed ACLs, independently of skill sharing.

Planning checkout: `aster/worktrees/ai-registry-plan`, branch
`feat/ai-registry-plan`, baseline `d31ca27`. The deployed catalog improvements
remain in `fix/catalog-current-rv`; this planning branch does not replace that
work. Reconcile that dependency before implementation delivery. No deployment is
authorized by this feature proposal alone.

## Source evidence and reuse

[AI routing](../../crates/server/src/ai.rs) already lists and resolves scoped
`personal/<id>` and `shared/<id>` references. Shared use checks current identity
and grants rather than trusting saved selections.
[Shared models](../../crates/server/src/shared_models.rs) provide encrypted token
storage, approved HTTPS destinations, administrator management, revisioned ACLs
and audit events. [Access tests](../../crates/server/tests/shared_model_access.rs)
cover collisions, revocation and credential routing. The “planned shared use”
paragraph in [AI documentation](../ai.md) is stale; implementation must reconcile
it against actual enabled and tested paths, not introduce a second registry.

Personal registration currently lacks the requested admin-controlled capability.
Its [PostgreSQL store](../../crates/server/src/store.rs) persists tokens in plain
text; endpoint validation is syntactic. Proposed security work: encrypt personal
tokens with recoverable migration, retain tokens server-side, and use approved
destinations so registration permission cannot grant arbitrary server-network
access. This is proposed design, not a claim of existing protection.

## Decision map

| Boundary | Status | Consequence |
|---|---|---|
| Skill payload | Accepted: instructions/reference files, no script execution | Treat content as bounded AI context, never executable authority |
| Version selection | Accepted: pin and explicitly upgrade | Immutable revisions; authorization must remain current |
| Personal registration | Accepted: admin grants users/teams | Enforce across browser, REST, RPC and replacements |
| Model registry | Reuse existing personal/shared identities | Preserve collision handling and saved selections |
| Team publication/editing | Accepted Q4 on continuation | Member publishes own skill; audience gets use, owner retains editing |
| Personal permission revocation | Accepted Q5 on continuation | Block subsequent registration/update/use, retain records |
| Organization boundary | Accepted Q6 on continuation | One organization per installation; no invented tenancy |

Pinning content must not pin permission. Proposed invariant: membership or skill
sharing revocation prevents the next use, including reuse from conversation
history; upstream requests cannot carry revoked content. Already delivered
responses cannot be recalled. Skill access never grants a model, credential,
query permission, or notebook access. Historical versions remain within current
sharing boundaries; individual revision retirement is proposed as out of scope for the first
release, pending acceptance. A revoked-history turn should fail with a clear
recovery action rather than silently strip context; this remains proposed behavior.

## Bounded party and critique

Product wants one discoverable AI workspace; architecture favors the existing
model registry and identity authority. Security separates skill audience from
editing rights, model grants and endpoint approval. Operations requires token-key
recovery and durable revisions. The conflict is convenience versus silent scope
expansion: selecting a shared skill cannot silently select a privileged model or
upgrade its content. Team publication and revocation remain human decisions.

Independent source critique confirmed the reuse and identified plaintext personal
tokens, missing registration policy and absent organization identity. No runtime
validation was performed. Existing deployment identity restrictions remain in
force; development claims are not production membership evidence.

## Questions, gates and recovery

Q1–Q3 answered through native async questions: payload, personal capability and
version pinning as above. Q4–Q6 pending on this checkout/r3: team authority,
revocation effect, organization boundary. These block dependent BDD/IP decisions;
no answer is inferred from timeout or silence. No tuicr session exists yet.

[Draft behavior contract](../../crates/server/features/ai-skills-models.feature)
records accepted examples only and is explicitly unautomated. Next: reconcile
answers, independent reread, then delivery planning with migrations, policy
negative cases and Apollo-only validation. No implementation or secret handoff
has begun. Recovery at this stage is revision of these planning artifacts.

## Independent reread and revision

R1 was read independently against the human seed. R2 moves version retirement
back into PL rather than delegating product behavior to IP, removes an undefined
endpoint-policy assumption from the draft scenario, and adds the seed's positive
admin-publication example without deciding the pending organization boundary.
UI terminology must distinguish a registration's display name from the upstream
provider model identifier. Documentation links and draft feature shape were
checked; this does not constitute executable behavior validation.

## Continuation: verified boundaries (r3)

The human requested continuation; Q4–Q6 remain unanswered and were consolidated
into one native async question with the previously recommended behaviors. The
answer affects publication, existing model use and identity isolation; it is not
an approval to deploy. Dependent scenarios remain absent until answered.

Source tracing confirms two personal-registration entry points:
`ai::put_config` (REST) and `ai::put_config_form` (browser). Both currently write
to the same subject-owned store. Neither checks a personal-registration
capability. There is no existing model-registration RPC to retrofit: new endpoint
work must follow the repository's protobuf-first convention. Notebook selection
already has REST and Connect entry points. Generation and conversation turns must
recheck use authority; hiding the registration form cannot enforce revocation.
Deletion should remain available to the owner even when use is refused, so users
can remove retained credentials; this is a proposed lifecycle rule.

The domain already tracks source dependencies in conversation messages and counts
them in the context budget. Skill identity/revision must be tracked distinctly
from contract/catalog dependencies, preserving legacy deserialization. Pinning a
skill cannot turn its instructions into authority over tools or data. References
must remain bounded, explicitly registered content; no automatic URL crawling or
script execution is implied by registration.

The AI workspace should distinguish model registration name, provider model ID,
sharing scope and access status. Reuse personal/shared model references and
current identity checks. Proposed skill editing shows immutable history and
optimistic conflict detection; sharing settings are separate from content
revisions so permission changes need not rewrite historical instructions.
These are source-grounded design recommendations, not implemented behavior.

No application code changed during continuation. The existing catalog delivery
worktree and running service remain untouched. Next gate remains the consolidated
permission answer, followed by reconciliation of the behavior contract and IP.

## Accepted continuation (r4)

After the consolidated recommended behavior was presented, the user again said
“continue”. This is treated as acceptance of that stated package (Q4–Q6), not
silence or timeout. Earlier pending notes are historical discovery. Team members
may publish their own skills to their team; sharing grants use, not editing.
Personal capability revocation blocks registration, changes and future use while
retaining records. One installation represents one organization. No separate
revision-retirement workflow in the initial release; revoke sharing instead.
Revoked skill history refuses the next turn with a start-new-session recovery
message rather than silently rewriting context. Owners can still delete retained
personal credentials. These boundaries guide the implementation plan below.
