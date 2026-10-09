# Compiled contracts: learnings before implementation

**Stage / revision:** PL/IP discovery input / r8, 2026-09-28.
**Status:** Q0 execution authorized with docs first and RV after each step; independent documentation RV pending, remaining design/policies open, runtime unverified.
**Checkout:** `aster/worktrees/odcs-ai-catalog-plan`, branch
`docs/odcs-ai-catalog-plan`, base `eb397d865cf2dc156fda172d50b13c98eda9d9f7`.
**Feedback binding:** Review this revision alongside the [existing plan](odcs-ai-catalog.md).

## Human seed (publication-safe summary)

Review contract hierarchy, multi-target deployment, Terraform generation and
semantic-layer examples; record reusable lessons before implementing.

The original seed and source-specific discovery notes are preserved privately.
This public revision retains generic lessons and accepted Aster decisions only.

## Accepted Aster boundary and primary UX

Exact human correction:

> we should base integration on full contracts in compiled folder. Tool does not need to know about uncompiled structure. contracts and semantics should be the preferred way to interact with data. Not only catalog

Aster consumes full resolved contracts from `compiled/` in a published/copied
artifact bundle, not a live sibling tree. Author hierarchy, composition,
target-specific expansion and compilation stay upstream. Earlier
assistant proposals to support these in Aster are superseded; upstream target
expansion fixes are not Aster requirements. Preserve all schema objects, servers,
versions, semantic metadata and raw content. Artifact identity and provenance
remain distinct from provider-qualified physical bindings; no filename suffix
alone establishes a full document or a supported version.

Contracts/semantics lead discovery, browse, query preparation and AI. Physical
catalogs support observations, bindings, drift and fallback/expert use. Authorized
contract scope allows discovery without a live schema; observation access is
separately authorized. [R5 acceptance](odcs-ai-catalog.md#human-acceptance-r5)
settles Q7 as whole-contract grants to existing Aster teams, default-deny;
mapping storage/wire and exact denial/revocation responses remain implementation
design/open as needed. Neither validity nor an unbound artifact grants disclosure. Execution retains engine
grants/backend policy; this is not a promise of a semantic query engine.

## Reusable architectural lessons

| Area | Lesson and Aster boundary |
|---|---|
| Hierarchy and composition | Keep authoring inheritance and composition in the upstream compiler. Aster consumes complete resolved documents, not authoring rules. |
| Assignment and output | Artifact identity, version, target assignment and physical binding are separate concepts. A target-specific output alone does not prove isolation or deployment. |
| Infrastructure | Contract compilation, infrastructure generation and infrastructure apply are separate stages. Aster consumes artifacts; it does not own upstream Terraform generation or apply. |
| Promotion | Pin published bundles and preserve provenance. Distinguish rendered, validated, planned, applied and observed states; generated output is not runtime evidence. |
| Semantics/version | Preserve declared grain, relationships and expressions without inferring executable joins, aggregates or cardinality. Aster's [example][orders]/[fixture][transform] contain measures/dimensions and `SUM(total)` but do not prove validity or semantic deployment. |

## Evidence and next gate

Discovery established architectural lessons, not runtime success.
[R6 acceptance](odcs-ai-catalog.md#human-acceptance-r6) settles first-release
intake as v3.2 only: reject v3.1 using an explicit apiVersion support gate separate
from official schema validation, which accepts older versions. Conversion stays
upstream outside Aster; unsupported output cannot be consumed directly or
relabelled as verified v3.2. Q1 selection/pins, invalid/unsupported handling,
startup/quarantine and exact errors/statuses remain open. The [canonical ledger](odcs-ai-catalog.md#open-questions-canonical-ledger)
also retains binding, drift, AI budgets and Q7 design details. Prior r2
RV does not sign off the r3 boundary/UX revision. Independent r3 review
`ses_f1ab8829cffefXA3JcQf0narDN` is applied as r4 in the
[RV page](odcs-ai-catalog-rv.md): Q1 selection/pins/version support precede internal
intake; Q7 inspection/denial/revocation stays separate; full-document ODCS previews
retain the compiled source. [R7's Q8 correction](odcs-ai-catalog.md#human-correction-r7-accepted-q8-outcome)
settles the outcome: selected authorized meaning, field definitions, declared
semantic roles/expressions, grain/relationships, provenance and available explicit
bindings materially inform existing manual/AI query building and reviewable drafts.
Missing/ambiguous bindings stay visible; no inferred executable joins, aggregates
or physical identifiers. Exact UX/budgets remain open, with no semantic execution
engine or guaranteed SQL promised. R8 authorizes implementation after a separate
independent documentation RV, with RV after every step; it does not turn these
target behaviors into shipped capabilities. Q1 remaining intake policy and Q2
binding decisions block S2, not S1's existing-policy safety work.

[orders]: https://github.com/ErikBPF/aster/blob/6a98b0c4292f9656b2463ab30b5ef0001b0f2ae6/contracts/orders.yaml
[transform]: https://github.com/ErikBPF/aster/blob/6a98b0c4292f9656b2463ab30b5ef0001b0f2ae6/crates/core/src/semantic.rs#L344
