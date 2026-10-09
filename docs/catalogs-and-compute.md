# Catalogs, compute and data access

**Implemented:** Aster registers catalogs for metadata browsing and engines
for query execution. `/catalog` now has a physical inventory projection and a
separately labelled authorized artifact/query-context section. With a compiled
bundle, `/catalog/physical` redirects there; without one it retains legacy browse.
`RunQuery` uses the engine selected in the cell or request only when a
configured binding connects it to the selected browse catalog. Engine choices
are filtered by that binding and the subject's app engine grant. Catalog browse
ID, SQL catalog alias, warehouse name and object location are separate
identifiers.

### S8 configured physical inventory candidate

`ListCatalogInventory` takes `catalog` and exact `namespaceSegments` and returns
the inventory JSON in `contextJson`. Configure the server-owned regular file
`schema-owners.json` beside the selected bundle manifest, independently of ODCS:

```json
{"formatVersion":1,"version":"owners-1","schemas":[
  {"catalog":"mock-local","namespaceSegments":["aster_demo"],"team":"existing-team"}
]}
```

The team must exist in the existing server team policy; membership comes from
fresh verified identity, never request headers, old session groups or ODCS owner
fields. One freshly resolved principal is shared by ownership and contract-grant
checks within an inventory operation, including multi-schema page/namespace reads;
the next operation refreshes it again. Configuration is reread per request
(1 MiB, at most 128 schema mappings). Missing/malformed/duplicate/unknown mappings
fail closed. An unknown schema and a configured unauthorized schema have the same
status and undisclosed payload; global grant failures also have identical responses
for those probes. Neither denial performs catalog IO. Treat the containing
directory as trusted operator configuration, just like `grants.json`; artifact
authors must not write these authority files. This file is versioned independently,
not part of the publisher-controlled document or a new user-editable authority.

Only owners can discover objects lacking an admitted contract. Other readers
need an exact physical binding and fresh whole-contract team grant. A fresh
complete descriptor listing proves current registration for that metadata request
only: 10-second listing deadline, 4096 descriptors maximum, no stale positive cache.
Polaris and fixture Mock support this proof; Cube/OpenMetadata index declarations
do not, and return unknown rather than accessible objects. No rows are read.

Physical `present`, contract `admitted`/`not_admitted`, and semantics
`declared`/`not_declared`/`unknown` are independent. `not_admitted` intentionally
does not reveal whether another team's hidden contract exists. Semantic detection
covers `semanticType`/`transformLogic` through admitted ODCS nested `properties`,
array `items`, and map `key`/`value` nodes. Annotation-shaped custom data is ignored.
It does not ingest `semantics.yaml` or claim semantic completeness or execution.
Unknown physical evidence yields no inventory entries. Legacy table-list responses
cannot encode that distinction and return an error on unavailable evidence.

Compiled-mode REST/RPC browse and table detail share this boundary. Unselected AI
omits physical metadata, old catalog-only AI history refuses replay, and legacy
completion offers keywords only. Artifact preview/preparation remains separately
admitted and is not an assertion of physical accessibility.

**Not enforced yet:** SQL/share contract coverage and view/join target resolution.
`queryAuthorization: not_evaluated` is deliberate. No-bundle development browse
retains its older policy and is not an r20 enforcement mode. Do not remove the
bundle as a policy rollback. No live activation has occurred; independent S8 RV
and the [execution receipt](plans/odcs-ai-catalog-rv.md) govern this candidate.

