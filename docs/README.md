# Aster handbook

Start with the [repository README](../README.md) to run Aster. These guides
explain the current product and its development workflow.

| I want to… | Read |
|---|---|
| Understand components and storage | [Architecture](architecture.md) |
| Browse data and choose compute | [Catalogs, compute and data access](catalogs-and-compute.md) |
| Load a data contract or render a model | [Data contracts and semantic output](data-contracts-and-semantics.md) |
| Save and recover notebooks | [Notebooks and Git](notebooks-and-git.md) |
| Register helpers and use notebook chat | [AI assistance](ai.md) |
| Run Compose, minikube or Helm | [Deployment](deploy.md) |
| Try the isolated benchmark notebook demo | [Real notebooks, TPC-H/TPC-DS catalogs, access and receipts](plans/odcs-ai-catalog-rv.md#r24-delivered-real-benchmark-notebooks-2026-10-09) |
| Back up, upgrade or troubleshoot | [Operations](operations.md) |
| Call the API or understand test coverage | [API and tests](api-and-tests.md) |
| Change Aster | [Contributing](../CONTRIBUTING.md) |

**Status language:** *implemented* means source provides the behavior;
*verified* names a passing check and its environment; *planned* names an
accepted contract that is not yet delivered. A `.feature` file marked
`@unautomated` describes behavior but does not run as a Cucumber test.
[The feature audit and documentation plan](documentation-plan.md) record the
coverage and delivery decisions.

## Planning candidates

**Current delivery, r24 / 2026-10-09:** the isolated Build-host demo runs Helm
revision 8 with owner-checked local notebooks, separate native TPC-H/TPC-DS
catalogs, all 33 tables and per-table ODCS contracts. Full CI, Compose, chart,
live metadata and actual Authentik notebook browser checks passed, including
save/reload/query and persistence after server restart. Structured contract
details retain source/provenance. Work remains uncommitted; team/GitHub activation,
real AI and SQL-target contract enforcement are not part of this demo. See
[r24 evidence and access](plans/odcs-ai-catalog-rv.md#r24-delivered-real-benchmark-notebooks-2026-10-09).

**Delivery snapshot, 2026-10-04:** canonical `main` is locally at `eb397d8`
(September 27; no remote fetch). The accepted ODCS slices and October 2 OIDC
correction are uncommitted in this task worktree, not mainline delivery. Two
independent source reviews found no blocking defect in the callback correction;
fresh Build-host focused checks and CI passed. Evidence and the separate session-cookie risk are tracked in the
[continuation review](plans/odcs-ai-catalog-rv.md#roadmap-continuation-review-2026-10-04).
Priorities: consolidate reviewed work; address session-cookie hardening; implement
authoritative query/share admission; settle no-bundle migration; finish scoped
live proofs. Team GitHub sync, protected compute and shared-model activation still
carry the obligations recorded in their owning guides. Initial ODCS scope closure
does not close those obligations.

**Prior r23 / S8:** [materialization-level contracts](plans/odcs-ai-catalog.md#human-correction-r23-materialization-level-contracts)
means one complete contract for each of the five current mock tables, with
independent whole-document grants and schema-level semantics references. The three
S8 P2 fixes and the per-table continuation are complete after source-only RV
`ses_f01458357ffeG13RoBziy3aSeB`: no verified findings or new runs; prior Build-host evidence stands.
The owning S8 gate now has six tests/scenarios/steps. The catalog page projects physical,
admitted-contract and semantic facets; compiled-mode browse uses owner/contract
admission. SQL/share target enforcement, no-bundle mode migration and semantics
sidecar ingestion remain outstanding; no deployed policy claim.

**Reviewed r20 basis:** [physical inventory and schema-owner policy](plans/odcs-ai-catalog.md#human-correction-r20-physical-inventory-ownership-and-mock-contracts-2026-10-02)
supersedes earlier contract-only access/admin exceptions. Only owners see uncovered
metadata; everyone needs contracts for data access/share. The
[MOCK bundle](../contracts/mock-catalog/README.md) covers all five current mock
tables with five pinned table-level ODCS contracts and three descriptive schema
semantics sidecars after the r23 correction.
The original r20 docs/three-document fixture passed source-only independent RV
with no verified findings. That review does not approve the r23 five-document
bundle or revised runtime candidate. The scope history follows.

[ODCS-first AI, semantics and catalog plan](plans/odcs-ai-catalog.md) records the
selected direction, source findings and proposed vertical delivery slices.
The accepted direction is whole-contract existing-team grants, default-deny,
with independent observation and execution checks. Intake accepts only validated
compiled v3.2 artifacts selected by exact manifest/path/hash; conversion stays
upstream. S1/S2/S3/S7 have verified Build-host receipts and review corrections. S5
implements explicit selected semantic assistance and dependency-aware history;
its final gate status is recorded on the RV page. Q5's refusal, optional omission
and historical-dependency policies were explicitly accepted on 2026-10-01.
No identifiers, joins or aggregates are guessed into bindings; no automatic
execution is added. S6's catalog-reliability implementation passes its full Build-host
gate; Q6 live activation remains blocked. S4's narrow original-source preview
reuses GetContract and is approved complete after source-only independent RV
`ses_f04fc5bd4ffe252GUe2zkjtRIW` (no verified findings; no runtime reruns).
Prior Build-host evidence remains 1 test / 1 scenario / 1 step.
The user's explicit “yes” closes initial implementation scope after prior per-step
RV: original-document preview is the initial export. Remaining S4 transformed
exports, optional Cube/new-document generation and Q3/Q4 policy are explicitly
deferred to future scope, not passed. Q6 live validation awaits authorized targets
and credentials; fixture proof is not production proof. See
[canonical acceptance](plans/odcs-ai-catalog.md#human-acceptance-r18-initial-scope-closure-2026-10-02).
S5 safely omits these adapters' unavailable bounded AI reads. Review the
[PL one-pager](plans/odcs-ai-catalog-pl.md),
[IP one-pager](plans/odcs-ai-catalog-ip.md) and
[RV one-pager](plans/odcs-ai-catalog-rv.md), plus the
[draft server feature](../crates/server/features/odcs-ai-catalog.feature).

[Compiled-contract learnings](plans/example-platform-contract-learnings.md) records
generic architectural lessons and the accepted r3 compiled-artifact boundary and
contract/semantic-first UX plus r5 team authorization, r6 v3.2-only intake and r7
query-building context; r8 authorizes staged execution with per-step RV while
remaining design/policy stays open.

[Provider matrix](provider-matrix.md) lists ports and selection sites.
[Chart README](../charts/aster/README.md) owns Helm values. The dated
[implementation record](implementation-one-pager.md),
[quality audit](quality-audit.md), and
[metadata compatibility study](metadata-compatibility.md) are background;
check current source before using their status claims.
## Verified slice receipts

Independent S2 RV `ses_f16d503e4ffe5PuK5zPFF0AHpj`: all five P2 corrected and full
Build-host S2 gate rerun GREEN, including mandatory offline proof. See the RV receipt
for dispositions/hashes. No independent second approval is inferred.

Compiled ODCS intake, versioned physical bindings and structured catalog identity
have passed the Build-host S2 gate, full CI, S1 regressions and isolated Compose.
[Exact receipts and independent RV handoff](plans/odcs-ai-catalog-rv.md).
S3 contract read/preparation APIs now pass the Build-host full gate (seven Rust tests,
five bound scenarios, S1/S2 regressions and isolated Compose). Independent S3 RV
completed with its comparison-coverage P2 fixed and mutation-checked. S7 manual
query context and its stale completion-draft correction are verified. S5 assistance
passes its full isolated Build-host gate, including both independent RV P2 corrections
(12 tests / 3 bound scenarios); no new sign-off is claimed. S6 adds verified
current-schema, bounded pagination/transport and target-secret wiring: seven tests,
one bound scenario/step, full CI, prior-slice regressions and isolated S6 Compose.
Both P2s from independent S6 review `ses_f05412269ffeyYuCSDjOoOprQU` are fixed and
verified (partial OAuth environment refusal and shared OAuth/health deadline).
No new reviewer sign-off is claimed; live activation remains blocked and optional
Cube export is explicitly deferred beyond the accepted initial scope. Configuration and wire details:
[compiled contract reads](data-contracts-and-semantics.md#compiled-contract-read-apis-s3).
