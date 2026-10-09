# MOCK catalog bundle — r23

**Current demo identity (2026-10-02): Authentik.** The deployed team policy maps
the existing `aster-editors` grant/owner ID to a real Authentik group UUID; it does
not treat the old `/aster-editors` token claim as fresh membership. Five exact
documents, bindings and grants remain byte-for-byte pinned. The historical
Keycloak seed references below explain fixture authorship, not current authority.
See the [successful Authentik first-test receipt](../../docs/plans/odcs-ai-catalog-rv.md).

Five hand-authored, full resolved ODCS v3.2 documents cover **all five** canned
tables, **one contract per materialization**. Here each materialization is a table.
This application fixture convention does not remove general ODCS multi-object
support or prescribe the kind of every future materialization.
These are development fixtures, not production business promises, a
compiler, or evidence of stored rows. `status: draft` is descriptive; current
intake does not enforce a publication lifecycle. Fixture authoring itself did not
change cluster configuration; the separate demo deployment is recorded above.

## Source inventory and live observation

Read-only Build-host inspection on 2026-10-02: context `aster-demo`, namespace
`aster`, server forwarded at `127.0.0.1:18080`. GET `/api/catalogs` reported the
healthy `mock-local` mock adapter. GET
`/api/catalogs/mock-local/namespaces` and each namespace's `/tables` confirmed:

| Catalog | Exact namespace segments | Tables and source columns |
|---|---|---|
| mock-local | `["aster_demo"]` | `orders(order_id bigint, customer varchar, region varchar, total decimal(12,2), placed_at timestamp)`; `customers(customer_id bigint, name varchar, email varchar, created_at timestamp)` |
| mock-local | `["aster_demo.sales"]` | `daily_revenue(day date, region varchar, revenue decimal(18,2))` |
| mock-local | `["aster_raw"]` | `events(event_id varchar, kind varchar, payload json, seen_at timestamp)`; `clickstream(session_id varchar, path varchar, country varchar)` |

Column definitions come from `MockCatalog::tables` in
[`crates/catalogs/src/lib.rs`](../../crates/catalogs/src/lib.rs); the fixture test
compares every column name/type against that adapter. Namespace names and table
presence were observed live; no row queries or production inventory extrapolation.
`aster_demo.sales` is one segment, never `["aster_demo", "sales"]`.

## Bundle and grants

- `manifest.json` selects five exact `compiled/*.yaml` documents, IDs, versions,
  target and SHA-256; `bindings.json` pins all five exact object associations.
- Bundle version: `mock-catalog-r23`; manifest SHA-256:
  `624a6339d761d00826effa497328858a08af6289ac80a5c96e60a3f7fce6ea15`.
- `grants.json` uses existing whole-contract wire format and team `aster-editors`.
  Grants remain separate from bindings and are reread by existing admission.
  All five retain the same `aster-editors` team as before; the permitted table set
  is not broadened. Each grant now selects exactly one table's whole document,
  so operators can grant/revoke sibling tables independently. ODCS owner labels
  and schema semantics sidecars do not grant access.
  An eventual reviewed demo setup must register that team against the **existing
  Keycloak seed** Alice membership `/aster-editors` (exact leading slash).
  Ownership in these examples is a proposed fixture assignment, not an existing
  schema-owner policy or grant conferred by an ODCS `team` field.
- Live deployment has no `ASTER_CONTRACT_BUNDLE`, team-policy or fresh identity
  provider environment wiring. Its browser receipt uses `/dev-login`, which
  supplies subject `alice` and role `editor`, **no groups or verified identity**.
  Thus there is no deployed Alice group to truthfully reuse. Do not manufacture
  one or bypass `current_principal`. The checked-in seed group is the grounded
  intended fixture mapping; verified-session/current-membership wiring is a
  separate implementation prerequisite, not a claim that this demo can read it.

| Materialized table | Compiled document | Stable document ID |
|---|---|---|
| `aster_demo.orders` | `compiled/aster_demo.orders.yaml` | `aster-mock-demo-orders` |
| `aster_demo.customers` | `compiled/aster_demo.customers.yaml` | `aster-mock-demo-customers` |
| `["aster_demo.sales"].daily_revenue` | `compiled/aster_demo.sales.daily_revenue.yaml` | `aster-mock-demo-sales-daily-revenue` |
| `aster_raw.events` | `compiled/aster_raw.events.yaml` | `aster-mock-raw-events` |
| `aster_raw.clickstream` | `compiled/aster_raw.clickstream.yaml` | `aster-mock-raw-clickstream` |

Every document is version `0.1.0`, with its table at `/schema/0`. The r20
schema-level document identities/paths are retired by this explicit fixture
migration; no latest-version or bare-name guessing is introduced. Only the three
task-created r20 documents were replaced. Their original bytes and pins remain in
the earlier Build-host snapshots and the r23 RED archive.

The existing loader can consume this directory using
`ASTER_CONTRACT_BUNDLE`, the manifest hash above, and
`ASTER_CONTRACT_BUNDLE_VERSION=mock-catalog-r23`. This is packaging guidance,
not a request to set deployment environment or activate grants.

