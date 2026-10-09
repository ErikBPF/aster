# ODCS-first AI and catalog: PL

**Authoritative correction r23:** [contracts belong to materializations](odcs-ai-catalog.md#human-correction-r23-materialization-level-contracts),
so the current five materialized mock tables have five complete compiled contracts,
not three schema-level documents. Schema semantics remains schema-level and points
to each exact table contract. Whole-contract grants become independently table-specific.
General multi-object ODCS support and future non-table materializations remain valid.
The r22 S8 reviewer fixes and r23 per-table verification pass their full Build-host
gates and are complete after source-only RV `ses_f01458357ffeG13RoBziy3aSeB`
(no verified findings/new runs), without deployment or next-slice work.

**Runtime continuation r21:** the user authorized the first S8 runtime slice after
the r20 review below. Its configured-bundle scope, existence oracle and remaining
SQL/share/sidecar work are in [the current IP](odcs-ai-catalog-ip.md). This retains
the reviewed r20 policy; it is not an independent approval of runtime code.

**Current stage / revision:** PL / r20, docs and MOCK fixtures independently reviewed; no verified findings.
The [exact latest seed](odcs-ai-catalog.md#human-correction-r20-physical-inventory-ownership-and-mock-contracts-2026-10-02)
settles physical presence as necessary, uncovered metadata as schema-owner-group-only,
and contract-required data access/sharing including owners. No admin bypass.
The [fixture record](../../contracts/mock-catalog/README.md) grounds all five mock
tables, Alice's seed group, the three ODCS contracts, semantics association and
official Cube examples/implementation. Sidecars are not runtime-loaded.

Independent source-only RV `ses_f030e0250ffeIPVS1cgeiNPZIZ` is recorded in RV;
the reviewer performed no runs. Next gate: review the runtime slices in IP. Open engineering decisions are authoritative
query-target/existence proof and owner-policy/identity wiring, not whether owners
need contracts. No cluster update is part of fixture creation. Current demo UI
and dev-login lack the new policy and compiled admission wiring. Build-host-only
validation and remaining blockers are in RV; no runtime or deployment approval claimed.

## Historical r19 discovery (superseded by r20)

**Historical stage / revision:** PL / r19, discovery reopened, pending product scope.
**Destination:** one catalog inventory with independent physical presence,
contract coverage and semantic coverage facets. Contract/semantic detail belongs
with entries; semantic presence is neither completeness nor permission.
The [verbatim correction and source-grounded proposal](odcs-ai-catalog.md#human-correction-r19-unified-catalog-and-access-scope-2026-10-02)
supersede the former separate-view/fallback target. Current UI duplicates the
contract picker across two routes and keeps physical browsing separate; current
query admission has no contract coverage requirement.

Ordinary access should use contract association plus explicit whole-document
existing-team grants, with engine/backend authority still required for queries.
Admin/private-product raw exceptions need a defined boundary. **Q9 first:** does
"accessible" govern discovery, query execution, or both? Q10 then defines private
product ownership/team eligibility; Q11 settles actual query-target coverage.
No implementation until these load-bearing decisions support a revised IP.

Feedback: canonical r19, `docs/odcs-ai-catalog-plan` at base `eb397d8` in the
existing Aster task checkout; platform sync remains `aster-odcs-plan` at `d792329`.
Source-only discovery, no fresh runtime or independent review evidence; all future
builds remain Build-host-only. The existing draft feature records only agreed facet
examples, not pending ACL choices. Prior r18 completion and the stage history
below remain valid for the old scope, not proof of the new destination.

## Historical PL record through initial-scope closure

**S5 continuation, 2026-10-01:** Q5 is explicitly accepted: refuse selected
denied/revoked/unverifiable/oversized mandatory context before helper calls,
preserve history on failure; label optional observation omissions while keeping
authorized meaning and making zero denied-catalog calls; refuse replay of a
no-longer-authorized contract-dependent exchange and require a fresh conversation.
Numeric defaults and continuous implementation/RV are authorized. See the
[canonical S5 acceptance](odcs-ai-catalog.md#s5-make-contract-semantics-primary-in-authorized-bounded-ai).

**Stage / revision:** PL / r14
**Status:** Initial implementation scope complete after prior per-step RV and the user's explicit “yes” to closure. Original-document preview is the initial export; Q3/Q4 transformed exports, optional Cube and new-document generation are explicitly deferred to future scope. Q6 awaits authorized targets/credentials for live validation, not production proof. See [canonical r18 acceptance](odcs-ai-catalog.md#human-acceptance-r18-initial-scope-closure-2026-10-02). Prior Build-host/review receipts remain unchanged; platform checks remain blocked. Earlier stage statements below are historical, superseded by this closure where applicable.
**Owner / date:** Aster maintainers / 2026-10-02 UTC
**Basis:** verbatim seed and selected ODCS-first direction in the
[canonical plan](odcs-ai-catalog.md#human-seed-and-destination).

## Outcome and why it matters

R9 supersedes the historical Q1/Q2 blockers below: the user's exact acceptance is
in [the canonical plan](odcs-ai-catalog.md#human-acceptance-r9-s2-selection-rejection-and-binding-ownership).
A manifest selects compiled paths/SHA-256 without latest guessing; invalid
contracts or pin mismatches reject the configured bundle; physical bindings are
Aster-owned versioned configuration separate from access grants. Intake remains
internal until S3 authorization. No S2 runtime proof or independent RV yet.

Execution is authorized with documentation first and RV after each step; remaining
product policies are not thereby approved. The following is target behavior,
not current implementation.
Full resolved contracts from `compiled/` in published/copied bundles supply
primary discovery, browse, query preparation and AI meaning. Physical catalogs
support observed schemas, bindings, drift and fallback/expert use; Cube remains
optional derived output. R6 accepts v3.2-only first-release intake: explicit
apiVersion support is separate from schema validity; reference compiler v3.1
output requires upstream conversion outside Aster before intake.

## What changed

The plan traces the existing parser, semantic emitter, catalog adapters, AI
entrypoints, workspace admission, proto and test seams. Its two-round party
exchange retains disagreements about compatibility, binding, drift, optional
Cube and unavailable AI context. It proposes a draft
[owning server feature](../../crates/server/features/odcs-ai-catalog.feature),
explicitly unautomated; Q0 authorization is not test proof.

R3 records the [exact human correction](odcs-ai-catalog.md#human-correction-r3).
Aster needs no author hierarchy, definitions, manifest merging, backend base
injection or compiler, and never reads a live sibling tree. Upstream expansion
changes are not Aster work. Full documents retain multiple schema objects,
servers, versions and semantics; artifact identity is separate from binding.

R4 applies four independent r3 review findings: preservation is internal intake,
not permission to expose raw contracts; Q1 selection/pins and schema-version
support precede S2; Q7 denial/revocation examples remain in the pending ledger;
S4 previews the entire ODCS document, separately from selected-object Cube output.
At r4, the minimum useful query-preparation outcome remained the user question Q8.

R5 records [human acceptance](odcs-ai-catalog.md#human-acceptance-r5) of
whole-contract grants to existing Aster teams, default-deny. Observation and
execution checks stay separate. Four concrete unautomated examples cover team
access, nonmember denial, browsing and helper context with denied observations
and ZERO adapter calls. This accepts the model, not implementation or other policy.

R6 records [human acceptance](odcs-ai-catalog.md#human-acceptance-r6) of "v3.2 only".
One additional draft S2 scenario rejects schema-valid v3.1 without conversion,
with a supported v3.2 control. Selection/pins, invalid/unsupported handling,
startup versus quarantine and exact errors/statuses remain Q1, not approved.

R7 records the [authoritative Q8 correction](odcs-ai-catalog.md#human-correction-r7-accepted-q8-outcome):
contract/semantic context must materially inform existing manual and AI-assisted
query drafting/review, beyond browse or selection. Selected authorized meaning,
field definitions, declared roles/expressions, grain/relationships, provenance
and available explicit bindings reach query building; unresolved bindings remain
visible without invented identifiers, joins or aggregates. Three draft API/UI/AI
scenarios assert concrete sentinels and ZERO execution calls. Exact UX and budgets
remain open; no semantic engine, mandatory Cube or guaranteed SQL is implied.

Close the conversation metadata
and workspace boundaries before expanding enrichment. A complete typed ODCS
implementation and a second semantic authority are rejected in favor of raw
source preservation plus the projection Aster needs. The trade-off is explicit
unsupported semantics rather than pretending every schema-valid field executes.

[Discovery receipts](odcs-ai-catalog.md#appendix-a-discovery-receipts) preserve
the source base and corrections. The coherent candidate works backward from
authorized object resolution and preserves the raw human seed; it does not
replace open questions with scenarios. Contract scope permits discovery without
a live schema through Q7's accepted team model; mapping storage/wire and exact
denial/revocation responses remain implementation design/open as needed.
Validity is not authority, observation access is separate, and execution retains
engine grants/backend policy. No semantic query engine is promised. Slice dependencies are in the
same plan, not published PRs.

## Evidence and limits

| Claim / scenario | Evidence | Result and limitation |
|---|---|---|
| Conversation boundary gaps | Plan R2/R3, source base `eb397d8`, read 2026-09-27 | Source verified; no deployed disclosure reproduction |
| ODCS projection loses identity | Plan R4/R5 and official schema/fundamentals R9/R10 | Source verified; no migration or schema validation executed |
| Official schema is reproducibly identifiable | R11 commit and decoded-byte SHA-256 | Bytes inspected; vendoring/offline validation is future work |
| Proposed examples are executable evidence | Draft feature and current runner filter R1 | Not run; unbound and excluded by `@unautomated` |

The first draft received author self-review, followed by independent architect
review `ses_f1b194920ffeAVrdTlVZ0G8Rfy`. Its eight findings are applied in r2;
the [RV page](odcs-ai-catalog-rv.md) records dispositions and current checks.
Independent r3 review `ses_f1ab8829cffefXA3JcQf0narDN` supplied four further
findings, applied in r4 with snapshot receipts on the RV page. Neither review
grants implementation approval. No behavior has runtime proof.

## Decisions and corrections

| ID | Decision requested | Recommendation and consequence | Blocks |
|---|---|---|---|
| Q0 | Authorized in r8: docs first, then implementation with RV after each step | Independent documentation RV before S1 | Review gate before advancing each step |
| Q1/Q2 | Compatibility settled: v3.2 only; remaining selection/pins, invalid/unsupported handling and binding? | Upstream conversion outside Aster; explicit compiled selection and reviewed binding remain proposed | S2 prerequisites and resolver delivery |
| Q3/Q5 | Drift and unavailable-context policy? | Distinguish denial, unavailability and ambiguity | Export/AI UX |
| Q4/Q6 | Cube scope and provider targets? | One proven aggregate; existing secret provider | Optional export and live activation |
| Q7 | Model accepted; mapping/response design remains | Existing teams, whole-contract grants, default-deny; independent observation/execution checks | S3/S7/S5 implementation design, not a new model approval |
| Q8 | Outcome accepted; exact UX remains design | Materially contextualize manual/AI query building and reviewable drafting; expose binding gaps, never invent SQL details | Preparation API/UI/AI design; budgets remain Q5 |

Answers belong in the [canonical ledger](odcs-ai-catalog.md#open-questions-canonical-ledger),
not in a second answer history. Silence is not acceptance.

## What continues / what waits

R8 records "start by docs and them do the rest. Lets do a /rv after each step" in
the canonical ledger. Separate independent documentation RV precedes S1 code;
RV follows every implementation step before advancing. S1 needs runnable checks,
not later product decisions. S2 waits for Q1 selection/pins and invalid/unsupported
handling and Q2 binding decisions; other policies block their own slices.
Human feedback must identify checkout
`aster/worktrees/odcs-ai-catalog-plan`, branch `docs/odcs-ai-catalog-plan`, base
`eb397d8`, artifact r8. Session `ses_f1ab8829cffefXA3JcQf0narDN` reviewed r3;
four corrections remain applied. R5/r6/r7 record user acceptances/correction only; no new independent
sign-off. R8 authorizes execution but is not independent RV or runtime proof.

## Risks and recovery

Source evidence is not runtime assurance. Invalid checked-in examples, bare-name
collisions and provider truncation remain until implemented and tested. This
document-only stage changes no runtime state. Checkout isolation was an agent
choice, not a user product answer; parent platform sync is complete, with
integration/check blockers recorded in the [RV page](odcs-ai-catalog-rv.md).
Revise the candidate in place on
feedback while retaining discovery receipts. Runtime rollback and evidence
requirements are per-slice in the plan.
