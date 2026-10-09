# ODCS-first AI and catalog: IP

## Delivery r24: real benchmark notebooks, 2026-10-09

**Delivered:** all r24 slices and scoped live gates completed; current outcome,
exact receipts and remaining limits are in
[RV/E2E](odcs-ai-catalog-rv.md#r24-delivered-real-benchmark-notebooks-2026-10-09).
The following plan is retained as the delivery/evidence map.

Accepted seed/decisions: [canonical r24](odcs-ai-catalog.md#accepted-demo-continuation-r24-2026-10-09).
Reuse this task worktree without discarding prior uncommitted work. Build/test
only on Build-host in a new exact-source snapshot; retain October 2 source/image and
Helm values for rollback. Independent plan grill precedes implementation.

1. **Notebook activation:** fail a router test showing local mode with contract
   policy cannot reach notebook creation; add explicit local notebook selection,
   keeping default team restrictions, owner checks and contract-team membership.
   Verify fresh login landing, create/save/reload, stale-save conflict and other
   user's read/write denial. No GitHub sync claim.
2. **Real metadata:** fail adapter tests for native Trino metadata discovery and
   exact per-catalog schema/table selection; implement one bounded catalog adapter
   through existing core port/registry. Test pagination, malformed responses,
   credential/continuation origin boundaries and errors. Use live backend inventory
   as independent deployment oracle, not fixture names as evidence of reality.
3. **Browser interaction:** fail browser checks for explicit catalog context and
   populated structured contract fields; minimally extend existing notebook
   selector/Run path and contract DOM rendering. Retain original source, provenance,
   request generation guard and revocation clearing; render untrusted text safely.
4. **Reproducible demo:** declare isolated Trino native connectors; obtain all
   tiny-scale schemas/columns, generate one synthetic ODCS contract per table,
   pin manifest/bindings and preserve existing team/grant policy. Use public image
   and the existing isolated Helm deployment, not stack-bootstrap's older Aster.
5. **End-to-end delivery:** fresh independent security/reliability/UI review,
   revise verified findings, full `just ci`, `just chart-lint`, isolated Compose,
   then candidate image deployment in `aster-demo` only. Test actual
   Authentik login landing and browser notebook create/edit/save/reload/run for
   both catalogs, backend row-count controls, viewer denial and contract revocation.
   Roll back complete image/config pins if runtime checks fail, never disable
   authorization or remove the contract bundle to make checks green.

New focused owner recipes will print `BENCHMARK_NOTEBOOKS_OK`,
`TRINO_CATALOG_OK`, `BENCHMARK_UI_OK` and `BENCHMARK_DEMO_E2E_OK` only after their
assertions pass. Record exact RED/GREEN commands and counts in the RV receipt;
missing checks remain explicit blockers. Each same-failure repair budget is three.
No publication or standing-profile repair is part of this delivery.

**Independent plan grill:** `ses_ede6bad76ffetB0JM7TWCTQ0Vi` found legacy local
listing/reads lack owner isolation even though updates check ownership. Slice 1
must centralize owner admission across browser/REST/RPC and associated notebook
results/session/helper entrypoints before exposing local mode. PostgreSQL ownership
and Git content must both survive restart. Persist catalog/schema selection with
notebook content; test unqualified queries to prove routing rather than relying
only on fully qualified SQL. Slice 2 must verify all tables/columns from live
metadata, bounded per-table queries, hostile identifiers and off-origin nextUri.
Slice 3 must retain zero/false/nested values and render attacker-controlled text
literally. These findings refine accepted behavior without adding team/GitHub scope.

**Current stage / revision:** IP / r23, materialization-level mock contracts verified
on Build-host; the three S8 reviewer fixes also pass the expanded gate. Accepted rules and
verbatim seed are in [canonical r20](odcs-ai-catalog.md#human-correction-r20-physical-inventory-ownership-and-mock-contracts-2026-10-02).

1. **Fixture step (r23 correction to reviewed r20):** five complete pinned ODCS v3.2 contracts,
   one per current materialized mock table; existing manifest/binding/grant formats; three
   descriptive schema semantics sidecars. Reuse the existing offline validator;
    verify complete inventory, raw bytes and table-grant sibling isolation. Schema
    semantics stays schema-level with per-table contract references. General ODCS
    multi-object support stays intact; future materializations are not necessarily tables.
    No compiler, mandatory SQL policy or rollout.
2. **Owner-scoped physical inventory:** server-owned versioned owner-group mapping
   keyed by exact catalog/namespace segments and fresh identity; unify UI over
   physically observed entries with independent contract/semantic facets. RED
   nonowner enumeration/detail/AI leaks and revoked membership; GREEN owner-only
   uncovered metadata and explicitly admitted contracted metadata across UI/REST/RPC.
   Contract-only artifacts are never offered as accessible data. Unknown existence
   is unavailable, not absent; choose authoritative proof/freshness before coding.
3. **Shared data admission:** all query and sharing entrypoints require physical
   presence, coverage and existing grants, including owner/admin controls. First
   establish backend/resolved-plan authority for every target and view dependency;
   no SQL string inspection shortcut. RED selected-contract-plus-unrelated-target,
   owner/admin uncovered queries and revoked grants; GREEN covered authorized
   targets with engine/backend grants still independently enforced. This is a
   genuine new runtime slice; do not disguise fixture loading as enforcement.
4. **Semantic context publication/projection:** implement the precise bounded
   per-object compiled custom-property proposal in the fixture README only after
   review. RED association mismatch/cross-contract disclosure and unsupported
   measures; GREEN admitted source-pinned context in manual and AI preparation,
    with no semantic execution claim. A later Cube executor needs its own oracle.

**S8 execution boundary:** step 2 now has a candidate for configured compiled
bundles. Its `ListCatalogInventory` and legacy metadata adapters share exact schema
owner/contract admission; the inventory page projects those facets. Existence is
fresh metastore registration, not a query permit. The no-bundle development mode is
not migrated by this slice, and removing a bundle is not a safe policy rollback.
R21's gate passed before independent RV `ses_f029ed1c9ffe2awvOuIQ06x5Xh`. R22 adds
indistinguishable schema probes, one identity snapshot and nested semantic controls.
The r22 gate completed successfully. R23 adds the actual mock table-grant isolation
case. The mock fixture and six-case canonical S8 gates pass. Per-table work and S8
fixes are complete after source-only RV `ses_f01458357ffeG13RoBziy3aSeB` (no findings/new runs), before remaining
global activation/UX work and step 3 authoritative query/share admission. No current
claim that owners or admins are prevented from arbitrary uncovered SQL. S8 owns
six Rust tests and six bound scenarios/six steps; no r20 policy-wide GREEN.

Owner: Aster for runtime and tests; publication stays upstream. Run applicable
RED/GREEN, full CI and isolated Compose on Build-host for runtime changes, independent
RV after each step. Before any reviewed demo activation, supply a real configured
team/current identity path: dev-login Alice has no group authority. Roll out exact
bundle/environment pins only after review and explicit deployment authorization;
rollback restores prior pins/config, not a permissive bypass of the new policy.
Fixture rollback simply removes this unactivated example. Q3/Q4/Q6 scope is unchanged.

## Historical r19 impact record (superseded by r20)

**Historical stage / revision:** IP impact record / r19; new execution plan blocked
on [Q9–Q11](odcs-ai-catalog.md#human-correction-r19-unified-catalog-and-access-scope-2026-10-02).
The destination is a unified catalog inventory with independent physical,
contract and semantic coverage. R18's completed slices remain historical;
this correction is not a cosmetic navigation-only follow-up.

Source tracing finds reusable whole-contract team admission and separate
observation/engine checks, but no query contract requirement or private-product
authority. Once discovery/query scope and exceptions are settled, revise slices
around inventory identity/facet evidence, scoped listing/detail, and (if required)
shared REST/RPC query admission. A selected contract must not serve as proof of
coverage for arbitrary SQL. Retain default-deny team grants, engine grants and
backend policy; semantics does not grant access.

Future verification must cover combined and missing/unknown facets, contract-only
entries, denied disclosure, revocation and agreed exception boundaries through
UI/API and shared query paths, with allowed controls. Those are planning seams,
not an approved ACL matrix or executed tests. No production/test implementation
or build in r19; all future builds and build-backed tests remain Build-host-only.
Feedback binds to canonical r19 in the same task checkouts/bases recorded in PL;
no new independent review approval. Q3/Q4 deferrals and Q6 handoff remain intact.

## Historical IP record through initial-scope closure

**Stage / revision:** IP / r14
**Status:** Initial implementation scope complete after prior per-step RV and explicit human acceptance. [Canonical r18](odcs-ai-catalog.md#human-acceptance-r18-initial-scope-closure-2026-10-02) accepts original-document preview as the initial export and explicitly defers remaining S4 transformed exports, optional Cube and new-document generation. Q3/Q4 are future scope, not unmet initial gates; the broader semantic/Cube gate remains unimplemented/unrun, not passed. Q6 is an authorized-target/credential handoff for live validation, not production proof. Build-host/review receipts and platform blockers remain unchanged; earlier stage statements below are historical.
**Owner / date:** Aster maintainers / 2026-10-02 UTC
**Basis:** [PL candidate](odcs-ai-catalog-pl.md), original seed and selected
direction in the [canonical plan](odcs-ai-catalog.md).

## Outcome and why it matters

S4 narrow continuation, 2026-10-02: reuse the admitted GetContract tree/provenance
and add only its missing original UTF-8 source. Build-host observed the missing-byte
assertion before production edits. `odcs-original-preview-validation` counts one
test/scenario/step and cannot satisfy the planned whole-S4 semantic/Cube gate.
The [accepted scope and handoff](odcs-ai-catalog.md#accepted-narrow-original-source-preview-2026-10-02)
keep original declaration separate from observed facts and future transformed output.

S6 continuation, 2026-10-02: coding authority is explicit. The implementation uses
current Iceberg schema IDs, bounded provider-specific pagination/transport and
existing-store target credentials. Its exact seven-test/one-scenario gate includes
S5's full transitive regression chain and isolated S6 Compose. S5's variable-budget
AI port remains unavailable with zero IO for these adapters; ordinary browse bounds
are not advertised as caller-specific AI bounds. The [canonical S6 section](odcs-ai-catalog.md#s6-make-catalog-observations-trustworthy)
and [RV receipt](odcs-ai-catalog-rv.md) distinguish implementation proof from the
blocked live-target handoff. Historical stage decisions follow.

S5 continuation, 2026-10-01: the user explicitly accepted all three Q5 policies
recorded in the [canonical S5 section](odcs-ai-catalog.md#s5-make-contract-semantics-primary-in-authorized-bounded-ai):
pre-helper refusal for denied/unverifiable/oversized mandatory selected context,
labelled omission of optional denied/unavailable observation, and refusal of
revoked history replay requiring a fresh conversation. Saved history remains
unchanged on refusal. Numeric defaults are authorized design choices. Continuous
implementation and RV are authorized; no renewed approval pause. S5's verified
candidate and exact source/log hashes are recorded on the RV page.

R9's [exact acceptance](odcs-ai-catalog.md#human-acceptance-r9-s2-selection-rejection-and-binding-ownership)
settles S2's prior Q1/Q2 blockers: exact compiled-path/SHA-256 manifest selection,
whole configured-bundle rejection on invalid input/pin mismatch, and Aster-owned
versioned physical bindings separate from grants. The S2 gate remains unchanged:
two core tests, six server tests, five named BDD scenarios, CI and asserting
isolated Compose, with assertion RED before production and Build-host-only builds.
No S3 or new policy approval is inferred. Historical r8 setup below is retained.

Completed execution candidate: two core/six intake tests and five bound scenarios,
full CI, S1 gate/regressions and isolated S1/S2 Compose pass on Build-host. Startup,
bindings and qualified identity are wired; exact hashes, RED/GREEN and limitations
are on the RV page. Independent S2 RV is the next gate, not S3 implementation.

The user authorizes staged execution, starting with documentation and separate
independent RV before code, then RV after each step. Seven vertical slices put
immediate metadata/workspace boundaries first: S1 -> S2 compiled-only intake ->
S3 contract-first read APIs -> S7 primary browse/query-preparation UI -> S5
contract-first AI. Stable slice IDs preserve review traceability. S4 previews
branches from S3; S6 observation quality branches from S2. Real-catalog claims
require S6, but authorized contract discovery does not require a live schema.

## What changed

Each slice names an owner, dependencies, observable behavior, exact future test
targets, expected assertion RED, minimal GREEN, rollout and rollback. It gives
an exact `devenv shell -- just ...` invocation and a success-only marker that
cannot print until assertions and regressions pass. S1's two `odcs-*` recipes
now pass on Build-host; later recipes remain planned. Existing focused recipes are identified
separately. Missing tools or targets are infrastructure failures, not RED.

The smallest useful seams are existing server router tests, pure core tests,
fake HTTPS adapter/helper captures and the repository's browser fixtures. The
larger independent Cube execution gate is reserved for proving optional Cube
output, not replaced with string assertions. R18 explicitly defers that scope/gate
beyond initial delivery; it is not a silent skip under a green marker.

The rejected alternative is one horizontal model rewrite followed by late tests.
It would delay the immediate security fix and let self-validation hide invalid
ODCS output. Raw source plus a typed projection keeps the change bounded; a
future schema-validator dependency is justified by an independent conformance
requirement, with offline behavior and an immutable official schema receipt.

The [human r3 correction](odcs-ai-catalog.md#human-correction-r3) rules out author
hierarchy/manifest merging, backend injection, compiler implementation and live
sibling reads. Full compiled documents retain all objects, servers, versions and
semantic metadata, with artifact identity distinct from physical binding.
S2 intake/preservation is internal and independent of Q7/S3 inspection, with
restricted legacy projections or denial rather than new raw disclosure. Q1 bundle
selection/pinning and invalid/unsupported handling remain S2 prerequisites;
R6 settles compatibility as v3.2 only, with an explicit apiVersion support gate
separate from official schema validation and conversion upstream outside Aster.
multi-document/target/version/nested-entry fixtures reject pin mismatches and
overwrite/latest guessing using only supplied compiled bundle/config selection.
R5 accepts whole-contract grants to existing Aster teams, default-deny, and adds
four unautomated team/nonmember/denied-observation examples to S3/S5/S7 gates
(18 draft scenarios at r5). R6 adds schema-valid v3.1 rejection and a v3.2
positive control to S2: 19 draft scenarios at r6. No exact error/status
or startup/quarantine policy is approved. API/UI/AI seams preserve authorized meaning with ZERO
catalog calls for denied observation. Mapping storage/wire and exact denial/revocation
responses remain design details; revocation seams remain in the pending ledger.
R7 settles Q8's outcome: selected authorized contract meaning and semantic context
materially inform existing manual/AI-assisted query drafting and review. S3/S7/S5
add API, browser and captured-helper sentinels, explicit missing/ambiguous binding
variants and ZERO execution calls: 22 draft scenarios map once each. Available
explicit bindings inform drafts; no invented physical identifiers, joins or
aggregates. Exact UX and Q5 budgets remain design, not a renewed outcome choice.
No semantic execution engine, mandatory Cube or guaranteed SQL is implied. S4's
actual endpoint test compares the full ODCS document, including every object,
server, nested field and standard section; selected-object Cube is separate.
Engine grants/backend policy remain execution authority; no semantic query engine
or upstream target-expansion work is required.

## Evidence and limits

| Claim / scenario | Evidence | Result and limitation |
|---|---|---|
| Current commands and runner seams exist | `justfile`, proto and test-source inspection at `eb397d8` on 2026-09-27 | R1 source read; r2 check attempts in RV page |
| Seven planned gates can detect named failures | [IP slices](odcs-ai-catalog.md#ip-vertical-delivery-and-honest-gates) | Design only; fixtures/recipes not implemented |
| Independent ODCS/Cube oracle is required | R9-R12 and S4 | Official source inspected; no validation or Cube execution |
| Independent architect review corrected eight findings | [Appendix B](odcs-ai-catalog.md#appendix-b-rv-of-the-planning-candidate) | Author self-review followed by `ses_f1b194920ffeAVrdTlVZ0G8Rfy`; r2 dispositions in RV page |
| Independent r3 review corrected four findings | `ses_f1ab8829cffefXA3JcQf0narDN`, user-relayed findings | Applied as r4; snapshot hashes/dispositions in RV page, no implementation approval |

R1 ran no tests. [R2 checks](odcs-ai-catalog-rv.md) report actual attempts; no
Compose stack, deployment, real model request or live catalog campaign ran.
S1's executable gate passes, pending independent RV; six later gates remain unmet.

## Decisions and corrections

| ID | Decision requested | Recommendation and consequence | Blocks |
|---|---|---|---|
| Q0 | Authorized in r8: docs first, then implementation with RV after each step | Independent documentation RV before S1; no unanswered policy approval | Review gate before advancing each step |
| Q1/Q2 | Compatibility settled: v3.2 only; remaining selection/pins, invalid/unsupported handling and binding? | Reject v3.1; conversion upstream outside Aster; supplied compiled bundle/config only | S2 prerequisites / S3 |
| Q3/Q4/Q5 | Drift/export, Cube subset and AI failure/budget policy? | Bounded supported behavior; explicit refusals | S3-S5 and UI |
| Q6 | Provider credentials and authorized validation target? | Existing secret mechanism, per-target identity | S6 activation |
| Q7 | Model accepted; mapping/response design remains | Existing teams, whole-contract grants, default-deny; separate observation/execution admission | S3/S7/S5 design; no renewed model approval |
| Q8 | Outcome accepted; exact UX remains design | Context materially informs manual/AI drafting and review; binding gaps stay explicit | S3/S7/S5 preparation design |

The [question ledger](odcs-ai-catalog.md#open-questions-canonical-ledger) is the
only answer history. No unchosen policy is asserted as agreement.

## What continues / what waits

The primary stack is S1 -> S2 -> S3 -> S7 -> S5 with S4/S6 supporting
branches; final integration runs all approved gates. No PR has been published. After
independent documentation RV, write and observe S1's owning RED assertions, implement minimally,
preserve the implementation discovery diff, obtain independent review, rewrite
backward and rerun affected gates. Bind human feedback to the named checkout,
`docs/odcs-ai-catalog-plan`, base `eb397d8`, r8. Independent review session
`ses_f1ab8829cffefXA3JcQf0narDN` covers r3; r4 applies its four findings without
claiming independent sign-off of the revision or implementation approval. R5's
authorization-model acceptance, r6's v3.2-only acceptance and r7's Q8 correction are recorded in the
canonical plan. R8 settles implementation authority, not remaining policies:
Q1 selection/pins and invalid/unsupported handling plus Q2 binding decisions still
block S2, not S1. Each step requires RV and rerun checks before advancing.

### Minimum next S1 verification setup (after documentation RV)

- User correction: "always build on build-host. Cancel builds on my pc". No local
  compilation, devenv acquisition, Cargo tests or container builds. Inspect remote
  state first; use an isolated exact-source snapshot, excluding secrets, `.git`,
  ignored files and build trees, and verify hashes. Documentation RV receipt:
  `ses_f1782bc2bffeAhfnGJ3S7hiCru`, user-relayed no blockers; not S1 sign-off.
- Enter the declared `devenv shell` on Build-host; run `devenv shell -- just ci` to
  establish the baseline. The historical attempt stopped in Clippy because
  sccache startup timed out; workspace tests never ran. A separate Cache-host cache
  fetch timeout preceded shell entry. Neither is assertion RED; neither has been
  rechecked in r8. Diagnose within the declared toolchain, without weakening gates.
- Reuse router/admission fixtures; add the planned `ai_context_boundary` target
  with HTTPS capture trusted through a local test CA, forbidden sentinels and
  working allowed controls. Exercise protected denial, all-unprotected allowance,
  mixed bound/unbound catalogs, and same-ID team/session/personal notebooks.
- Bind only the three S1 scenarios in a separate feature-level
  `@contract @odcs-s1-bound` file; the current 22-scenario draft stays unautomated
  until its scenarios are moved and bound. Assert exact test/scenario names,
  nonzero counts and zero skips; observe actual assertion RED before fixing code.
- Implement the future `odcs-context-boundary-validation` and isolated
  `odcs-compose-smoke s1` recipes. Require backend-identity and notebook-isolation
  regressions, actual health/allowed/denied controls, cleanup and no green marker
  on failure. A working Build-host container engine/Compose is required for completion;
  no running stack or future recipe is claimed in this documentation step.

## Risks and recovery

Rollback must not reopen unsafe enrichment or discard ODCS source. Per-slice
recovery disables affected exposure while retaining strict authorization and
original Git artifacts. Every affected server slice now requires an asserting
Compose smoke and cleanup before its success marker, or remains incomplete.
Checkout isolation is an agent choice; parent platform sync is complete, with
integration/check blockers recorded in the [RV page](odcs-ai-catalog-rv.md).
This planning stage has no runtime changes to recover.