`ASTER_CATALOG_BINDINGS` takes comma-separated
`browse_catalog_id;engine_id;native_sql_catalog;unprotected|protected` entries.
The policy field is mandatory. `unprotected` permits only explicit disposable
development bindings. `protected` refuses metadata access; query execution
requires an explicitly configured authenticated engine adapter. A REST query or
`RunQuery` RPC may pass `catalog_context` for the browse ID; the older `catalog`
field remains the native SQL alias and must agree with the binding. Without an
explicit context, Aster infers only a unique configured match. A missing or
ambiguous binding refuses execution. `/api/engines?catalog_context=ID` and the
`ListEngines` RPC return only bound and granted unprotected engines; protected
choices remain hidden until their per-user visibility path exists. Spark receives no
per-request catalog setting; its SQL must use the configured qualified alias.
A verified session can run protected Trino only when that engine has a
token-file/CA-file delegation configuration and Trino authenticates the token,
permits bounded impersonation and enforces table policy. Without that
configuration Trino refuses; protected Spark still refuses.
Unbound catalogs are hidden from guarded browsing. With no bindings, the current
coarse guard also hides legacy loaded contracts from contract pages and unselected AI.
This is not whole-contract team authorization. S1 applies the global metadata
guard and per-catalog admission to conversation grounding, with zero unbound reads;
AI omits unadmitted contract summaries. The
[S1 receipt](plans/odcs-ai-catalog-rv.md#s1-build-host-execution-receipt) records verified
controls without relaxing the conservative protected-metadata policy.

**Compiled contract path:** Physical inventory is the data-discovery surface;
authorized artifacts and semantics provide declared query-building context,
explicit bindings and drift comparisons. Existing teams
receive whole-contract grants, default-deny. Observation access and execution
retain independent checks, so an authorized artifact remains useful with inaccessible
catalogs without becoming accessible physical inventory. Missing/ambiguous bindings
stay visible rather than becoming guessed SQL.
S3 read/preparation and S7 manual review are verified. S5's bounded selected AI
candidate reuses their admission and binding rules; it labels optional observations
unavailable when an adapter cannot guarantee bounded source reads. The
[current plan and receipt](plans/odcs-ai-catalog-rv.md) distinguish verified scope
from optional Cube/export scope and the separately blocked live-provider handoff.

For the local stack, `k8s/stack/aster.yaml` registers Aster catalog ID
`polaris`, Polaris warehouse `quickstart_catalog`, and engines `trino-gw` and
`spark-live`. The Trino connector is also named `polaris`, so SQL such as
`polaris.sales.orders` resolves there. This matching name is a property of
the current stack configuration, not a universal Aster mapping. A binding may
instead connect browse ID `lake-a` to Trino alias `iceberg`. Spark still has
no per-query catalog switch from Aster. The current local stack manifest marks
its real Polaris/Trino/Spark bindings `protected`; it has no token/CA
delegation configuration, so deploying it now would refuse those Aster queries.
The
existing Build-host development image predates this opt-in guard. The separate
`tests/live-backends.sh` runner uses an explicitly unprotected, one-user
disposable sample to keep engine connectivity checks; it is not data-policy
evidence.

| Provider | Current capability | Limit |
|---|---|---|
| Polaris | Browse Iceberg REST namespaces, tables and schemas. A tested, default-disabled adapter can list/load Generic Table metadata with format and optional base location. | Generic browsing is not enabled in runtime wiring; schema, Delta rows and per-user storage policy are unproved. |
| Cube | Browse compiled semantic metadata read-only. | It is a metadata view, not a SQL execution target. |
| OpenMetadata | Browse databases, schemas, tables and columns read-only. | It does not authorize Aster SQL. |
| Nessie, Unity | Registered kinds with unavailable stub adapters. | Do not select them for a working catalog. |
| Trino, Trino Gateway | Execute SQL over HTTP for disposable unprotected bindings. Protected Trino can use HTTPS, a rotating service JWT file, a pinned CA and the verified session subject as `X-Trino-User`; optional routing group sends `X-Trino-Routing-Group`. | Trino must authenticate the delegation identity, constrain impersonation, and enforce catalog/table policy. Aster's current stack has not provisioned that identity or enabled this path. |
| Spark Connect | Execute SQL through a Spark session. | Per-query catalog switching is not implemented. |
| StarRocks | Registered engine stub. | Health is unavailable and execution returns an error. |
| Mock | Canned rows and metadata for local UI work. | Never evidence of a live data backend. |

`ASTER_ENGINES` accepts comma-separated `id;kind;endpoint[;routing_group]`
entries. Only protected Trino may add `;token_file;ca_file` after the routing
group (use an empty fourth field when there is no group). Both paths must be
absolute and the endpoint must use HTTPS. The token file contains a short-lived
service JWT issued and rotated outside Aster; Aster reads it per query and
trusts only the configured CA. The local V3b1b gate proves this path with
synthetic verified sessions, not a production issuer or rotation service.
`ASTER_CATALOGS` accepts
`id;kind;endpoint[;catalog[;token[;credential]]]` entries; the fourth field
has provider-specific meaning. See [`AppConfig`](../crates/core/src/config.rs)
and the [provider matrix](provider-matrix.md) before editing deployment values.
Keep tokens and credentials in the deployment's secret mechanism, not in docs or
Git. `ASTER_DEFAULT_ENGINE` and `ASTER_DEFAULT_CATALOG` choose default IDs.
The bare mock quickstart, Compose, Helm and local stack each declare a
binding; an empty binding set refuses queries. Aster validates malformed,
duplicate or unknown references at startup.

### S6 catalog reliability candidate

Polaris selects `metadata.schemas` by `current-schema-id`, not array order.
Metadata versions 1–3 are supported for this schema projection. The explicit v1
legacy path reads `metadata.schema` only when both modern fields are absent;
missing, duplicate or unknown current IDs fail rather than returning stale fields.
Ordinary namespace/table lists follow Iceberg `next-page-token` using `pageToken`.
OpenMetadata follows `paging.after` using `after`, keeps `limit=200`, and URL-encodes
database/schema filters. Missing OM namespace FQNs refuse rather than falling back
to ambiguous short names. Opaque OpenMetadata names and structured Polaris namespace
arrays remain distinct from their display labels. Generic browsing remains off.

Each browse operation has a **4 MiB aggregate response-body budget, 64 HTTP reads
and a 10-second deadline**, including Polaris OAuth and nested Generic Table loads.
Limits and malformed/repeated/empty cursors fail the entire result. Responses are
streamed within the byte budget, redirects are refused, HTTP failures remain errors,
and diagnostics contain neither response bodies nor request URLs/tokens.

The existing token/credential fields accept `secret://KEY` references. Server
registration resolves only explicit references through the selected
`ASTER_SECRET_STORE` (`env` by default); a missing reference refuses startup and
never falls back to another provider. The controller uses its existing environment
source through `EnvSecrets`. Resolved values enter only their configured adapter;
the stored configuration retains the reference. Inline values remain compatible.
Polaris supports an issued bearer token or OAuth `client_id:client_secret` (a
ready token takes precedence). In the default Polaris environment configuration,
`POLARIS_CLIENT_ID` and `POLARIS_CLIENT_SECRET` must both be absent or both be
nonempty UTF-8 values; a partial pair refuses startup before catalog IO. Explicit
`ASTER_CATALOGS` pool selection retains precedence over these defaults.
Polaris health shares OAuth's remaining deadline and reads status only, without
requiring a JSON health body. Cube and OpenMetadata use already-issued API/bot
tokens, not signing secrets. No new credential store or token issuer is added.

S5's caller-specific `table_schema_bounded` port remains explicitly unavailable for
these three adapters: the browse ceiling is not an implementation of an arbitrary
AI caller's smaller byte/read allowance. Both selected observation and legacy AI
discovery omit them safely, with zero provider IO. The canonical S6 behavior does
not require a new AI provider capability; authorized contract meaning still works.

`just odcs-catalog-quality-validation` owns seven named fixture tests, one bound
scenario/step, provider/routing/Generic regressions, S5's full transitive gate and
isolated S6 Compose credential/startup controls. See the [RV receipt](plans/odcs-ai-catalog-rv.md)
for the actual verification result. **Q6 live activation remains blocked** pending
an explicitly authorized installation and target-specific machine credential;
fixture GREEN does not prove live authorization, deployment or provider readiness.

Protocol grounding: [Iceberg metadata and version rules](https://iceberg.apache.org/spec/),
[Iceberg REST parameters](https://raw.githubusercontent.com/apache/iceberg/main/open-api/rest-catalog-open-api.yaml),
[OpenMetadata table resource](https://raw.githubusercontent.com/open-metadata/OpenMetadata/main/openmetadata-service/src/main/java/org/openmetadata/service/resources/databases/TableResource.java),
[Cube REST authentication](https://docs.cube.dev/reference/core-data-apis/rest-api)
and its [authorization-header parser](https://raw.githubusercontent.com/cube-js/cube/master/packages/cubejs-api-gateway/src/gateway.ts).
These were inspected on 2026-10-02; the fixture contract covers only the fields Aster reads.

**Verified:** `just catalog-routing-validation` runs 11 bound in-process
scenarios for REST/RPC admission, filtering, alias translation and refusal
audit. `tests/live-backends.sh` checks independent Trino and Spark
queries, gateway routing/fallback, a denied app grant and audit records against
the local stack. It does not prove that both engines read the same Iceberg
table, that Polaris generic Delta is queryable, or that two admitted users have
different backend table permissions.

`just polaris-generic-adapter-validation` passes six fake-Polaris adapter tests
and three REST/RPC/browser tests, including valid names requiring URL path
encoding. It checks metadata behavior only; runtime generic browsing remains
disabled until a live Delta read and storage policy are validated.

`just polaris-delta-validation` passes a disposable Polaris 1.7/RustFS/Spark
3.5.6 fixture: an S3 Delta log exists before Generic Table registration,
the Polaris-qualified read fails before registration, then returns two exact
rows and a third row after a Delta append. Generic list/load and data reads
are checked separately. This is a compatible standalone Spark client result;
it does not prove Aster's Spark Connect 4.1.3 path, per-user backend policy or
storage denial. Runtime generic browsing stays disabled for those reasons.

**Planned:** The [catalog-bound contract](../crates/server/features/catalog-bound-execution.feature)
also requires the catalog or engine to enforce each caller's data permissions
with no shared privileged fallback. Application routing alone cannot stop
fully qualified SQL from reaching other catalogs when a backend uses shared
privileged credentials. The [Polaris Delta contract](../crates/catalogs/features/polaris-generic-tables.feature)
separates generic registration/browsing from a real Delta row read. A shared
Iceberg table browsed through Polaris and read by both Trino and Spark follows
that evaluation. Those full contracts remain `@unautomated` drafts; the
configured local manifests and application routing are not completed proof.
