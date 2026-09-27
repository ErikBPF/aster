# Catalogs, compute and data access

**Implemented:** Aster registers catalogs for metadata browsing and engines
for query execution. A catalog page lists namespaces, tables and columns;
`RunQuery` uses the engine selected in the cell or request only when a
configured binding connects it to the selected browse catalog. Engine choices
are filtered by that binding and the subject's app engine grant. Catalog browse
ID, SQL catalog alias, warehouse name and object location are separate
identifiers.

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
Unbound catalogs are hidden from browsing. With no bindings, loaded data
contracts are also hidden from contract pages and AI context because they have
no catalog-scoped access policy.

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
