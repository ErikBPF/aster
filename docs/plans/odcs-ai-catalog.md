# ODCS-first AI, semantics and catalog plan

## Accepted demo continuation r24, 2026-10-09

**Human seed, verbatim:** “we need to test notebooks. Also catalogs should be
information from a tpc-h and tpc-ds on two different catalogs. We should show more
information from datacontracts”. Subsequent answers accept real queries through
one native Trino backend, separate `tpch`/`tpcds` catalogs, all benchmark tables
at tiny scale, owner-checked persistent local Git notebooks, and structured
display of all populated contract fields with exact source/provenance retained.
The user said “continue”. Build-host's isolated demo remains the authorized target;
no commit, push, production identity or other cluster change is authorized.

**Destination:** a real Authentik login lands on usable notebooks. Alice creates,
edits, saves and reloads a notebook, explicitly chooses either benchmark catalog,
and executes real SQL with independently asserted results. Both catalogs expose
actual metadata for every table. Each table has a separate synthetic ODCS
contract; readable summaries cover every populated field, including schema and
columns, descriptions, ownership, declared rules and terms where supplied. Do not
invent operational SLAs, quality results or production/business ownership.

**Grounding:** `web.rs::index` rejects whenever `team_workspaces` exists;
`lib.rs::build_state` creates it from the contract membership policy. Runtime
team workspace activation is incomplete. The existing engine executes Trino SQL,
but catalog registry has no Trino metadata adapter and browser Run omits catalog
context. The existing GetContract response already retains the whole document
and original source; richer display needs no parser replacement.

**Bounded elicitation:** Product: “A notebook must save and execute, not just show
a catalog.” Architecture: “Native generators avoid a second copied dataset, but
require a bounded Trino metadata adapter.” Security: “Do not remove contract-team
policy to make notebooks work; explicitly choose local notebook mode and retain
ownership checks.” Tester: “Assert the actual landing response and UI lifecycle;
the old browser test skipped the broken landing.” These concerns define separate
test seams, not additional user requirements.

**Decisions / map:** preserve current contract admission and current-identity
checks while selecting local notebook mode explicitly. Use native Trino benchmark
connectors, not Polaris materialization or relabelled mocks. Preserve all tables
and per-table grants. Browse metadata and query execution remain distinct; SQL
target-level contract enforcement is still unimplemented and must not be claimed.
Keep this backend synthetic-only and loopback-accessed. Full team/GitHub workspace
activation, AI integration, semantic-sidecar execution and production rollout are
out of scope. Remaining factual frontier: exact tiny schemas/table inventory,
adapter transport bounds, browser routing and repeatable isolated deployment.

**Behavior contract:** `crates/server/features/benchmark-notebooks.feature`
records agreed examples; draft until executable checks bind and fail. Delivery
and rollback are in the r24 section of `odcs-ai-catalog-ip.md`; observed results
and unresolved limitations belong in `odcs-ai-catalog-rv.md`.