## Schema semantics association and precise integration proposal

Each `semantics/<literal-namespace>/semantics.yaml` remains a **schema-level
description artifact**, not Cube configuration or executed code. Format version 2
keeps the catalog/namespace tuple at schema scope and places the exact contract
path/ID/version/SHA-256 and object pointer on each table entry. There is no single
schema-wide contract reference. These associations must agree with the selected
manifest and bindings. `fixture-pins.json` pins their bytes, the example grants and manifest for
the fixture test; it is **not a new runtime manifest format**. Runtime intake
accepts only known manifest fields and selected `compiled/` files. It never scans
or loads these sidecars. Existing preparation/AI already sees the contract's
descriptions, semantic roles and transformLogic; sidecar-only measures such as
`average_amount` do not currently reach those surfaces.

Proposed smallest integration: upstream publication copies approved schema-wide
definitions into each affected compiled object's ODCS `customProperties` under
one documented, versioned `aster.schemaSemantics` key, including source-sidecar
path/digest, exact catalog/namespace identity and per-object definitions. Publish
and repin the full resolved documents; Aster still reads only compiled artifacts.
No author-tree lookup, cross-working-tree dependency or Aster compiler. Before
advertising support, add a bounded typed projection to the current preparation/AI
path, validate exact associations and references, and test whole-contract/team
revocation plus context limits. Raw preservation alone does not mean executed
semantics. Schema-level material must not disclose another contract's content:
include only this materialization's admitted definitions. Cross-table measurement
descriptions do not authorize copying a sibling contract into a table's context.
The duplicate copies must be
validated against their source digest upstream. This is proposed, not implemented.

Owner mapping belongs in separately versioned server-owned policy keyed by
`(catalog, namespaceSegments)`, with fresh group membership, not this editable
sidecar or the ODCS team label. It grants visibility of uncovered metadata only.
Only contracted, physically present, independently authorized data may be accessed
or shared; the owner is not exempt. Semantics never grants access.

## Cube grounding and deliberate limits

Official sources read 2026-10-02:

- [YAML model example](https://cube.dev/blog/introducing-cube-support-for-yaml-data-modeling):
  `count` counts rows; a sum measure uses `sql: amount`, `type: sum`.
- [Measures](https://docs.cube.dev/reference/data-modeling/measures) and
  [types/formats](https://cube.dev/docs/product/data-modeling/reference/types-and-formats):
  aggregation type and input expression are distinct; do not double aggregate.
- [Joins](https://docs.cube.dev/reference/data-modeling/joins): explicit join SQL,
  cardinality and primary keys matter for row multiplication; joins are directional
  LEFT JOINs. This fixture declares **no joins** because `orders.customer` is a
  label, not a declared key to `customers.customer_id`.
- Actual upstream implementation, pinned commit
  `1e869d1d795cf771522b40ce4b21c563e160a057`:
  [CubeValidator.ts](https://github.com/cube-js/cube/blob/1e869d1d795cf771522b40ce4b21c563e160a057/packages/cubejs-schema-compiler/src/compiler/CubeValidator.ts)
  has dedicated schema validation and primary-key handling;
  [BaseQuery.js](https://github.com/cube-js/cube/blob/1e869d1d795cf771522b40ce4b21c563e160a057/packages/cubejs-schema-compiler/src/adapter/BaseQuery.js)
  distinguishes count/countDistinct and multiplied-measure handling. Reading these
  implementations is evidence of modeling constraints, not a Cube execution test.
  No local Cube source checkout was found in the reference cache.

Here only illustrative `count`, `sum`, `avg` and string/time dimensions are
declared. Grain is explicit but unverified; no false primary-key assertions.
`daily_revenue` is already day/region-shaped: only disjoint rows are additive;
counting them does not count orders. No currency, timezone, finer time grain,
session duration, payload schema, rolling windows or executable joins is invented.
ODCS measure `transformLogic: SUM(total)` and Cube-like `type: sum, sql: total`
are different representations, not strings to copy blindly into the legacy exporter.

## Validation

On Build-host, inside the declared devenv: `just odcs-mock-fixture-validation`.
The fixture check reuses the actual bundle loader and pinned official offline
v3.2 validator, checks exact source/tree preservation, five unique stable documents
with one object/binding apiece, full five-table inventory, no retired compiled
files, manifest/document/sidecar/grant digests, grants versus Keycloak Alice seed,
and exact per-table sidecar associations. It runs without network via the existing runner.
The six existing intake tests cover invalid schemas, support/version policy,
digest/selection/binding rejection and fail-closed startup. The gate also exercises
the actual route boundary using a non-owner fixture principal: an orders grant
cannot read customers, and an events grant cannot read clickstream. Switching
each grant reverses visibility without expanding the table set. List/GetContract,
preparation and inventory/legacy table-list responses are checked against the real
mock bundle. S8 binds that regression alongside its existing five cases.
General multi-object ODCS intake and preview regressions remain intact. These
checks do not prove mandatory SQL protection, semantics ingestion, deployed IdP
wiring, stored rows or Cube execution. See the [r23 receipt](../../docs/plans/odcs-ai-catalog-rv.md).
