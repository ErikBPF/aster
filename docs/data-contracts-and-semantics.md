# Data contracts and semantic output

## Materialization-level contracts and MOCK examples (r23)

The [r20 policy](plans/odcs-ai-catalog.md#human-correction-r20-physical-inventory-ownership-and-mock-contracts-2026-10-02)
requires physical catalog presence for accessible data, a per-schema owner group
that alone sees uncovered metadata, and contracts for all data access/sharing,
including owners. It introduces no admin bypass. S8 provides configured-bundle
metadata inventory admission; the current query/share paths do not enforce
contract coverage for arbitrary targets. No-bundle development mode remains legacy.

The [latest correction](plans/odcs-ai-catalog.md#human-correction-r23-materialization-level-contracts)
places contracts at the **materialization level**. In the current mock catalog,
that means one contract per table, not one per containing schema. Whole-contract
grants stay the authorization unit and are therefore table-specific for this bundle.
General ODCS multi-object parsing and full-document preservation remain supported;
future materializations are not hardcoded to tables.

[Five complete table-level MOCK ODCS contracts and schema semantics examples](../contracts/mock-catalog/README.md)
cover the actual five-table mock inventory. Bundle pins, bindings and grants use
existing intake formats. Schema `semantics.yaml` files are descriptive sidecars,
not runtime inputs or executable Cube models; their precise proposed integration
and official Cube evidence are documented with the fixtures. Live dev-login Alice
has no verified team membership; a fixture is not an activated demo configuration.

## Contract-first manual review (S7)

`/catalog` presents the physical inventory and a separately labelled admitted
artifact/query-context section. In compiled mode, `/catalog/physical` redirects
there; the no-bundle development path retains legacy browse. Select a whole document, inspect its full
declared JSON and provenance, then select an exact schema object. The page uses
S3 List/Get/Prepare APIs with fresh team admission. Declared meaning, roles,
transforms and any declared grain/relationships remain distinct from explicit
bindings, observation status and drift facts. Missing bindings are not SQL.

The notebook's **Contract context for manual query review** section follows the
focused/edited cell SQL into an editable review draft. **Review draft with
context** refreshes admission and presents the draft beside selected context;
it neither validates nor executes SQL and does not change or save the cell.
Selection changes and draft edits clear the old review. Revoked access clears
the admitted document and preparation on the next request. Manual review never
calls the helper. S5 attaches the exact selected identity to explicit notebook/cell
helper questions; the server reconstructs authorized bounded meaning. No contract
meaning is persisted in browser storage.

Gate: `devenv shell -- just odcs-catalog-ui-validation` on Build-host. See the
[RV record](plans/odcs-ai-catalog-rv.md) for actual execution status. The S5
assistance candidate, accepted Q5 policy and numeric budgets are described in
[AI assistance](ai.md#authorized-odcs-first-assistance-s5-candidate).
Transformed exports and optional Cube generation are explicitly deferred beyond
the accepted initial scope; original-document preview is the initial export.

## Compiled contract read APIs (S3)

`ListContracts`, `GetContract` and `PrepareContractQuery` are declared in
`proto/aster.proto` and served through Connect/gRPC. Responses carry `contextJson`;
detail/preparation require exact `path` and `sha256`, and preparation also requires
an exact `/schema/N` `object`. Names are never guessed into bindings.
`GetContract` returns the admitted whole parsed document as `declared`, its
`provenance`, and `originalSource`: the exact original UTF-8 ODCS v3.2 source
from the pinned artifact, including comments and formatting. Byte equality applies
to the decoded UTF-8 `originalSource` value; JSON wire escaping is expected.
Intake limits bound admitted inputs, not serialized response size. This is the original
declared document, independent of observed metadata, with no transformation or
new document generation. Whole-document team admission applies to all content;
an `object` pointer does not truncate this preview or grant narrower access.
No catalog or execution grant is required, and no query is executed or artifact
written. Narrow gate: `devenv shell -- just odcs-original-preview-validation`
on Build-host (1 test / 1 bound scenario / 1 step; see the RV receipt for results).
The approved narrow preview is complete after source-only independent RV
`ses_f04fc5bd4ffe252GUe2zkjtRIW`, with no verified findings or found regression;
that review performed no runtime reruns.
The user's explicit “yes” closes initial implementation scope after prior per-step
RV; this original-document preview is the initial export. Remaining S4 transformed
exports, optional Cube/new-document generation and Q3/Q4 policy are explicitly
deferred to future scope, not passed. Q6 live validation awaits authorized targets
and credentials, with no production proof claimed. See
[canonical acceptance](plans/odcs-ai-catalog.md#human-acceptance-r18-initial-scope-closure-2026-10-02).
Preparation retains declared semantics, contract description, source pins,
binding version and independently admitted observation. Comparison reports separate
facts using declared `physicalName`; missing mappings stay unmapped. No SQL, joins
or aggregations are generated or executed by preparation. Export policy remains
S4. S7 uses these APIs; S5 uses a bounded preparation variant with the same
selection, grant and binding rules.

Place separate Aster-owned `grants.json` beside the selected bundle manifest:

```json
{"formatVersion":1,"version":"reviewed-1","grants":[{"path":"compiled/sales.json","sha256":"<exact document SHA-256>","teams":["alpha"]}]}
```

Grants cover whole documents. Each request requires a verified OIDC session,
fresh current-identity membership and configured existing team policy. Production
uses the existing team Git policy/authority configuration; contract reads do not
access Git repositories. Missing grants default deny; unknown teams, malformed
configuration and authority failure fail closed. Replace grants atomically for
revocation: they are re-read even when artifacts are cached. Catalog bindings and
engine grants never grant contract access.

Optional `ASTER_AUTHENTIK_CA_FILE` adds a bounded PEM trust certificate for the
existing HTTPS identity authority without disabling TLS verification. Compose
uses a disposable HTTPS authority and seeded verified session, not a live OIDC
login proof. Validation: `devenv shell -- just odcs-resolution-validation` on Build-host.
See [the S3 receipt](plans/odcs-ai-catalog-rv.md) for scope and evidence.

## Legacy contract projection

**Current implementation:** Aster reads JSON, YAML and YML contracts from
`ASTER_CONTRACTS_DIR` once at server startup.
[`contracts/orders.yaml`](../contracts/orders.yaml) is ODCS-shaped, not a
schema-validated v3.2 conformance example. A missing directory is allowed; malformed files are skipped with
a warning. Restart the server after changing a contract file.

The parser uses the contract name or ID, owner, description, ordered fields,
required flags, semantic types and transform logic. It tolerates other ODCS
fields but does not preserve the full document in its typed projection: objects
are flattened and object/property identities are lost. There is no official
schema validation or separate apiVersion support gate. Loading is not ODCS conformance.
Legacy flattened summaries remain omitted from AI context. S5 uses independently
admitted compiled artifacts instead. A loaded contract is not an access grant or
proof that a table exists. S3 owns the accepted team-grant model.

`RenderSemantic` takes a catalog, namespace, table and target (`cube` or
`odcs`). It reads that table's catalog schema and emits text plus a suggested
repository path. When a loaded contract matches the table name, its richer
field semantics take precedence over catalog columns. The response does not
save, publish or deploy a Cube model or contract.

Bare-name matching can collide, declared fields replace observed columns in the
current projection, and generated ODCS is not independently schema-validated.
Cube translation can guess aggregation, including a double-aggregation risk;
local string/parse-back tests do not establish correct Cube execution.

**Initial compiled scope complete; transformed exports deferred:** The
[current plan](plans/odcs-ai-catalog.md)
consumes full resolved `compiled/` documents from published/copied bundles, with
no author hierarchy or compiler in Aster. First-release intake supports only
`apiVersion: v3.2.0`, independently of schema validity; v3.1 requires upstream
conversion. Whole-contract grants to existing teams are default-deny, separate
from observation and execution permissions. Authorized meaning, field definitions,
declared semantics, grain/relationships and available explicit bindings contextualize
manual/AI query building. Missing bindings stay visible, without invented SQL or
automatic execution. Original-source preview reuses GetContract; supported Cube
translation and new-document generation are explicitly deferred under S4/Q4. Cube is optional.
Selection/pins, whole-bundle rejection and binding
ownership were settled for S2. S5's exact current verification is on the RV page.

```sh
grpcurl -plaintext -import-path proto -proto aster.proto \
  -H 'x-aster-subject: alice' \
  -d '{"catalog":"polaris","namespace":"sales","table":"orders","target":"odcs"}' \
  127.0.0.1:8080 aster.v1.Aster/RenderSemantic
```

The header above works only when the development identity seam is enabled.
Use a real authenticated session in an OIDC deployment. The output for `odcs`
suggests `contracts/<table>.yaml`; `cube` suggests
`model/cubes/<table>.yml`. The [core contract](../crates/core/features/semantic-models.feature)
and [data-contract scenarios](../crates/core/features/data-contracts.feature)
run in the core Cucumber tests. The
[server contract scenarios](../crates/server/features/contracts.feature) are
still tagged `@unautomated`.
# S2 compiled intake (r9)

S2 RV intake limits (conservative first-release bounds): manifest and bindings
each at most 1 MiB, each document at most 4 MiB, at most 128 selected documents,
and at most 32 MiB total source bytes including manifest/documents/bindings.
Limits reject the configured bundle; no truncation or partial admission. Every
input must be an opened regular file; nonblocking open avoids waiting on FIFOs.
The canonical validation gate includes network-isolated schema proof using the
test executable reported by Cargo, not a hardcoded target filename.

Configure `ASTER_CONTRACT_BUNDLE`, `ASTER_CONTRACT_MANIFEST_SHA256` and
`ASTER_CONTRACT_BUNDLE_VERSION` together. The directory contains `manifest.json`:
`manifestVersion: 1`, `bundle`, `version`, and `documents` entries with exact
`path` under `compiled/`, `sha256`, contract `id`, contract `version`, and `target`.
No directory discovery or latest-version selection occurs. Invalid selected
documents or pin mismatches fail startup; originals remain unchanged.

Optional `bindings.json` contains `formatVersion: 1`, reviewed `version`, and
`bindings` entries: artifact `path`, exact object pointer `object: /schema/0`,
`catalog`, `namespaceSegments`, `physicalName`. Its digest is `bindingsSha256` in
the manifest. Missing/unpinned/ambiguous bindings refuse intake. This is physical
configuration, not access grants. Raw contracts are retained internally; compiled
mode exposes no contracts through legacy projections until S3 authorization.

Build-host verification: `devenv shell -- just odcs-document-validation`; details and
independent-review status in [the RV receipt](plans/odcs-ai-catalog-rv.md).