**R24 delivery:** Helm revision 8 is deployed on the isolated Build-host demo. Real
browser notebook queries, persistence across server restart, all 33 native
benchmark tables/contracts, viewer denial and contract revocation passed. Full CI,
Compose, chart and adapter checks passed. See the current
[delivery receipt](odcs-ai-catalog-rv.md#r24-delivered-real-benchmark-notebooks-2026-10-09).
This does not close the explicitly deferred SQL/share or team/GitHub work.

**Historical r23 status:** materialization-level mock contract correction verified on Build-host;
per-table work and S8 fixes complete after source-only RV `ses_f01458357ffeG13RoBziy3aSeB`
(no verified findings, no new runs). SQL/share, global activation and semantics ingestion remain pending.
The three S8 P2 fixes from `ses_f029ed1c9ffe2awvOuIQ06x5Xh` passed their full Build-host
gate before this correction; receipts are preserved.
R20 docs/mock review `ses_f030e0250ffeIPVS1cgeiNPZIZ` remains the reviewed policy
basis, not runtime approval. Contract-required SQL/sharing and semantics sidecar
ingestion remain unimplemented. No deployment or universal data-access claim.
**Date / owner:** 2026-09-28 / Aster maintainers.
**Checkout:** `/home/developer/projects/aster/worktrees/odcs-ai-catalog-plan`.
**Branch / source base:** `docs/odcs-ai-catalog-plan` / `eb397d865cf2dc156fda172d50b13c98eda9d9f7`.
**Independent r3 review:** `ses_f1ab8829cffefXA3JcQf0narDN`; four user-relayed
findings applied in r4 (receipts in the RV page). New feedback
must name r23 and the affected decision, behavior or slice. Historically, r8 authorized staged
implementation after the documentation RV, not commits, publication, deployment
or a live campaign. The S1 execution receipt is in the RV page.

**New discovery input:** [Compiled-contract learnings](example-platform-contract-learnings.md).
The accepted boundary is compiled-artifact consumption and contract/semantic-first
interaction. Generic hierarchy, compilation and materialization lessons remain
background, not Aster implementation requirements. Source-specific notes are
preserved privately.

### Human correction r23: materialization-level contracts

Exact authoritative user seed:

> contracts should be on the materialization level. If here the materialization level is a table we should have one for each table

This is application artifact granularity. For the five current mock materialized
tables, publish five complete compiled ODCS v3.2 contracts: orders, customers,
daily_revenue, events and clickstream. Each has its own stable document identity,
exact physical binding and whole-contract grant. An orders grant must not disclose
customers through artifact reads, preparation or inventory admission; the raw pair
must be equally separable. Preserve general ODCS multi-object parsing/preservation:
this correction neither bans multi-object documents nor defines every future
materialization as a table.

Schema-level `semantics.yaml` remains a description of measurements across that
schema. Its table entries reference the exact corresponding contract identity,
version/digest and object pointer. It grants nothing, is not runtime-ingested, and
must not broaden table-specific disclosure. Replace only the three task-created
mock documents after tracing consumers; repin manifest, bindings, grants and
sidecars. Observe fixture/admission RED before replacement and run the mock gate
plus the affected canonical S8 gate on isolated Build-host snapshots. Preserve prior
review evidence; no commits, pushes, agents, deployment or next runtime slice.

### Runtime continuation r21: S8 physical inventory admission

**R22 reviewer corrections:** unknown/configured unauthorized schema probes now
share status and payload, including global grant-error handling. One freshly
verified identity supplies both owner and contract admission, also across scopes
in one inventory-page/namespace operation. Semantic annotation detection follows
nested ODCS property structures, not just top-level fields; it grants no access
or executable support. The [RV disposition](odcs-ai-catalog-rv.md#independent-s8-rv-ses_f029ed1c9ffe2awvouiq06x5xh-r22-corrections)
records the three observed RED failures and the current five-case gate. No next
slice or new product-policy acceptance is inferred from these fixes.

The user authorizes the first coherent runtime slice after r20 review, in this
existing worktree, with assertion RED and all builds/tests/devenv on Build-host only.
No nested agents, commits, pushes or Minikube refresh before slice review.

S8 implements the configured compiled-bundle inventory boundary: exact schema
ownership in operator-managed `schema-owners.json`, existing team membership from
fresh verified identity, and independent exact artifact grants. ODCS `team` or
owner fields confer no authority. Fresh authoritative metastore descriptor listings
are the existence oracle, valid only for the current metadata request; there is
no positive cache or claim that registration proves readable rows. Unavailable,
non-authoritative or malformed listings yield unknown evidence and no entries.
Polaris can prove current registration; Mock proves fixture registration only.
Cube and OpenMetadata discovery metadata do not prove physical registration.

`ListCatalogInventory`, compiled-mode REST/RPC browse and the catalog inventory
page share admission. Only current schema owners see objects without an admitted
contract. Contract-only artifacts stay in a labelled artifact/context section.
Legacy completion offers keywords only in compiled mode; unselected AI omits
physical enrichment, and catalog-only old history refuses replay in that mode.
Selected admitted artifact context remains available without claiming physical
data accessibility. Independent physical/contract/semantic facets carry no query
authorization; semantic coverage currently means recognized property annotations,
not sidecar ingestion, completeness or executable Cube support.

**Scope boundary:** this is the compiled-bundle metadata path, not retirement of
the no-bundle development browse path. No-bundle operation still has its old
unprotected development policy and must not be represented as r20 enforcement.
Do not remove the bundle to bypass ownership on rollback; disable exposure instead.
SQL/sharing still need authoritative resolution of every target and view dependency
in the next slice. No SQL substring/regex authorization was introduced.

The owning gate is `devenv shell -- just odcs-inventory-validation`: six Rust
tests, six bound scenarios/six steps (including r23 table-grant isolation), exact-count report, transitive S6/S5/S7/S3/S2/S1
regressions/full CI and isolated S8 Compose cleanup. The [RV record](odcs-ai-catalog-rv.md)
distinguishes observed failures, verification and independent-review status.

### Human correction r20: physical inventory, ownership and mock contracts, 2026-10-02

Exact latest authoritative seed, spelling preserved:

> data exists in a catalog. If it is not there it cannot be accessible. Every catalog schema has a owner group and only this group can see non-contracted data. Only data with contract can be accessed and shared. Optimally all data have a contract. We should follow ODCS for this. Lets create mock contracts for what we have right now. Each schema can offer a semantics.yaml explaining how things are measured. Look at cube semantic layer implementation for examples.

Authority: user supplied this correction for the existing Aster/platform task
worktrees, branches and bases above. R20 authorizes documentation and MOCK fixture
creation/validation. It does not authorize cluster updates, commits or pushes.
Independent source-only RV is recorded in the existing RV page; it establishes
no Cube execution, actual business grain or deployed behavior. All builds and tests remain Build-host-only.

#### Accepted rules versus proposed design

- Data must physically exist in a catalog to be accessible. A pinned binding or
  ODCS document alone is not existence evidence. Denied/unavailable observation
  is unknown, never proof of absence or permission to query.
- Each exact catalog/schema has an owner group. Only that group sees uncovered
  metadata. This settles Q9's nonowner disclosure question and Q10's owner scope.
- Only contracted data may be accessed/shared, **including by its owner** unless
  the user explicitly changes that rule. The earlier admin/private query bypass
  is not carried forward. Existing explicit whole-contract team admission and
  independent role/engine/routing/backend checks still apply; ownership alone
  grants no data access. Contract validity/presence alone also grants no access.
- All data having contracts is the desired coverage goal. Schemas may offer
  `semantics.yaml`; it is optional descriptive context, not an access grant or
  an asserted executable Cube engine.

**Proposed interpretation, not extra human policy:** treat "see" as metadata
discovery/detail and "access/share" as data read/query/distribution. Contract-only
documents may remain in an explicitly separate authorized artifact-management
surface, never as accessible physical inventory. The precise artifact-only UX,
existence freshness window and ownership administration are design decisions to
review, not reasons to reopen the owner's stated contract requirement.

Q11 remains an implementation/security question: prove coverage for all actual
query targets, including view dependencies and joins, at an authoritative backend
or resolved-plan seam. Do not admit arbitrary SQL because a contract was selected,
and do not implement a bypass-prone SQL string parser. Unknown target coverage
must fail closed under the proposed enforcement design. Sharing also needs a
concrete surface/recipient admission design; existing grants are not transferable.

#### Grounded mock deliverable and pipeline (r20; granularity superseded by r23)

[Mock bundle inventory, Cube evidence and exact integration proposal](../../contracts/mock-catalog/README.md)
cover `mock-local`, namespaces `["aster_demo"]`, `["aster_demo.sales"]`,
`["aster_raw"]`, and all five source/live-observed tables. Three full hand-authored
compiled ODCS v3.2 draft fixtures, a pinned manifest, five explicit bindings,
separate whole-contract grants and three schema semantics descriptions are supplied.
No inferred production promises, author hierarchy or compiler. Schema owner
assignment to `aster-editors` is **mock design**, matching the checked-in Alice
Keycloak group `/aster-editors`, not an observed deployed owner configuration.

Build-host read-only inspection found the demo uses dev-login with no groups/verified
identity and has no compiled-bundle/fresh-identity/team-policy wiring. Consequently
the fixtures cannot honestly be advertised as loaded in its UI. `semantics.yaml`
is not an existing runtime input: the precise proposal is upstream inclusion of
bounded per-object definitions/provenance in compiled ODCS custom properties,
followed by an explicitly tested projection in current preparation/AI. Details,
pins, association rules and Cube source links live with the fixtures.

Current UI still has a contract picker and separate physical browser. Current
REST/RPC query admission does not enforce physical/contract coverage or schema
ownership. The [IP amendment](odcs-ai-catalog-ip.md) separates fixture completion
from the next runtime slices; the unautomated feature states target behavior,
not passed tests. No live stack changes occurred.

### Human correction r19: unified catalog and access scope, 2026-10-02

**Historical correction and proposal:** r20 supersedes contract-only accessible
inventory, admin/private bypass assumptions, and the Q9/Q10 unanswered status.
The original quotation and discovery evidence are retained verbatim below.

Exact user correction after the live demo (spelling preserved):

> catalogs and contracts are in different views, but completely bounded. We should unify on a catalog view and evaluate all data on: bein on te catalog, having a contract, having semantic layer. Data is mainly accessible only if a contract is provided. direct catalog access is reserved for admins or for "private product tables"

**Authority and provenance:** supplied for the existing checkout/branch/base above
and platform `aster-odcs-plan` at `d792329e05f913ed5aa220476c38cb9b54f55512`.
This is a product correction and a documentation/discovery authorization, not an
ACL specification or runtime approval. Feedback now binds to canonical **r19**,
Q9–Q11 and these two task worktrees. No new independent review session exists;
earlier sessions cover their recorded revisions only. All future builds remain
Build-host-only; source review and documentation checks need no build.

#### Destination and proposed interpretation (separate from the quotation)

Use **one catalog view as the data inventory**, with contract details and semantic
context attached to entries rather than competing top-level discovery views.
"Catalog view" is the product inventory, not a requirement that every entry be
physically observed. Preserve authorized contract-only entries when physical
observation is unavailable. Join evidence only by explicit qualified bindings;
never collapse same-named objects or distinct artifact versions/targets.

Evaluate three **independent facets**, not mutually exclusive lifecycle statuses:

| Facet | Evidence and limits |
|---|---|
| Physically cataloged | Qualified catalog/namespace/object identity and observed evidence. A configured binding alone is not a live existence check. Denied/unavailable observation is unknown, not proof of absence. |
| Contract coverage | Explicit association to admitted compiled artifact path/digest and schema object; whole-document team authorization remains separate from association. No bare-name match or hidden-contract disclosure through a badge. |
| Semantic coverage | Show what meaning/roles/expressions/relationships are actually declared for the entry or its fields, with provenance. A contract, Cube provider, or semantic annotation does not establish completeness, executable support, or access. |

Recommended ordinary route: a contract-associated entry with an explicit
whole-contract grant to an existing Aster team, default-deny. Querying additionally
requires the current role, subject-to-engine grant, valid routing and backend
authority. Mere contract presence is insufficient. Semantics improves query
context; it grants no access and is not an additional mandatory gate in this
correction. Observation remains separately authorized.

The named raw-catalog exceptions are admins and "private product tables".
Their precise operation and ownership scope is **unsettled**. Do not interpret
admin as a bypass of engine grants/backend policy, or equate private products
with personal/team notebooks, a schema name, or an arbitrary table label.
Whether other users may see uncovered entries at all awaits Q9; the inventory
does not imply globally visible metadata. Earlier generic "fallback/expert"
access is superseded as a target by these narrower, pending exceptions.

#### Current implemented behavior versus r19 target

Source inspected on 2026-10-02 in the existing dirty task tree at base `eb397d8`,
including prior implementation. These are source findings, not a new runtime run.
No Graphify graph exists in either task checkout; direct source tracing was used.

| Surface / source | Implemented now | r19 delta, not implemented |
|---|---|---|
| `crates/server/src/web.rs` (`layout`, `catalog`, `contracts`, `physical_catalog`, namespace/table handlers); `assets/app.js` contract-context block | Separate nav entries; `/catalog` and compiled `/contracts` render the same contract picker; `/catalog/physical` browses separately. JS lists/gets/prepares contracts, not a unified inventory. | One entry-oriented catalog surface, three independent evidence facets and embedded contract/semantic detail. |
| `crates/server/src/contract_reads.rs` (`read`, `grants`, `prepare_inner`); `api.rs` List/Get/PrepareContractQuery | Exact artifact path/digest whole-document grants; current identity/team membership refreshed on each read; separately admitted observations; missing/ambiguous binding explicit. | Reuse these authorities/provenance when associating inventory entries, without inventing semantic completeness or new grants. |
| `crates/server/src/lib.rs` (`catalog_metadata_visible`, `require_catalog_metadata`); web and REST/RPC catalog handlers | Coarse metadata policy: any protected binding closes visibility; otherwise an explicit unprotected binding is needed. ReadNotebook role, not an admin/private-product distinction. | Exception-aware metadata behavior depends on Q9/Q10. A UI rename does not enforce it. |
| `crates/server/src/lib.rs` (`QueryBody`, `run_query`, `execute_query`, `run_attempt`); `api.rs::run_query`; `crates/core/src/{auth,grants}.rs` | REST and RPC converge on RunQuery role, subject engine grant, exact catalog routing and protected verified-session execution. QueryBody carries no contract selection; no contract-coverage admission or private-product policy is checked. | Ordinary contract-required query access, if Q9 includes execution, needs an explicit shared admission design and authoritative coverage of actual SQL targets (Q11). UI selection alone cannot secure SQL. |

#### Decision map and pending behavior

Accepted unified inventory + independent facets -> Q9 operation scope -> Q10
private-product identity/owner/team boundary -> Q11 trustworthy query-target
coverage -> revised IP and review -> authorized implementation with Build-host proof.
Keep r5 whole-contract grants and independent engine/backend authority throughout.

| ID | Precise product question / recommendation | Blocks |
|---|---|---|
| Q9 | Does "accessible" restrict discovery (entry names, schema and contract detail), query execution, or both? Specifically, can ordinary users see an uncovered table as unavailable, or must its existence be hidden? Recommend an explicitly authorized discovery scope, separate from query eligibility; do not choose visibility by implementation accident. | Load-bearing first answer: inventory disclosure and query policy boundary. |
| Q10 | What makes a table a "private product table", who is its authoritative owner, and is the exception owner-only or available to explicitly designated existing team members? Who can classify/revoke it, and does admin/private raw access cover discovery, querying or both? Recommend authoritative explicit ownership/membership, not self-asserted labels or notebook ownership. | Exception model and negative cross-team examples; no guessed ACL. |
| Q11 | If query execution is included, must every referenced table (including joins/views) have admitted contract coverage or a permitted exception, and which authority proves that coverage? Recommend all actual data targets covered; a selected contract cannot authorize unrelated SQL. | Shared REST/RPC admission design; enforcement location cannot be chosen from a UI-only requirement. |

Pending examples are questions, **not agreed or bound scenarios**: ordinary user
discovers an uncovered physical table; owner versus teammate queries a private
table; admin queries without an engine grant; selected allowed contract accompanies
SQL referencing an unrelated table. Agreed facet/no-inference examples are recorded
in the existing unautomated feature; historical bound S1–S7 receipts prove only
their earlier scope.

Bounded source-review perspectives (single author, no agents or independent review):
product favors one inventory, while security requires deciding whether listing
uncovered entries leaks their existence. Architecture can reuse exact identities
and grants but finds no current query-to-contract admission. A counterexample
grill rejects "contract selected means query allowed" and "semantic present means
complete/authorized". These conflicts remain Q9–Q11, not manufactured agreement.

Next: obtain Q9, settle the exception/coverage boundaries, then revise concrete
vertical slices and their observable allow/deny examples. No new runtime policy,
build, commit or deployment follows this documentation revision. R18 closure and
all prior review/execution receipts below remain intact as historical scope.

### Human acceptance r18: initial scope closure, 2026-10-02

Exact user answer, relayed for this documentation update:

> yes

Accepted scope: close the initial implementation with original-document preview
as the initial export; defer transformed exports and optional Cube generation;
validate live providers once authorized targets and credentials are supplied.
S1/S2/S3/S7/S5/S6 and narrow S4 are complete after the recorded per-step RV and
corrections. Remaining S4 output, including new-document generation and its
identity/version policy, is explicitly deferred to future Q3/Q4 scope. The broader
`odcs-semantic-validation` gate is unimplemented/unrun, not falsely passed or
required for this accepted initial scope.

Provenance: this acceptance updates canonical r17 to r18 in the checkout/branch/base
above and the existing platform `aster-odcs-plan` worktree, branch
`docs/aster-odcs-plan`, base `d792329e05f913ed5aa220476c38cb9b54f55512`.
It follows narrow S4 source-only RV `ses_f04fc5bd4ffe252GUe2zkjtRIW`; it is a human
scope decision, not a new independent review or runtime receipt. Earlier planning
and execution statements below retain their historical scope; this closure governs
historical initial-scope status; r19 above governs the new destination. Q6 needs
target-specific credentials through the existing secret
mechanism and authorized live validation; fixture proof is not production proof.
Known platform missing-reference/link blockers remain, with no all-repository GREEN.
This is documentation-only: no build repeat, commit, push or deployment; any future
builds remain Build-host-only.

## Human seed and destination

Verbatim seed:

> check our aster repo and lets better structure ai and semantic layer/odcs 3.2 integration. Also look at catalog im plementation. Lets do a full /pl /ip /rv flow

User answer: ODCS-first is selected: Git-managed ODCS 3.2 owns business
semantics, catalogs supply observed schema, and Cube is optional derived output.
The user initially requested review first, with **no implementation**; r8 below
supersedes that execution restriction. The agent
chose this checkout and the no-platform-write boundary; those were not product
answers from the user. The parent completed platform documentation synchronization
in its dedicated worktree; integration and check blockers are recorded below.
At r11 the current step implemented user-authorized S7 after S3 RV; r19 reopens
discovery only, with no new RFC or ADR.

### Human continuation r11: S7 after S3 review

The user authorizes S7 in the existing worktree with test-first actual DOM,
network and query-action proof, full regressions and isolated Build-host Compose.
Use full compiled ODCS 3.2 and semantics as primary query-building context,
whole-contract existing-team default-deny, independent observation/execution,
and no automatic execution. S5 helper integration, AI budgets and Cube decisions
remain outside this authorization. Preserve prior receipts and running stacks;
no nested agents, commits, pushes or deployments. Review each step directly and
deliver an RV-ready candidate, without claiming independent review approval.

### Human correction r3

> we should base integration on full contracts in compiled folder. Tool does not need to know about uncompiled structure. contracts and semantics should be the preferred way to interact with data. Not only catalog

This supersedes assistant-proposed author-hierarchy support. Aster consumes full
resolved contracts from `compiled/` in a published/copied artifact bundle, never
a live sibling working tree. It requires no author files, `_definitions/`,
manifest merging, backend base injection or compiler implementation. Upstream
target-expansion changes are not prerequisites or work within Aster.

Contracts and semantics are the primary discovery, browse, query-preparation and
AI interface. Physical catalogs support observed schemas, explicit bindings and
drift, plus fallback/expert access; they are not the prerequisite discovery tree.
Authorized contract scope can expose a contract without a live schema. R5 settles
Q7's model as whole-contract access granted to existing Aster teams, default-deny;
mapping storage/wire remains implementation design. Neither a valid artifact, an unbound object,
nor an allowed schema implicitly authorizes contract disclosure. Observation
access is separately authorized. Execution still requires engine grants and
backend policy; this direction does not promise a semantic query engine.

### Human acceptance r5

Exact recommendation, as relayed by the user in this planning-only update:

> whole-contract access granted to existing Aster teams, default-deny. Catalog observations and query execution retain separate permission checks, so an authorized contract remains usable when its catalog is inaccessible.

Exact user answer:

> use it

Provenance: the user supplied this exchange on 2026-09-27 for the existing Aster
checkout/branch/base above and platform coordination checkout
`/home/developer/projects/platform/worktrees/aster-odcs-plan`, branch
`docs/aster-odcs-plan`. R5 records acceptance of this authorization model only,
not implementation, other policy, or an independent review sign-off. Existing
review session `ses_f1ab8829cffefXA3JcQf0narDN` covers r3 only; no new review
session is attributed to this answer.

Q7's team and whole-contract granularity is settled. Grant mapping storage/wire
and exact denial/revocation response details remain implementation design/open
as needed, not additional invented human approvals. Contract grants do not grant
catalog observation or query execution. At r5, Q8's minimum preparation outcome
and all other unanswered policies remained undecided.

### Human acceptance r6

For the first-release compiled-contract intake choice, the user explicitly selected:

> v3.2 only

The selected option's description was:

> Require upstream conversion before intake. Smaller compatibility scope, but current reference compiler output cannot be consumed directly.

Provenance: user-relayed selection recorded on 2026-09-28 in the same Aster and
platform task checkouts/branches named above. R6 settles only Q1's first-release
compatibility scope: accept `apiVersion: v3.2.0`, with an explicit support gate
separate from official schema validation, because that schema accepts older
versions. Reject unsupported v3.1 intake rather than silently converting it.
Conversion belongs upstream, outside Aster; current reference compiler output
cannot be consumed directly. The document's own `version` remains independent.

This is not a startup-versus-quarantine decision or approval of exact errors,
statuses, bundle pins, selection wire format or implementation. Q1's remaining
intake decisions stay open; at r6 Q8's minimum preparation outcome was the next question.
R5's whole-contract existing-team grants, default-deny, and separate observation
and execution checks remain accepted. No new independent review is attributed
to r6; historical review receipts remain unchanged.

### Human correction r7: accepted Q8 outcome

> we must be able to use contracts and semantic layer to better contextualize query building

Provenance: latest user correction supplied on 2026-09-28 for the same Aster
and platform task checkouts/branches above. This is authoritative for Q8, not a
new independent review or implementation approval.

Contract and semantic context must materially inform building and reviewing
queries, not merely browsing or passive object selection. Existing AI-assisted
and manual query building receive authorized selected contract meaning, field
definitions, declared semantic roles/expressions, grain and relationships where
declared, provenance and explicit physical bindings where available. These inform
reviewable query drafting; missing or ambiguous bindings are surfaced without
inventing physical identifiers, joins or aggregations. Declared relationships
alone do not prove an executable join, nor does an expression prove support.

Q8's outcome is settled; exact selection/drafting UX and presentation remain
design, and Q5 budgets/error policy remain open. This does not require a new
semantic execution engine or Cube, make every semantic declaration executable,
guarantee SQL without bindings, or authorize automatic execution. R5's team
whole-contract default-deny and separate observation/execution checks and r6's
v3.2-only intake remain accepted.

### Human authorization r8: docs first and RV after each step

> start by docs and them do the rest. Lets do a /rv after each step

Provenance: user-relayed execution authorization on 2026-09-28 for the same two
task worktrees and branches above. Q0 is settled: begin with existing documentation,
hand that revision to a separate independent RV before code, then implement the
planned slices with RV after each step before advancing. This author consistency
pass was not that independent review. The user subsequently relayed documentation
RV `ses_f1782bc2bffeAhfnGJ3S7hiCru` without blockers; S1 RV remains pending.

R8 preserves r5 whole-contract grants to existing teams, default-deny, r6 v3.2-only
intake and r7 semantic context for query building, plus full compiled artifacts
without author-hierarchy awareness. It does not approve unanswered product policy.
S1 needs the documentation RV and runnable verification prerequisites, not Q1-Q8
product answers. S2 still needs Q1 selection/pins and invalid/unsupported handling
(including startup/quarantine and response policy), and Q2 binding decisions.
Q3-Q6 and Q7/Q8 design apply to their later slices as listed in the ledger.

### Human acceptance r9: S2 selection, rejection and binding ownership

Exact user-relayed acceptance on 2026-09-28:

> User explicitly accepted: manifest selecting compiled files by path and SHA256 no latest guessing; reject configured bundle on invalid contracts or digest mismatch not silently skip; physical bindings Aster-owned versioned config separate from access grants.

The same instruction authorizes continuing S2 in the named Aster checkout and
synchronizing the existing platform task checkout. S1 is complete, tested and
RV-fixed. This supersedes the remaining Q1/Q2 S2 policy blockers in r8; historical
acceptance and review receipts remain unchanged. It is not independent S2 RV.

Select exact compiled paths and SHA-256 identities from a supplied manifest;
multiple targets and document versions remain distinct artifacts, with no latest
guessing or contract-ID overwrite. Reject the entire configured bundle on invalid
or unsupported selected contracts, missing selections or pin mismatch: startup
must fail rather than silently skip or install a partial bundle. Physical bindings
are Aster-owned versioned configuration, separate from contract access grants.
Wire spelling and exact-path references for optional ODCS IDs are routine design;
ambiguous references are rejected, never guessed. R6's explicit v3.2-only support
gate remains independent of official offline schema validation.

Intake stays internal, with restricted/denied legacy disclosure until S3. Lossless
core-port/adapter/API identity transport is S2, before resolution. R5's whole-doc
team default-deny and r7's manual/AI query-context outcome remain accepted. No S3
work, nested agents, commits, pushes or deployment is authorized. All builds,
devenv acquisition, Cargo/build-backed tests and isolated Compose run only on
verified `build-host` as `developer`, in a new exact-hash S2 snapshot; preserve S1 receipts
and existing stacks. Assertion RED precedes production, then all S2 gates and
independent RV before advancing.

### Human continuation r10: S3 only

The user authorizes S3 next, documentation first, then review and fixes at each
step. S2 independent review `ses_f16d503e4ffe5PuK5zPFF0AHpj` was performed and
all five P2 findings are resolved; no independent second approval is inferred.
Builds, devenv, tests and isolated Compose remain Build-host-only. Preserve S1/S2
evidence; no commits, pushes, deployment, nested agents, S7 or S5 implementation.

Routine S3 design: exact artifact path plus digest identifies a whole-document
grant in separate versioned Aster configuration. Authenticate a verified session,
refresh the existing current-identity authority each request and check existing
team policy; never accept team headers or catalog bindings as contract authority.
Missing/unknown/malformed configuration fails closed. Read/preparation APIs expose
declared meaning and explicit binding provenance independently of observations;
denied observation makes no adapter calls. Q3 export blockers remain S4; S3 reports
separate facts without claiming supported translation or executable SQL.

### Historical S1-era target and baseline (superseded by r18/r19)

The destination below is unimplemented. Current source has a lossy flat contract
projection, bare-name matching, no official ODCS schema/support gate or accepted
whole-contract team grant implementation. S1 closes the retrieval/workspace gaps;
the handbook distinguishes that verified safety boundary from later targets.
Three S1 scenarios are bound and Build-host-verified; 19 scenarios remain draft.
S1's executable gate passes, pending independent RV; six later gates remain unmet.

Destination: an analyst starts with authorized contracts and semantic meaning,
then distinguishes declared meaning from observed structure,
resolves an object without name collisions, builds/reviews queries using that
meaning, and asks a helper about only metadata
and notebook material they may access. A producer reviews Git contract changes;
an operator can diagnose invalid, unsupported, stale and unavailable metadata
without treating any of those as an empty successful catalog.

Non-goals: a complete typed ODCS implementation, a second semantic authority,
OpenMetadata publishing, a Cube query engine, automatic SQL execution or artifact
publication, vectors, new standing services, row sampling, or enabling protected
catalog access without backend user policy. Disposable future test processes are
not standing services. Existing team cell-conversation rejection remains in place.

## Source-grounded findings

All findings below are source observations at the base revision, not reproduced
runtime incidents. Links and external receipts are in [Appendix A](#appendix-a-discovery-receipts).

| ID / priority | Observation and consequence | Smallest planned response |
|---|---|---|
| F1 / immediate | `conversations::send_checked` calls `ai::grounding(state, text)` without principal or metadata admission; legacy `ai::generate` calls `require_unscoped_metadata`. The reported name `retrieval_context` does not occur on this path at this revision. `Action::RunQuery` and helper access do not authorize catalog disclosure. | S1 carries authorization to retrieval and refuses unsafe access before adapter/provider calls. |
| F2 / immediate | Team admission reads the selected workspace but returns only principal/key. `send_checked` then reads `state.notebooks.get(turn.notebook).await.ok()` for the cell index. Same notebook IDs can produce global material or an empty index rather than the admitted workspace. | S1 carries the admitted snapshot/store; never reload by a bare ID from global state. |
| F3 / high | AI relevance is substring matching; `find_table` picks the first case-insensitive bare name while scanning `CatalogRegistry` HashMap values. Schema errors are swallowed. `RenderSemantic` selects a catalog but matches a contract by bare table name. | S2/S3 preserve qualified identity and return explicit resolution outcomes. |
| F4 / high | `DataContract` flattens every object's properties, discards object/property IDs and physical names, reads only scalar description, and uses a loader-supplied file stem as ID. No `apiVersion`, kind or published-schema validation. | S2 retains raw source plus a deliberately small object-aware projection and offline validation. |
| F5 / high | `TableModel::fields` replaces all observed columns with contract fields. ODCS output writes physical strings such as `bigint` into `logicalType`, scalar top-level description, generated ID and fixed document `version: 3.2.0`. Local parse-back cannot establish conformance. | S3 exposes drift separately; S4 validates ODCS output independently and preserves contract version/identity. |
| F6 / high | Cube output emits `sql: SUM(total)` with `type: sum`; unrecognized/missing transforms default to sum. Source plus Cube documentation supports a double-aggregation risk, not a measured Cube execution failure. | S4 refuses unsupported translation and executes a disposable pinned Cube fixture for supported output. |
| F7 / high | Polaris loads `/metadata/schemas/0/fields`, not the schema matching `current-schema-id`. Ordinary namespace/Iceberg table listings ignore pagination. Generic Table listing already follows tokens and rejects repeats; do not report that path as wholly unpaginated. | S6 current-schema selection, structured names, bounded paging and explicit errors. |
| F8 / medium | Cube `meta` and OpenMetadata `get` deserialize without checking HTTP success. OM requests `limit=200`, ignores paging, reduces FQNs by splitting dots, and registration passes token `None` despite adapter support. | S6 checked transport, exact provider identity, paging and deliberate secret wiring. |
| F9 / high | `GROUNDING_LIMIT` is checked before a whole summary/schema/model append; one oversized item can exceed 16 KiB. Notebook index is separately bounded, but retrieval scans and intermediate allocations are not. | S5 bounds fetching, construction and final UTF-8 reference material, not just loop entry. |
| F10 / medium | Catalog UI renders observed columns and a separate flat contracts page, without declared/observed provenance or drift. Some catalog errors are displayed as escaped raw error strings. | S3/S7 primary contract read/browse/preparation surfaces with supporting comparison, generic errors and accessible status labels. |

Existing source checks prove narrower things: semantic tests assert strings or
parse back through the same permissive parser; conversation tests capture a
single positive catalog; workspace tests verify transcript isolation but the
send fixture does not capture notebook-index material. None is a substitute for
the new negative/positive controls. Historical handbook “verified” statements
are not fresh evidence from this session.

## Historical delivery decision map (r19 delta above governs current work)

```text
selected ODCS-first authority
  -> S1 authorization + admitted workspace (independent urgent boundary)
   -> S2 compiled intake + identity + preservation + validation [Q1, Q2]
      -> S3 contract-first read APIs + resolver + provenance [Q2, Q3, Q7 design]
         -> S7 contract-first browse/query-preparation UI [Q7 design, Q8 UX]
            -> S5 contract-first bounded AI [Q5, Q7 design, Q8 UX]
         -> S4 ODCS/Cube previews [Q4, S7 UI] (separate output branch)
      -> S6 observation adapter quality [Q6] (parallel supporting branch)
Real-catalog readiness requires S6; authorized contract-only discovery does not.
```

| Decision | Status / owner | Consequence |
|---|---|---|
| D1 Git ODCS business meaning; catalog observed structure; Cube derived optional | Selected by user | No runtime catalog refresh rewrites a contract, and no Cube model overrides ODCS. |
| D2 Documentation first, RV after each step | Initial reviewed-plan-first restriction superseded by r8 execution authorization | Separate independent documentation RV precedes S1 code; every implementation step has RV before advancing. Future recipes/gates still require implementation and observed evidence. |
| D3 Structured object identity and raw document plus typed projection | Planning constraint, not a separate user product answer | Keep browse catalog ID, namespace segments and physical object name separate from ODCS contract/object/property identity and engine SQL aliases. |
| D4 Offline pinned official v3.2 schema; validity distinct from support | Planning constraint, not a separate user product answer | A schema-valid document can still have unsupported semantics; neither status grants access. |
| D5 No implicit legacy fallback or automatic publication/execution | Planning constraint, not a separate user product answer | Migration and compatibility are explicit; previews have no side effects. |
| D6 Full compiled-artifact intake, no author-tree knowledge | Human correction r3 | Published/copied bundle only; preserve the full resolved document. No Aster compiler or upstream target-expansion work. |
| D7 Contracts/semantics are the primary interaction | Human correction r3 | Contract-first discovery, browse, query preparation and AI; physical catalogs support observations/bindings/drift and fallback/expert use. Scope authorization remains distinct from validity and execution. |
| D8 Whole-contract team authorization, default-deny | Human acceptance r5: “use it” to the exact recommendation above | Grant whole contracts to existing Aster teams. Catalog observation and query execution require separate checks; inaccessible catalogs do not invalidate authorized contract use. |
| D9 First-release compiled intake supports v3.2 only | Human acceptance r6: "v3.2 only" | Explicit `apiVersion: v3.2.0` support gate, separate from schema validity; reject v3.1, never silently convert. Upstream conversion stays outside Aster. |
| D10 Contract/semantic context materially informs query building | Human correction r7 | Existing manual and AI-assisted query drafting/review uses admitted meaning and available explicit bindings; unresolved bindings stay visible, with no invented identifiers, joins or aggregates and no execution. |

### Open questions (canonical ledger)

**Current frontier:** r20 settles uncovered visibility and owner/no-bypass scope.
Q11 authoritative query-target coverage, owner-policy wiring and existence
freshness need the next runtime slice. Q0–Q8 below retain historical decisions;
contract-only usability is artifact context, never data accessibility under r20.

Unanswered portions are recommendations, not agreements; Q1 records accepted
compatibility, Q7 the accepted model and Q8 the accepted outcome, each with remaining design.
Q0 is authorized in r8 subject to per-step RV; later choices block their own slices,
not S1's existing-policy safety boundary.

| ID | Question / recommendation and trade-off | Owner / blocks |
|---|---|---|
| Q0 | **Authorized in r8:** docs first, then implementation with RV after each step. Separate independent documentation RV precedes code. This does not settle unanswered policy or authorize publication/deployment. | User authorization recorded; documentation RV pending before S1, per-step RV thereafter |
| Q1 | **Accepted r6/r9:** v3.2 only, explicit support gate independent of schema validity; manifest selects compiled files by exact path and SHA-256, with no latest guessing. Reject configured bundle on invalid/unsupported selection or digest mismatch, never silently skip or install partially. Conversion remains upstream. Exact wire/errors are routine design; S2 retains internal intake and restricted/denied legacy disclosure until S3. | S2 policy settled; implementation and verification pending |
| Q2 | **Accepted r9:** physical bindings are Aster-owned versioned configuration, separate from access grants. Keep artifact/document/object identity distinct from `(catalog ID, namespace segments, physical name)`; no author-tree dependency or name guessing. Exact wire/path references are routine design with duplicate/ambiguity rejection. | S2 binding storage/identity; S3 physical resolution |
| Q3 | **Deferred by human acceptance r18:** transformed-export drift policy, freshness TTL and stale-cache policy belong to future scope. Original-document preview is the accepted initial export, independent of observation. Future recommendation remains refusal of ambiguous resolution/unsupported translation without silent field deletion. | Product/operator / future transformed exports; not initial-scope closure |
| Q4 | **Deferred by human acceptance r18:** optional Cube and new-document generation are outside initial delivery. If reopened, choose aggregate/dialect subset and document version/ID policy; require an independently executed Cube oracle, never default unknown expressions to sum. | Product/domain / future S4 output; not initial-scope closure |
| Q5 | **Accepted 2026-10-01:** refuse selected denied/revoked/unverifiable or oversized mandatory context before helper calls, preserving history; label optional denied/unavailable observation omissions and retain authorized meaning with zero denied reads; refuse revoked/unverifiable historical dependencies and require a fresh conversation, without silent dropping/redaction. Conservative numeric defaults are authorized design choices. | Settled by explicit YES; S5 implementation and verification |
| Q6 | **Live activation handoff retained by r18:** validate providers once explicitly authorized installations and target-specific nonhuman credentials are supplied through the existing secret mechanism. S6 fixture-backed implementation is complete; no live compatibility or production proof is claimed. | Operator/security / authorized targets and credential handoff, then live validation |
| Q7 | **Model settled in r5:** whole-contract access granted to existing Aster teams, default-deny. Observation and execution checks remain independent, so an authorized contract remains usable when its catalog is inaccessible. Mapping storage/wire and exact denial/revocation response details remain implementation design/open as needed; no object-level grant model or implicit access from validity/binding. | Model accepted by user; implementation design: architect/security / S3, S7 and S5; does not block r8-authorized S1 after documentation RV |
| Q8 | **Outcome settled in r7:** contracts and semantic context materially inform existing manual/AI-assisted query building and reviewable drafting, beyond browsing/selection. Use authorized selected meaning, field definitions, declared roles/expressions, grain/relationships if declared, provenance and available explicit bindings. Surface missing/ambiguous bindings; invent no physical identifiers, joins or aggregations. Remaining: exact selection/drafting UX and presentation; budgets/error policy remain Q5. No semantic execution engine, mandatory Cube, universal executability, guaranteed SQL without bindings or automatic execution. | Outcome accepted by user; UX design: product/architect / S3 preparation API, S7 UI and S5 assistance |

### Pending policy examples (not agreed or bound feature scenarios)

R5 adds concrete unautomated team-access and denied-observation examples to the
owning feature and gate manifest. Remaining seams below retain unresolved
response/preparation details; they do not reopen the accepted Q7 model or claim
the user approved exact response formats or revocation mechanics:

| Example | Proposed observable seam / owner |
|---|---|
| Authorized contract; separately authorized observation unavailable | S3 API list/detail, S7 browser and S5 helper capture retain authorized declared meaning and provenance, label unavailable observation, and never invent physical evidence. |
| Contract access revoked after an allowed control | Reuse the same principal/session/cache: the next S3 request, S7 refresh/request and S5 turn disclose no revoked contract meaning, including replayed reference/context; no catalog fallback bypasses revocation. Capture an allowed sentinel first so absence is meaningful. Exact revocation mapping and response policy await Q7. |

R7 moves Q8's settled outcome into B8 and the S3/S7/S5 draft scenarios below;
only exact UX/presentation remains design, not a renewed outcome choice.

Fog: deployed contract inventory, consumers of legacy JSON responses, exact SQL
dialects/quoting rules, representative catalog sizes, and acceptable observation
age. Resolve inventory and fixture requirements next; do not infer product policy
from permissive current code. Metadata admission today is conservative: any
protected binding hides catalog metadata globally. This plan must not silently
relax that rule or claim principal-aware backend metadata authorization exists.

## Bounded party exchange and grill

Direct, role-separated elicitation; these are review perspectives, not human
votes or independently spawned agents. Two rounds only.

### Round 1: authority and immediate boundary

**Product:** “ODCS-first should let an analyst see an agreed revenue definition.
Architect, can catalog columns fill its missing details without another model?”

**Architect:** “Only as a comparison projection, not a contract rewrite. Your
request needs object bindings: two catalogs can both have `sales.orders`.
Tester, the current parse-back tests cannot validate that binding.”

**Tester:** “Nor do they validate ODCS: a parser accepting its own invalid output
is a circular oracle. Security, a denied-request test can pass merely because
the fake helper is unreachable. I need a captured allowed control and denied
metadata-call counters.”

**Security:** “Agreed on evidence, but Product's enrichment must wait. The
conversation route bypasses the metadata guard and loses the admitted workspace
store. Architect, a better resolver alone cannot fix the global notebook read.”

**Product:** “Then safety first, but I have not chosen whether ordinary chat
should stop when optional metadata is unavailable.”

Outcome: retain D1; prioritize S1 and require a fake HTTPS capture with both
forbidden sentinels and a working allowed control. Disputes retained as Q2/Q5;
no vote chooses refusal versus omission for the final product.

### Round 2: preservation, compatibility and useful delivery

**Architect:** “Retain the source document and project supported objects; a
complete ODCS type system is unnecessary. Product, strict parsing will reject
our checked-in example's physical types in `logicalType`.”

**Product:** “Do not silently lose that deployment on upgrade. Tester, can we
make migration visible without calling old files ODCS-valid? I still want
optional Cube output, but its release priority is not settled.”

**Tester:** “Yes: explicit format/migration status and separate fixtures. But
Architect's `SUM` translation needs an independent Cube query result, not only
YAML assertions. Paging failures must not look like a complete catalog.”

**Security:** “And raw preservation must not mean raw disclosure to a helper.
Examples, arbitrary properties and descriptions are untrusted material; bound
and authorize the selected projection, keep credentials and other teams out.
Tester, UTF-8 cases must measure bytes independently of the implementation.”

**Architect:** “Then schema validity, supported semantics, binding, observation
and authorization are separate states. A commit pin is necessary: upstream
explicitly says the v3.2 filename is rolling.”

Outcome: S2 preservation/offline pin, S4 independent schema/Cube gate and S5
bounded projection are necessary. Q1/Q4/Q6 stay open. Second grill corrects a
false assumption: the official v3.2 schema permits earlier `apiVersion` values,
so Aster's v3.2 intake policy must check the selected version separately.

## Historical PL target through r18 (r20 governs physical accessibility)

The remaining [draft feature](../../crates/server/features/odcs-ai-catalog.feature)
is `@unautomated`; its 19 scenarios have not run. The three approved S1 scenarios
moved into [the bound S1 feature](../../crates/server/features/odcs-s1-bound.feature)
and have observed RED/GREEN on Build-host. Later drafts do not answer open policies.

- B1: Metadata admission precedes retrieval and provider disclosure. A denied
  catalog or contract never reaches the helper. Workspace index material comes
  only from the admitted team/session/personal workspace snapshot; cell scope
  never expands to other cells. Helper authorization is a separate check.
- B2: ODCS intake preserves original bytes and a parsed raw tree, document
  `id`, contract `version`, `apiVersion`, object/property IDs when present,
   names, `physicalName`, all schema objects, servers, semantic metadata, nested
   structure and standard sections from the full compiled document. Artifact
   identity (bundle/version/path/digest) stays separate from provider-qualified
   physical bindings and document identity. No author-tree lookup is needed. The typed
   projection implements only what Aster consumes. R6 requires a separate
   `apiVersion: v3.2.0` support gate; schema-valid v3.1 is rejected from intake,
   never converted inside Aster. Valid-but-unsupported content within v3.2
  stays preserved and labelled, not “fully supported.” YAML formatting is kept
  by retaining source bytes, not promised by reserialization.
- B3: Physical resolution uses exact structured identity and an explicit binding;
  contract discovery uses its separately authorized scope, not a mandatory live binding.
  `['sales.eu']` differs from `['sales','eu']`; browse IDs differ from engine
  aliases. Ambiguity is explicit and never resolved by HashMap order, substring,
  case folding or the first contract. Missing optional ODCS IDs are not invented
  as stable IDs; binding by an approved exact path requires duplicate detection.
- B4: Declared properties and observed columns remain separate. A comparison
   carries compiled artifact path/content digest (Git revision only when known),
  contract/object identity, catalog identity, observation time/schema ID when
  supplied, mapping evidence and drift. “Unavailable” is not “empty” or “fresh.”
- B5: Full-document ODCS output preserves all objects, servers, nested content,
  standard sections and existing version/identity, and validates offline against
  the pinned schema; returning unchanged original bytes is sufficient. This is
  distinct from a selected-object derived Cube projection. Physical types stay physical and logical
  types use the standard enumeration. Optional Cube output either translates a
  supported measure faithfully or reports unsupported semantics. Rendering
  returns a reviewable preview; it never writes, publishes or executes it.
- B6: Legacy SQL assistance and notebook/cell chat use the same authorized
  bounded reference builder. The builder receives selected scope, principal
  and authorized object references; raw retained documents are not wholesale
  prompts. Reference material is untrusted and transient, with provenance and
  explicit omitted/truncated status. User/assistant history remains governed
  by existing persistence rules; transient reference is not saved as history.
- B7: Adapters return the current schema and complete bounded list or an
   explicit incomplete/error result. UI exposes declared/observed/support/drift
   states through the same authorized resolution as API and AI.
- B8: Discovery and browse start from authorized contracts and semantics, not a
  physical catalog scan. Query preparation and AI use that same primary surface.
  A contract remains discoverable when its separately authorized observation is
  unavailable; expose the unavailable state without inventing physical evidence.
  R5 grants whole-contract access to existing Aster teams, default-deny, with
  concrete unautomated examples. Separately denied observation makes ZERO catalog
  calls and discloses no observed material while admitted contract meaning remains
   usable. R7 requires meaning and declared semantic context to inform manual/AI
   query drafting and review, with provenance and available explicit bindings.
   Missing/ambiguous bindings stay visible; never invent physical identifiers,
   joins or aggregations. Preparation makes ZERO execution calls. Contract scope is required
  even for unbound contracts; contract validity
  never grants access. Query execution retains engine grants/backend policy.

### Validation and legacy compatibility boundary

Pin official `schema/odcs-json-schema-v3.2.0.json` at upstream commit
`d3e1cb3e69849e05c9a7522abed9b27fb9af50d7`, SHA-256
`edb41f33ec46e84780e99872ab2bd67f074959d2bf3e9c9fc54e61f8982b0d93`.
S2 will vendor the exact bytes and license/receipt under existing test-fixture
conventions, verify digest, and disable remote reference resolution. The schema
uses draft 2019-09; its required top-level keys are `version`, `apiVersion`,
`kind`, `id` (not `schema`). Do not strengthen that claim by citing the prose
page, which calls schema required. Empty/no-object contracts may be schema-valid
but unusable by Aster's object resolver; report semantic support separately.

No validator dependency is added here. Future S2 must justify a maintained
draft-2019-09 Rust validator, pin it in Cargo.lock, disable network/file resolver
features and demonstrate offline operation. Serde only parses syntax; reusing it
does not replace JSON Schema validation. Keep core pure: input bytes/values in,
diagnostics out; loading and blocking validation orchestration belong in server.

R6 settles first-release intake as v3.2 only. Explicitly check
`apiVersion: v3.2.0` separately from the official schema, whose enum also accepts
older versions. A schema-valid v3.1 document is unsupported and rejected from
intake; a v3.2 document failing validation never falls back to legacy. Conversion
of reference v3.1 or legacy-flat artifacts must happen upstream before intake,
outside Aster, not by relabelling or silently rewriting documents. The earlier
temporary legacy-reader/conversion-in-Aster proposal is superseded for this
release. Inventory compiled bundles and response consumers before rollout;
retain original artifacts for rollback. No inferred measure aggregate, generated
stable ID or silent contract-version bump. Bundle selection/pins, invalid or
unsupported input startup-versus-quarantine handling, exact errors/statuses and
old response negotiation remain Q1; r6 approves none of those details. Never
downgrade by truncating a new document.

## IP: vertical delivery and honest gates

All commands below are **planned execution**, not executed results. User correction
on 2026-09-28: "always build on build-host. Cancel builds on my pc". All builds,
toolchain acquisition and build-backed tests use the declared devenv on Build-host
only, in an isolated exact-source snapshot with verified hashes. Never overwrite
existing remote work or copy secrets, `.git`, ignored files or build trees. Only
the disposable scoped test stack is authorized, not existing-stack deployments.
Documentation RV `ses_f1782bc2bffeAhfnGJ3S7hiCru` was relayed without blockers;
S1 alone may proceed, assertion RED before production edits, then independent RV.
Every future nontrivial recipe uses Bash with `set -euo pipefail`.
Existing runnable recipes at the base include `just ci`, `just features`,
`just ai-registration-validation`, `just catalog-routing-validation`,
`just backend-identity-validation`, `just notebook-isolation-validation`,
`just polaris-generic-adapter-validation` and `just conversation-browser`.
`just features` is shape-only. S1's `odcs-context-boundary-validation` and
`odcs-compose-smoke s1` now exist and pass on Build-host; later recipes remain planned.

### Gate implementation contract (applies to every slice)

The exact invocation is `devenv shell -- just <recipe>` as listed per slice.
Each planned recipe first lists its exact Rust test target and asserts every
named test below appears as `<name>: test`; then runs the whole target (no
filter that could silently run zero tests). External fixture drivers must assert
nonzero cases and their outcomes, fail on missing prerequisites, and return
nonzero on skipped/incomplete evidence. When binding BDD, assert the named
scenario manifest, positive scenario count, zero failures/skips and no remaining
unbound steps. The server runner filters tags at feature level
(`crates/server/tests/contracts.rs:2217`), not scenario level. In each test-first
slice, move its approved scenarios into a separate owning server feature tagged
`@contract @odcs-sN-bound` without `@unautomated`; leave all unapproved scenarios
in the draft. Never try to override the draft feature tag on a scenario.
Binding alone is not evidence of passing.

Every slice gate MUST also run its exact owning BDD command below. Capture its
report with `tee` under `pipefail`; assert a positive executed/passed scenario
count, zero skipped/failed scenarios, and every exact scenario name in its row.
Only after these assertions, that slice's other checks and Compose smoke may
the slice print its marker. These features/steps are planned, not implemented.

| Slice / separate feature under `crates/server/features/` | Exact runner inside the planned recipe | Required scenario names |
|---|---|---|
| S1 `odcs-s1-bound.feature` | `cargo test -p aster-server --test contracts -- --tags @odcs-s1-bound` | `Denied metadata stays off the wire`; `Mixed unbound catalogs are never retrieved`; `Notebook index belongs to the admitted workspace` |
| S2 `odcs-s2-bound.feature` | `cargo test -p aster-server --test contracts -- --tags @odcs-s2-bound` | `Compiled intake needs no author files`; `A multi-object contract retains its identity`; `Invalid v3.2 does not become legacy`; `Schema-valid v3.1 is rejected by the support gate`; `Namespace segments survive adapter and API boundaries` |
| S3 `odcs-s3-bound.feature` | `cargo test -p aster-server --test contracts -- --tags @odcs-s3-bound` | `Colliding table names do not select the first catalog`; `Declared and observed fields remain distinguishable`; `Existing team grants cover the whole contract`; `Contract access is denied without a granted team membership`; `Query preparation exposes selected meaning and unresolved bindings` |
| S4 narrow `odcs-s4-original-preview-bound.feature` | `cargo test -p aster-server --test contracts -- --tags @odcs-s4-original-preview-bound` | `Original ODCS preview preserves all content without observation or execution` (1 bound scenario; original-source portion only) |
| S4 remaining `odcs-s4-bound.feature` (planned) | `cargo test -p aster-server --test contracts -- --tags @odcs-s4-bound` (planned) | Remaining derived-preview coverage of `ODCS preview is independently valid`; `Semantic preview never guesses support`; `Catalog comparison and preview have no write side effects`; blocked on Q3/Q4 |
| S5 `odcs-s5-bound.feature` | `cargo test -p aster-server --test contracts -- --tags @odcs-s5-bound` | `Reference context remains scoped and transient`; `Denied observations stay out of authorized contract context`; `Query assistance uses semantic context in reviewable drafting` |
| S6 `odcs-s6-bound.feature` | `cargo test -p aster-server --test contracts -- --tags @odcs-s6-bound` | `Current Iceberg schema and all pages are observed` |
| S7 `odcs-s7-bound.feature` | `cargo test -p aster-server --test contracts -- --tags @odcs-s7-bound` | `Authorized contract browsing survives denied observations`; `Manual query building retains semantic context for review` |

R7 maps all 22 draft scenarios once: S1=3, S2=5, S3=5, S4=3, S5=3,
S6=1, S7=2. These are unautomated examples, not executed gates. Exact UX,
budgets and denial/revocation responses remain outside this manifest.

Every slice here affects server integration and additionally requires planned
`just odcs-compose-smoke sN` inside its gate. That future Bash recipe must isolate
the Compose project/ports and disposable data, call existing `just compose-up`,
assert `/healthz` succeeds, perform an allowed API control plus the slice's
denied/invalid counterpart through the actual server, then call existing
`just compose-down` and assert the isolated project has no running containers.
S1 checks forbidden metadata absence; S2 compiled-only intake; S3 contract-first
reads plus qualified resolution; S4 preview/no writes; S5 contract-first captured
scoped context; S6 adapter status; S7 contract-first browse/query preparation.
Capture and propagate assertion and cleanup failures; an EXIT cleanup trap may
not print success or hide failure. `odcs-compose-smoke sN OK` prints only after
explicit successful assertions and cleanup. Missing container infrastructure
makes the owning slice incomplete, not passed. This is a disposable development
smoke requirement, not deployment authority; no Compose execution in this edit.

The recipe prints its literal `<recipe> OK` using `printf '%s\n' '<recipe> OK'`
**only after** all target tests, fixture-driver assertions, regression commands
and cleanup assertions return zero. No marker in `finally`, traps, unconditional
shell separators or `|| true`. Missing target/recipe/toolchain is infrastructure
failure, not RED. RED is an observed assertion failure against the pre-change
implementation; record assertion, command, base and stderr. Deliberately break
one required assertion once to show the wrapper cannot print a green marker.

### S1: close metadata and workspace disclosure paths

**Owner/dependencies:** server/security; Q0 authorized in r8, documentation RV and
runnable verification prerequisites still required. No Q1-Q8 policy answer or ODCS redesign blocks S1.
**Behavior:** B1; feature scenarios “Denied metadata stays off the wire” and
“Notebook index belongs to the admitted workspace.”
**RED:** add `crates/server/tests/ai_context_boundary.rs` with exact tests
`denied_metadata_never_reaches_helper` and
`workspace_index_uses_admitted_snapshot`. Drive Connect `SendMessage`, legacy
`/api/ai` and legacy cell chat where supported. Fake HTTPS helper capture uses
a local test CA, not disabled certificate verification. Seed unique forbidden
contract/schema/global-workspace sentinels absent from the user's prompt; prove
allowed metadata and admitted-workspace sentinel appear in a positive control.
Use separate protected-denial and all-unprotected-positive configurations: the
existing protected policy closes metadata globally. Add a third configuration
with one explicitly unprotected bound catalog and one registered unbound catalog.
Assert zero unbound catalog calls and an allowed-schema positive control in that
mixed configuration. `require_unscoped_metadata` alone is insufficient because
one unprotected binding permits that coarse guard. Omit all unassociable contract
summaries until approved contract admission exists; an allowed schema is not inferred
contract authority. Assert denied catalog calls are zero and scan every outbound
message, not only the response. Two teams, two sessions and personal workspace use the same
notebook ID. Current code is expected to disclose forbidden enrichment and/or
fail the admitted-index positive assertion; no runtime reproduction is claimed.
**Minimal GREEN:** thread principal/admission and the selected store/snapshot
through current handlers; conservatively refuse unsafe enrichment using existing
metadata policy before any catalog or model call. Apply catalog-specific
admission to every adapter candidate as well as the coarse global guard; omit
unbound/unassociable contracts. Reuse admission, not duplicate it. Preserve team
cell rejection and do not loosen protected metadata policy.
**Exact planned gate:** `devenv shell -- just odcs-context-boundary-validation`.
Recipe runs `cargo test -p aster-server --test ai_context_boundary`, then existing
`just backend-identity-validation` and `just notebook-isolation-validation`.
Success-only marker: `odcs-context-boundary-validation OK`.
**Rollout/rollback:** separately reviewed security PR; exercise disposable
fixtures before any deployment. If the new path fails, disable enrichment or the
affected send path; do not restore known unsafe global retrieval for availability.

S1's omission of unassociable contracts is an interim safety boundary, not the
final discovery model. S3 may admit independently authorized contract scope only
using the accepted Q7 model once its mapping is designed; unavailable observation
is not unbound authorization.

### S2: consume full compiled artifacts and validate explicit formats offline

**Owner/dependencies:** core types + server intake; S1, Q1 bundle selection/pinning
and invalid/unsupported handling decisions, Q2 binding wire decisions. R6 settles
schema-version support as v3.2 only. S2 preservation
is internal intake, not admitted inspection; it does not depend on Q7 or S3.
**Behavior:** B2/B3; “Compiled intake needs no author files”,
“A multi-object contract retains its identity” and
“Invalid v3.2 does not become legacy” and
“Schema-valid v3.1 is rejected by the support gate.”
**RED:** new `crates/core/tests/odcs_documents.rs`, exact tests
`multiobject_roundtrip_preserves_identity` and
`invalid_version_and_legacy_are_distinct`; new
`crates/server/tests/odcs_intake.rs`, exact test
`offline_schema_validation_is_distinct_from_support`,
`schema_valid_v31_is_rejected_without_conversion` and
`compiled_bundle_requires_no_author_tree`. Supply only a copied/published bundle
with full documents under `compiled/`; make author files and sibling repositories
absent. Assert pure intake/preservation requires none of them. Add exact test
`compiled_bundle_selection_and_pins_are_explicit`: multiple full documents with
the same contract ID for different targets and versions, nested compiled entries,
and noncontract/intermediate files in the same bundle. Selection comes solely
from the supplied compiled bundle/config under the approved Q1 policy, never
from author hierarchy. Assert exact selected entries and independent artifact
identities, no ID-keyed overwrite, no first/latest-version guessing, and explicit
missing/ambiguous selection diagnostics. Exercise bundle version/digest and
selected document version/content-digest mismatches; mismatches never silently
select a substitute or reach consumers. Exact selection policy remains Q1.
Use two objects with
same-named properties, physical names different from business names, nested
properties, servers, semantic metadata, quality/team/description content and
optional IDs; preserve artifact identity separately from physical bindings. Include invalid
YAML, invalid UTF-8 input boundary, invalid enums, missing required keys, earlier
apiVersion, duplicate keys/ambiguous identities, valid unsupported fields, and
unchanged raw bytes. Current projection loses identities and accepts invalid
documents. Assert a v3.1 fixture passes the pinned official schema but fails
Aster's explicit support gate and never reaches consumers or an Aster converter;
use a valid supported v3.2 fixture as the allowed intake control. Do not bind an
exact error/status or startup/quarantine outcome before Q1's remaining answer.
Aster must reject ambiguous YAML parsing before validation.
**Minimal GREEN:** compiled-bundle loader, raw bytes/tree plus typed document/object/property projection,
source identity separate from document identity, pure schema validation and
explicit v3.2 apiVersion admission and loader diagnostics; no in-Aster conversion
or legacy intake fallback. Keep retained raw documents internal; existing
legacy response projections stay restricted or deny access where that cannot be
maintained. Adding raw fields to storage must not expand the public contracts API
or legacy serialization. Add `intake_preservation_does_not_expand_legacy_disclosure`
to `odcs_intake`, asserting preserved internal sentinels never appear in legacy
responses. Authorized inspection/projection is S3 using Q7's accepted team model,
not an S2 gate.
Do not implement hierarchy/manifest merging, base
injection or compilation; do not require upstream expansion fixes. S2 MUST include minimum lossless core port and
adapter identity transport before S3: preserve provider namespace arrays and
exact physical names through `Catalog`, adapters, server and API; a structured
namespace such as `['sales.eu']` must not collapse into `['sales','eu']`.
Add the integration assertion `namespace_segments_survive_adapter_boundaries`
to `odcs_intake`; reject ambiguous legacy strings instead of splitting dots.
S6 owns later pagination/auth quality, not this identity prerequisite.
Preserve namespace segments in core and add
backward-compatible proto fields only through `proto/aster.proto`; never reuse
field numbers. Legacy strings must not be guessed apart at dots. Review existing
fixtures and upgrade their validity claims rather than weakening the validator.
**Exact planned gate:** `devenv shell -- just odcs-document-validation`.
Recipe runs `cargo test -p aster-core --test odcs_documents` and
`cargo test -p aster-server --test odcs_intake`, including digest verification,
network-disabled validation and migration fixtures; then `just ci`.
Success-only marker: `odcs-document-validation OK`.
**Rollout/rollback:** inventory and upstream conversion evidence first, explicit
v3.2 bundle selection second, strict consumers last. Retain original Git artifacts and
prior reader-compatible response shape during the approved transition. Roll
back artifacts and reader together; never re-enable silent format fallback.

### S3: contract-first read APIs with authorized resolution and provenance

**Owner/dependencies:** core resolution + server/API; S2, Q2/Q3 and Q7 mapping design;
Q8's outcome is accepted; exact preparation UX/wire remains design.
**Behavior:** B3/B4/B8; whole-contract team access and default-deny scenarios,
“Colliding table names do not select the first catalog” and
“Declared and observed fields remain distinguishable.”
**RED:** `crates/server/tests/odcs_resolution.rs`, tests
`qualified_collision_and_multiobject_resolution` and
`drift_and_provenance_do_not_replace_observation` and
`contract_reads_do_not_require_live_schema`. The latter tests contract-first
list/detail reads under the accepted Q7 team policy, with a
separately authorized but unavailable observation and a denied-contract sentinel.
It must not infer admission from validity or expose unbound contracts implicitly.
Add `whole_contract_team_access_is_default_deny` for both S3 authorization
scenarios, including all objects and full-document preview admission, plus
`contract_reads_with_denied_observation_make_zero_catalog_calls`.
Proposed `contract_revocation_stops_next_request_disclosure` retains the
pending-ledger revocation seam; exact response details await design. Assert actual API responses and call
counters, including a populated cache, not just resolver output.
Two catalogs, two namespace
segment shapes, equal table names, two contracts and a multi-object document;
vary insertion order. Assert explicit ambiguity, exact successful binding,
observed-only and declared-only fields, type/nullability differences, missing
catalog, upstream refusal and unavailable observation. Current first-match and
wholesale replacement paths fail these assertions.
Add `query_preparation_exposes_selected_meaning_and_binding_gaps` in the same
target, binding the r7 S3 scenario. Assert API payloads carry the selected
`NET_AFTER_REFUNDS`, `NET_FIELD_DEFINITION`, declared role/expression,
`ONE_ROW_PER_ORDER`, `CUSTOMER_REFERENCE_ONLY`, provenance and explicit binding
`warehouse.sales.orders.net_amount`. Repeat with missing and ambiguous bindings:
retain meaning, expose the gap, no fabricated identifier/join/aggregate and ZERO
engine calls. A denied contract's `DENIED_QUERY_MEANING` is absent; observation
denial gives ZERO catalog calls. These are fixture sentinels, not product policy.
**Minimal GREEN:** contract-first read APIs and one deterministic authorized
resolver/comparison used by query preparation, RenderSemantic and future context/UI.
Discovery enumerates admitted contracts independently of catalog availability;
physical browse remains fallback/expert access under its own admission. No
semantic query execution is added. Preserve stable IDs, artifact digest and
observation provenance; no global name scan or automatic contract mutation.
Unknown/stale/incomplete observations cannot claim “no drift.” Authorization
must occur before fetching candidates or revealing forbidden candidate names.
**Exact planned gate:** `devenv shell -- just odcs-resolution-validation`.
Recipe runs `cargo test -p aster-server --test odcs_resolution`, then
`just catalog-routing-validation` and `just odcs-context-boundary-validation`.
Success-only marker: `odcs-resolution-validation OK`.
**Rollout/rollback:** expose contract-first read APIs first; reject ambiguous legacy
requests with actionable qualified-selection guidance. Rollback removes new
consumers, retaining strict identity/admission; do not reinstate first-match.

### S4: validate ODCS previews and prove optional Cube measures

#### Accepted narrow original-source preview, 2026-10-02

Human direction: "Minimal reuse if GetContract already full raw bytes plus
authorization meets preview don't duplicate endpoint." Also: "Don't invent drift
export policy original-source preview clearly original declared document independent
observed no transformed output." Q4 optional Cube subset/new-document generation
remains undecided, explicitly handed off rather than abandoned or counted green.

Source inspection found S3 GetContract already returns the entire parsed tree with
fresh whole-contract team admission and provenance, but not stored source bytes.
The accepted delta is `contextJson.originalSource`: the exact UTF-8 source from the
validated/pinned artifact, including comments and formatting. `declared` and
`provenance` retain their existing meanings. No new endpoint, conversion, generated
identity/version, catalog dependency, drift/export rule, writes or execution.
An object pointer neither truncates the document nor narrows the required grant.

**Narrow gate:** `devenv shell -- just odcs-original-preview-validation` on Build-host.
Exact scope: **1 Rust test / 1 bound scenario / 1 step / zero skips** in
`odcs_preview::full_document_preview_preserves_all_content` and
`odcs-s4-original-preview-bound.feature`. It compares original bytes and the whole
tree, all objects/servers/nesting/standard sections, validates the actual response
against the pinned published schema with an invalid-type negative control, and
asserts provenance, fresh denial/revocation, no catalog requirement/calls, no engine
invocation and unchanged files. Repeat the endpoint test in a network namespace;
run S3's regression chain (S2/S1, CI and isolated Compose). Success-only marker:
`odcs-original-preview-validation OK (whole S4 pending Q4)`.

**Review complete:** independent source-only RV `ses_f04fc5bd4ffe252GUe2zkjtRIW`
reported no verified findings or found regression; the user approved narrow preview
completion. No runtime reruns. Exact bytes refer to decoded UTF-8 `originalSource`,
not its JSON-escaped wire representation; intake limits are not a response-size
guarantee. **Remaining handoff:** Q3 drift blockers concern
future transformed output, not original declared-source inspection. Q4 still needs
an explicit inclusion/subset/dialect decision, new-document ID/version policy and,
if Cube is chosen, the independently executed pinned Cube oracle below. The full
`odcs-semantic-validation` gate below remains planned and cannot be claimed green
by this narrow gate. Q6 remains a separate authorized-target/credential handoff.
Rollback the additive source field if needed; retain existing admission and pins.

#### Remaining whole-S4 design (not delivered by the narrow gate)

**R18 disposition:** explicitly deferred by the user beyond initial scope. The
design and planned gate below are preserved for future work, not executed evidence.

**Owner/dependencies:** core semantic emitter + server render; S3/S7, Q4.
**Behavior:** B5; “ODCS preview is independently valid”,
“Semantic preview never guesses support” and
“Catalog comparison and preview have no write side effects.” Cube subset cases are designed only
after Q4 is answered; the draft asserts no particular aggregate support.
The original full-document endpoint proof is covered by the narrow gate above.
Any future selected-object Cube output is a separate derived projection; it cannot
truncate the ODCS original or narrow Q7's required whole-document admission.
**Remaining planned RED:** `crates/core/tests/odcs_semantics.rs`, tests
`odcs_export_validates_against_published_schema` and
`unsupported_aggregate_is_not_sum`. Assert description object, physical/logical
type separation, version and IDs retained, names with punctuation, no observed
column loss disguised as authoritative semantics, and unique suggested paths
across catalogs/namespaces. Current output fails official validation and guesses
sum. Future `tests/odcs-cube-execution.py` must compile generated YAML in a
pinned disposable Cube release and query rows with totals 10 and 20 (expected
30, independently read from the fixture); inspect generated SQL and grouped/null
cases to catch double aggregation. An unsupported expression must refuse export.
Add `crates/server/tests/odcs_preview.rs`, exact test
`preview_does_not_publish_or_execute`, asserting actual comparison/preview
requests leave engine invocations and Git/catalog writes at zero. The shared
draft comparison/preview scenario belongs to S4, so S7 need not wait for exports.
**Minimal GREEN:** full-document ODCS preview plus explicit supported translation.
Returning the unchanged original bytes is sufficient for a valid supported
document after full-document authorization and offline validation; do not rebuild
it from the selected object's typed projection. Preserve all other objects and
standard/unsupported content. Keep selected-object derived Cube output separate.
For an approved `SUM(column)` subset, choose the Cube representation proven by
execution (for example column expression plus sum type), not string-prefix
guessing. Optional Cube failure never prevents authoritative ODCS preservation.
**Exact planned gate:** `devenv shell -- just odcs-semantic-validation`.
Recipe runs `cargo test -p aster-core --test odcs_semantics` and
`cargo test -p aster-server --test odcs_preview`, then
`python3 tests/odcs-cube-execution.py` and `just odcs-resolution-validation`.
Success-only marker: `odcs-semantic-validation OK`.
**Rollout/rollback:** preview only. If Q4 defers Cube, explicitly split S4 into
ODCS delivery and a blocked Cube handoff, revise this gate/marker and acceptance
scope before implementation; never silently skip execution and claim S4 passed.
Disable failed target output on rollback; Git source remains authoritative.

### S5: make contract semantics primary in authorized, bounded AI

Independent RV correction: eight unique contract/object selections means the
union of saved history and the proposed exchange, checked before helper IO.
Repeated selections do not consume a slot. The same read/write invariant covers
the 32-catalog dependency cap. Failed cell sends invalidate the cached transcript
and read generation while preserving the composer and stored history. Both P2s
from `ses_f05b08380ffePmVosl4XKql3ZF` have observed Build-host RED/GREEN and a full
gate receipt; no new reviewer approval is inferred.

**Q5 acceptance (2026-10-01, current implementation session):** The user
explicitly answered YES to all three policies and authorized continuous S5
implementation without a renewed approval pause:

- Refuse BEFORE contacting the helper if an explicitly selected contract is
  denied/revoked, authorization is unverifiable, or mandatory selected context
  exceeds bounds; leave saved history unchanged.
- Optional denied/unavailable observations are labelled omissions; retain
  authorized meaning and make zero denied-catalog calls.
- If a prior contract-dependent exchange is no longer authorized, refuse replay
  and require a fresh conversation; never silently drop or redact exchanges.

Conservative documented numeric defaults are authorized implementation choices.
This acceptance supersedes historical open-Q5 statements. Acceptance authorized
the work; the separate RV receipt records observed verification and exact hashes.

**Owner/dependencies:** server AI/conversations + core projection; S3/S7, Q5, Q8 UX and Q7 mapping/response design. Q8's outcome is accepted.
S4 is required only for optional Cube references; AI must work without Cube.
**Behavior:** B6/B8; denied-observation context and r7 query-assistance scenarios and
“Reference context remains scoped and transient.” Q5 is now accepted; concrete
budget and history checks are bound in the S5 target and documented in `docs/ai.md`.
**RED:** `crates/server/tests/odcs_ai_context.rs`, tests
`all_ai_entrypoints_use_authorized_context` and
`multibyte_and_malformed_context_respect_budgets` and
`contract_semantics_are_primary_without_live_observation`. The latter captures
authorized declared meaning when separately authorized observation is unavailable,
labels its absence, and excludes denied contract sentinels under Q7. Test
`contract_context_with_denied_observation_makes_zero_catalog_calls` binds the r5
context scenario; proposed `contract_revocation_stops_next_turn_disclosure` retains the pending-ledger seam
to actual helper captures, including reused history/cache. Keep authorized meaning
with ZERO catalog calls on observation denial; revocation stops subsequent
reference disclosure/replay. Q5's 2026-10-01 acceptance settles turn refusal,
optional omission and unverifiable-history handling; it is not an open approval gate.
Capture actual HTTPS requests
for legacy assist, notebook and cell conversations with personal/shared helper
authorization where enabled. Include oversized first field, multibyte strings,
many properties, malformed adapter payloads, prompt-like descriptions, denied
catalog, duplicate names and timeouts. Independently count serialized reference
bytes; verify allowed positive control, forbidden sentinel absence, deterministic
ordering, request/time/item bounds and unchanged saved history on failed turns.
Current pre-append 16 KiB check permits an oversized item; old assist uses a
separate unbounded summary path.
Add `query_assistance_uses_semantic_context_for_drafts` in the same target for
the r7 S5 scenario. With a prompt containing none of the S3 fixture sentinels,
capture them, declared roles/expressions and available binding/provenance in the
actual helper request. A deterministic fake helper returns a reviewable draft
using the selected net definition/binding; assert that draft reaches the caller.
Change the selected definition to `GROSS_BEFORE_REFUNDS` with a different explicit
binding and assert request and returned draft change accordingly. Repeat with
missing/ambiguous bindings: explicit gaps, no invented identifiers/joins/aggregates.
Assert denied sentinel absence, ZERO denied-catalog calls and ZERO execution calls
throughout. This verifies context/draft plumbing, not arbitrary model accuracy.
**Minimal GREEN:** shared scoped contract/semantic-first builder, not a new
service or catalog-summary decoration. Select authorized contract meaning for
discovery and reviewable query drafting, feeding the existing SQL assistance path;
observations only supplement it. Surface unresolved bindings, not guessed SQL.
Budget source reads,
individual fields, object counts, intermediate allocation and final reference;
UTF-8-safe bounded serialization with provenance. Mandatory context is refused,
not truncated. Reference ceiling is 16 KiB including labels, independent of
existing 8 KiB prompt/cell limits; the helper JSON body is capped at 384 KiB.
History admission plus selected preparation has a 5-second deadline; optional
observation has a 1-second deadline and explicit read/byte caps. See `docs/ai.md`
for the full numeric defaults. Delimit untrusted content,
do not fetch URLs mentioned in contracts, do not append raw documents or execute
transform text. Recheck access per turn; no cross-principal cached projection.
**Exact planned gate:** `devenv shell -- just odcs-ai-context-validation`.
Implemented recipe runs the exact twelve-test S5 target with isolated PostgreSQL,
three bound scenarios/three steps and the exact-count report, then AI registration,
PostgreSQL persistence for contract/catalog ledgers and S7's gate. S7 transitively runs conversation-browser, S3/S2/S1 regressions,
full CI and their isolated Compose controls; S5 adds its own isolated Compose.
The transitive chain avoids repeating an unchanged full CI gate solely for S5.
Success-only marker: `odcs-ai-context-validation OK`.
**Rollout/rollback:** explicit selection opts into the new builder; compare
metadata/provenance, never real prompt contents in logs. Disable enrichment by
removing bundle configuration while retaining S5's dependency-aware history reader
and S1 guards. A pre-S5 reader ignores the new ledger and is not a safe downgrade
over grounded histories. No return to unscoped retrieval.

### S6: make catalog observations trustworthy

**Independent RV corrections, 2026-10-02:** Both reported P2s are fixed with
observed RED/GREEN and the full gate. `partial_polaris_environment_refuses_startup`
adds both-half startup refusals with zero upstream requests, plus both-absent and
complete-pair controls. `polaris_health_shares_oauth_deadline` verifies status-only
health shares the authentication deadline; `polaris_pages_share_bytes_and_elapsed_budget`
preserves cumulative byte/elapsed behavior across successful pages. The bound gate
now requires seven tests and one scenario/step. This does not reopen accepted
scopes or authorize Q6 activation/S4 work. Exact dispositions and hashes are in
the [RV record](odcs-ai-catalog-rv.md).

**2026-10-02 continuation:** The user authorizes the remaining S6 implementation
while S4's optional Cube scope is undecided. This settles coding authority, not
Q6: no real credentials, live provider targets or deployment are authorized.
Use the existing Aster task worktree, preserve earlier hashes/snapshots, observe
fixture assertion RED before production, and run builds/devenv/tests/Compose only
on Build-host. No nested agents, commits or pushes. Return an RV-ready verified
implementation and retain the explicit live-target handoff. The request also
requires inspection of S5's bounded-read port; that inspection confirms its safe
zero-IO omission remains valid because S6 does not require new AI provider support.

**Candidate contract details:** 4 MiB aggregate body bytes, 64 HTTP reads and a
10-second operation deadline; OAuth exchanges and nested metadata reads consume
the same budget. Current Iceberg schema uses its ID; the supported legacy case is
v1 `schema` with both modern schema fields absent. Supported metadata versions are
1–3. Iceberg uses `pageToken`, OM uses `after`; repeated/empty/malformed cursors
and exhausted bounds return errors, never partial success. `secret://KEY` in
existing token/credential fields uses the selected server `SecretStore` and the
controller's existing environment source. No token issuer/store is added. Cube
metadata authentication uses an issued API token, independently of S4 exports.
The [handbook](../catalogs-and-compute.md#s6-catalog-reliability-candidate) records
protocol sources and compatibility. S5 adapters deliberately keep bounded AI
reads unavailable; both AI paths perform zero IO for this omission.

**Owner/dependencies:** catalogs adapters + server secret wiring; S2 identity,
S1 admission, Q6 credential activation. Can be developed after S2; integrate
as a supporting branch, before claiming real-catalog readiness. It does not
block contract-first discovery against an unavailable observation fixture.
**Behavior:** B7; “Current Iceberg schema and all pages are observed.”
**RED:** `crates/catalogs/tests/catalog_quality.rs`, tests
`iceberg_current_schema_and_pages`, `provider_http_errors_are_not_empty_success`
and `openmetadata_pages_and_registered_auth`. Add server integration target
`crates/server/tests/catalog_credentials.rs` with exact test
`secret_selection_reaches_only_the_configured_catalog` to cover secret-provider
selection through registration, beyond direct adapter construction.
Fake HTTPS fixtures return old
schema first/current schema second, missing current ID, more than 200 OM rows,
multiple ordinary Polaris namespace/table pages, repeated/empty tokens,
malformed shapes, 401/403/429/500 and deadline/body-budget exhaustion. Assert
exact encoded namespace segments, no token in diagnostics or redirect targets,
registration-to-wire authentication and explicit incomplete results on limits.
Current schema selection, paging and status handling fail these assertions.
**Minimal GREEN:** reuse checked transport where applicable, select current
schema by ID with an explicit supported legacy metadata-version path, validate
required shape, follow provider-specific paging within bounds and wire approved
secret source through the central registry. Do not copy Generic Table's token
spelling blindly to Iceberg/OM or infer policy from a successful service token.
**Exact implemented gate:** `devenv shell -- just odcs-catalog-quality-validation`.
Recipe runs `cargo test -p aster-catalogs --test catalog_quality` and
`cargo test -p aster-server --test catalog_credentials`, then
`just polaris-generic-adapter-validation`, `just providers-check` and
`just catalog-routing-validation`, one S6 bound scenario/step with exact nonzero
counts and zero skips, `just odcs-ai-context-validation` (S1/S2/S3/S7, full CI and
their isolated Compose transitively), and isolated `just odcs-compose-smoke s6`.
The S6 Compose fixture asserts target-token routing, complete pagination and
startup refusal when the selected store lacks a required secret, followed by cleanup.
Success-only marker: `odcs-catalog-quality-validation OK`.
**Rollout/rollback:** fake upstream gate before separately authorized provider
smoke. Keep Generic Table runtime enablement and protected metadata gates
unchanged. Disable a faulty provider rather than return stale/empty success.

### S7: contract-first browse and query-preparation UI

**Owner/dependencies:** server web/API + docs; S3, relevant Q1/Q3/Q8 UX answers and Q7 mapping/response design.
Runs before S5; S4/S6 are separate preview/real-observation readiness gates.
**Behavior:** B4/B7/B8; authorized browse with denied observation and r7 manual
query-building scenarios; Q8 outcome accepted, exact UX remains design.
**RED:** `crates/server/tests/odcs_catalog_view.rs`, exact tests
`catalog_view_matches_authorized_projection` and
`contract_browse_prepares_without_live_schema`; future `tests/odcs-catalog-browser.py`
asserts the primary discovery/browse path starts with authorized contracts and
semantics, query preparation works with unavailable separately authorized
observation, and physical catalogs remain a supporting fallback/expert path.
Assert actual DOM labels for declared/observed fields, provenance, drift,
invalid/unsupported/unavailable states and escaped malicious metadata. Keyboard
navigation, labelled controls and narrow viewport remain usable. Assert denied
sentinels absent from DOM and network, with an allowed control. Engine invocation
and Git/catalog writes stay zero. Test
`contract_browse_with_denied_observation_makes_zero_catalog_calls` binds the r5
browse scenario; proposed `contract_revocation_stops_next_browser_request_disclosure` also exercises actual
DOM/network after an allowed control and refresh with the same session/cache.
Preserve authorized meaning on observation denial with ZERO catalog calls; after
contract revocation the next request discloses no revoked meaning. Q7's model is
accepted; exact denial/revocation responses and Q8 UX remain design.
Add `manual_query_building_retains_semantic_context` in the same target and browser
driver for the r7 S7 scenario. Select the S3 fixture, enter/edit a query draft and
assert the query-building/review DOM retains its definition, declared semantics,
grain/relationship, provenance and available binding; switching to the gross
fixture changes this context. Missing/ambiguous binding variants expose the gap
without invented identifiers/joins/aggregates. Check `DENIED_QUERY_MEANING` absent
from DOM/network, ZERO denied-catalog calls and ZERO engine calls, not just a
contract browse page or selection label. No exact widget/layout is prescribed.
Current UI lacks comparison states.
**Minimal GREEN:** contract-first navigation/detail/query preparation using existing
server-rendered patterns and S3 read APIs, no new frontend stack or semantic
query engine. Comparison supplements this primary UX. Additive proto fields serve shared
API consumers; legacy routes must converge or explicitly reject ambiguity.
Document actual compatibility policy and commands after they exist.
**Exact planned gate:** `devenv shell -- just odcs-catalog-ui-validation`.
Recipe runs `cargo test -p aster-server --test odcs_catalog_view`,
`python3 tests/odcs-catalog-browser.py`, S1-S3 gates,
and `just ci`. Success-only marker: `odcs-catalog-ui-validation OK`.
**Rollout/rollback:** read-only UI first; remove UI exposure on regression without
discarding raw contracts or reverting security boundaries. For future approved
server rollout, existing `devenv shell -- just compose-up` starts a stack and
is not itself a success assertion: verify health and representative authorized
flows, then `devenv shell -- just compose-down`. No stack runs in this stage.

### Implementation review and handoff

R8 requires separate independent RV of this documentation step before any code,
then RV after every implementation step before the next. Preserve each step's
pre-review diff/receipts, reconcile findings and rerun affected checks; author
consistency checks do not substitute for independent review. No delegation in
this documentation task; the next review is a separate handoff.

Primary delivery order is S1 -> S2 -> S3 -> S7 -> S5. Slice numbers
remain stable for review traceability. S4 branches from S3 for previews; S6
branches from S2 for observations; S4's comparison/preview browser surface also
requires S7. Neither optional Cube nor adapter hardening
turns the primary UX back into catalog decoration. Final integration must run
every approved slice gate plus `just ci`; real-observation claims require S6,
and derived-preview claims require S4. No PR exists or is published.
Inside each slice, land domain/adapter leaves before
server consumers. Bind and observe RED before implementation, preserve the
first implementation diff and receipts, obtain a fresh independent review,
rewrite backward from the supported outcome, remove obsolete scaffolding and
rerun affected gates. Final RV covers security, correctness, schema support,
compatibility, reliability, performance bounds, operations, UI accessibility and
documentation. CI success alone cannot replace HTTPS, published-schema or Cube
execution evidence. Missing live authorization is an explicit operator handoff,
not permission to use the platform or a preexisting deployment.

## Appendix A: discovery receipts

Read-only discovery snapshot D0, preserved before final artifact review. All
local line references are at base `eb397d8`; no tests were run.

| Receipt | Source / exact evidence |
|---|---|
| R0 checkout | `git rev-parse HEAD` returned `eb397d865cf2dc156fda172d50b13c98eda9d9f7`; status showed clean `docs/odcs-ai-catalog-plan`; worktree list confirmed the requested checkout. No local graph at `graphify-out/graph.json`; direct source traversal used without building one. |
| R1 rules | [AGENTS](../../AGENTS.md), [CONTRIBUTING](../../CONTRIBUTING.md), [justfile](../../justfile), [repo check](../../tests/repo-setup.sh), [API/test scope](../api-and-tests.md). Server runner [filter](../../crates/server/tests/contracts.rs#L2210) excludes `@unautomated`. |
| R2 AI path | [grounding/scan](../../crates/server/src/ai.rs#L344), [legacy guard](../../crates/server/src/ai.rs#L407), [send](../../crates/server/src/conversations.rs#L144), [metadata guards](../../crates/server/src/lib.rs#L859), [HashMap](../../crates/core/src/registry.rs#L38). |
| R3 workspace path | [RPC admission/send](../../crates/server/src/api.rs#L223), [selected store discarded](../../crates/server/src/lib.rs#L1573), [global reload](../../crates/server/src/conversations.rs#L177), [workspace send test](../../crates/server/tests/notebook_isolation.rs#L1195). |
| R4 contracts | [domain parser](../../crates/core/src/contract.rs), [loader](../../crates/server/src/contracts.rs), [checked-in example](../../contracts/orders.yaml), [catalog identities](../../crates/core/src/catalog.rs), [render handler](../../crates/server/src/api.rs#L667), [proto](../../proto/aster.proto#L292). |
| R5 semantics | [field replacement](../../crates/core/src/semantic.rs#L35), [Cube](../../crates/core/src/semantic.rs#L147), [ODCS](../../crates/core/src/semantic.rs#L175), [fallback aggregation](../../crates/core/src/semantic.rs#L250), [circular oracle](../../crates/core/src/semantic.rs#L419), [bound feature](../../crates/core/features/semantic-models.feature). |
| R6 adapters | [ordinary Iceberg list](../../crates/catalogs/src/lib.rs#L187), [Generic paging](../../crates/catalogs/src/lib.rs#L214), [namespaces/current schema](../../crates/catalogs/src/lib.rs#L316), [Cube status](../../crates/catalogs/src/lib.rs#L454), [OM transport/paging](../../crates/catalogs/src/lib.rs#L597), [OM registration](../../crates/catalogs/src/lib.rs#L938). |
| R7 evidence gaps/UI | [single-catalog capture](../../crates/server/tests/conversations.rs#L375), [legacy deny tests](../../crates/server/tests/backend_identity.rs#L270), [catalog UI](../../crates/server/src/web.rs#L477), [provider ownership](../provider-matrix.md), [architecture](../architecture.md). |
| R8 discovery docs | [AI](../ai.md), [catalogs](../catalogs-and-compute.md), [semantics](../data-contracts-and-semantics.md), [historical study](../metadata-compatibility.md). The historical “real v3.2 example” and parse-back claims do not establish current conformance. Its OpenMetadata-authority future branch is not the selected direction. |
| R9 official ODCS | [v3.2 schema page](https://bitol-io.github.io/open-data-contract-standard/v3.2.0/schema/) read 2026-09-27: objects/properties; optional stable IDs and physicalName; logicalType enum `string/date/timestamp/time/number/integer/object/array/boolean/map/vector`; semanticType `column/measure/dimension`, aggregate expression example `SUM(revenue)`. |
| R10 fundamentals | [v3.2 fundamentals](https://bitol-io.github.io/open-data-contract-standard/v3.2.0/fundamentals/) read 2026-09-27: contract version distinct from apiVersion, description is an object. |
| R11 immutable schema receipt | [upstream version policy](https://github.com/bitol-io/open-data-contract-standard/tree/main/schema) says versioned filenames roll. `gh api repos/bitol-io/open-data-contract-standard/commits/main --jq .sha` yielded the commit pinned above. GitHub contents API at that ref returned bytes whose decoded SHA-256 is recorded above; JSON inspection found draft 2019-09, four required keys and an apiVersion enum including earlier versions. This is a source-byte inspection, not schema validation of Aster output. [Pinned blob](https://github.com/bitol-io/open-data-contract-standard/blob/d3e1cb3e69849e05c9a7522abed9b27fb9af50d7/schema/odcs-json-schema-v3.2.0.json). |
| R12 Cube oracle | [official measures documentation](https://docs.cube.dev/reference/data-modeling/measures) read 2026-09-27: measure type controls aggregation and the meaning of sql. No Cube process/query was run; version/digest and fixture driver remain S4 work. |

### D0 discovery conclusions retained

Initial risk reports were hypotheses. Direct tracing verified the missing
conversation metadata guard, global workspace reload, bare-name scan, flattened
contract model, emitter issues and adapter limitations. Corrections before
writing the coherent plan: actual retrieval symbol is `grounding`; Generic
Table pagination already exists; metadata policy is globally conservative;
versioned upstream schema URLs are mutable; official schema validity accepts
more apiVersion values than Aster's selected v3.2 support policy. No claim here
establishes exploitability in a deployed configuration or successful migration.

## Appendix B: RV of the planning candidate

**R4 review correction:** independent r3 review `ses_f1ab8829cffefXA3JcQf0narDN`
supplied four findings relayed by the user; r4 applies all four. The frozen r3
snapshot, hashes, dispositions and current checks are on the
[RV page](odcs-ai-catalog-rv.md). Historical r1/r2 review below covers that earlier
scope only. At r4, Q0-Q8 and implementation approval remained pending; r4 is not an
independent sign-off of its own corrections or runtime proof.

**Scope:** documents and draft behavior only, reviewed against seed/base; no
runtime code review completion or implementation approval implied. The author
first performed self-review using separate conformance, security/test,
compatibility/reliability and editorial/simplicity lenses. Independent architect
review `ses_f1b194920ffeAVrdTlVZ0G8Rfy` subsequently supplied eight findings;
r2 applies them. This is review feedback, not user approval or runtime proof.

Before r2 edits, the exact five-file r1 draft was copied with `cp --parents` to
`/tmp/opencode/odcs-ai-catalog-r1-ses_f1b194920ffeAVrdTlVZ0G8Rfy` after verifying
the parent. No duplicated tracked draft was created. SHA-256 receipts:

| File relative to snapshot root | SHA-256 |
|---|---|
| `docs/README.md` | `b21655a5c38b55ea684fab22f426c3d47af63f3aa03eaa5f7425d4cbb8a8c67d` |
| `docs/plans/odcs-ai-catalog.md` | `946a2355887c18cf1ef8dbacdef8dd2c26789ad24a59580ef3dc7ff7a5c9c991` |
| `docs/plans/odcs-ai-catalog-pl.md` | `cf0f9be31859d51a6f763e01c8586d0a79d033c867bd473f6fe40f6ceeccb58e` |
| `docs/plans/odcs-ai-catalog-ip.md` | `5755510a99c9be57f08e966a5f581c9af9e43ee51ade5496098620ec880b9b70` |
| `crates/server/features/odcs-ai-catalog.feature` | `26c4c9bac0efe6658c039d4af62d7eb499c57249d3be91566a17928108c08138` |

Frozen first-candidate propositions P0: urgent boundary before model work;
raw plus typed projection; one resolver; optional Cube; one shared AI builder;
adapter quality before final UI (superseded by r3). D0 above retains source receipts. R2
works backward from those supported outcomes rather than retaining competing
roadmaps. Review corrections: separate upstream schema validity from version
support; require namespace segments rather than dot splitting; require working
positive controls for sentinel absence; gate markers after assertions, never
after mere process startup; keep optional Cube deferral explicit and Q1-Q6 open.

Security review is applicable to proposed disclosure boundaries even though
this change is prose-only. Reliability/performance gates require bounded scans,
paging and UTF-8 construction. Compatibility review requires explicit migration
and additive proto evolution. Test review rejects parser-self-validation and
zero-test success. Accessibility is assigned to S7. Editorial review keeps one
canonical decision ledger and one slice sequence; no parallel RFC/ADR or
speculative new service is needed.

**Evidence status:** r1 executed no tests. R2 check attempts and actual results
are in the [RV one-pager](odcs-ai-catalog-rv.md); those checks cannot prove the
unimplemented behavior. All seven slice gates remain unmet future work; none is
abandoned. R5 settles Q7's model and r6 settles Q1's first-release compatibility;
r7 settles Q8's query-building outcome; r8 authorizes Q0 with per-step RV.
Remaining Q1 intake policy, Q2-Q6,
Q8 UX and Q7 design details remain open.
Human review is requested through the
[PL one-pager](odcs-ai-catalog-pl.md) and [IP one-pager](odcs-ai-catalog-ip.md).

### Parent coordination and manual review handoff

Historical parent-reported evidence (before r3; current correction sync remains
with the parent): platform sync completed in
`/home/developer/projects/platform/worktrees/aster-odcs-plan`, branch
`docs/aster-odcs-plan`, base `d792329`. It updates the existing 2026-09-14 proposal
with an appended 2026-09-27 decision, the Aster row in `docs/proposal-index.md`
and `docs/README.md`. The dirty main checkout remains untouched. Integration
reconciliation is still required. Platform `just test` is blocked by missing
ignored repository references; `just docs-check` reports 14 pre-existing
broken-link occurrences covering 13 missing targets. Its diff check passed.
These are parent receipts, not checks rerun from Aster.

Parent reports `cr` loaded and `tuicr` installed, but no CMUX/TMUX/ZELLIJ/HERDR
environment is available. Manual human handoff: run
`tuicr --file docs/plans` from this Aster task checkout. No session was launched
and no approval was received through that historical handoff. R8 now authorizes
execution; reconcile new independent RV comments against this checkout/r8 before
starting S1 code. Aster CI and the platform integration/check blockers
remain outstanding.
