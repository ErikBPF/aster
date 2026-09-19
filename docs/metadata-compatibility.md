# Compatibility with OpenMetadata and Cube

Status: findings and phased plan, 2026-09-18. Research grounded on 2026-09-18 against
OpenMetadata `2.0.2-release`, ODCS `v3.2.0`, Cube `v1.7.42`. Phase 1 (read-only) and
phase 2 item 5 (emit) are implemented; the open items are named per phase.

The question: aster already reads catalog metadata through the `Catalog` port and
formats a small ODCS-shaped subset of data contracts into its AI context. How do we
become compatible with OpenMetadata and Cube without inventing a second metadata model?

Answer in one line: OpenMetadata is a **metadata consumer/producer we talk to over its
REST API**, Cube is a **semantic layer we read as another catalog and can emit models
for**. Both fit the existing port model; neither needs a new core abstraction in phase 1.

## Verified findings

### OpenMetadata

- Latest release `2.0.2-release` (2026-09-16); docs track `v2.0.x`.
- REST base `<host>/api/v1`, camelCase entities by id/name/FQN. Auth is a JWT bearer:
  a **bot token** (non-expiring, the automation choice) or a personal access token.
- Read paths that matter to us: `GET /api/v1/tables/name/{fqn}` (columns, description,
  owners), `GET /api/v1/databases`, `GET /api/v1/search/query`,
  `GET /api/v1/lineage/{entity}`, `/api/v1/glossaries` and `/api/v1/glossaryTerms`,
  `GET /api/v1/dataContracts/...`.
- It ships an **MCP server** (semantic search, entity/lineage/glossary tools) with OAuth
  or PAT, and it can itself catalog other MCP servers as entities. That is the cheapest
  route to better AI context without writing an integration.
- Being indexed *by* OpenMetadata from an arbitrary app has two generic paths: the
  **REST/OpenAPI connector** (point it at an OpenAPI schema URL) and the **Custom
  Connector** (a Python `Source` subclass yielding `CreateTableRequest`). Our Trino-backed
  tables can also be ingested natively with OpenMetadata's Trino connector, in which case
  aster only adds what the engine cannot see.
- Unverified: whether 2.0.2 changed MCP tool names, and whether a custom connector can
  attach contract entities.

### ODCS

- Standard is **v3.2.0** (2026-09-08); JSON Schema `odcs-json-schema-v3.2.0.json`,
  draft 2019-09, `additionalProperties: false`, required `version`, `apiVersion`, `kind`, `id`.
- OpenMetadata's ODCS import/export endpoints (`POST /api/v1/dataContracts/odcs[/yaml]`,
  merge or replace, attached by `entityId` + `entityType`) are documented against
  **v3.1.0**, so the standard is currently ahead of OpenMetadata. The Data Contract CLI
  is the practical bridge for validation and conversion.
- Our `crates/core/src/contract.rs` used to parse a **flat JSON subset**: it ignored
  `version`/`apiVersion`/`kind`/`id`, read `owner` (nonstandard; real ODCS uses
  `team: {name, members[]}`), and expected flat `schema[]` entries. Real ODCS `schema`
  entries are objects with nested `properties[]`, and v3.2 adds
  `semanticType: column | measure | dimension`. A real ODCS document therefore yielded
  few or no fields, and YAML was skipped outright. **Fixed on 2026-09-18** (phase 1,
  item 1 below): the parser now reads YAML or JSON through `serde_norway`, accepts
  `name` or `id`, `team.name`, nested `properties[]` and `semanticType`, and keeps the
  flat shape working. Validation against the published JSON Schema is deliberately
  deferred to the emit phase, where conformance actually has to be proven.
- Rust crates: `serde_yaml` is archived; `serde_norway` `0.9.42` is the maintained
  successor; `jsonschema` `0.56.0` can validate against the published v3.2 schema.

### Cube

- Latest `v1.7.42` (2026-09-18). Cube Core is open source and self-hostable; Cube Cloud is
  managed. The SQL API is **off by default in Core**; the MCP server is Premium/Enterprise.
- Trino is a first-class data source: `CUBEJS_DB_TYPE=trino`, `CUBEJS_DB_HOST/USER/PASS`,
  `CUBEJS_DB_PRESTO_CATALOG`, `CUBEJS_DB_SCHEMA`; custom headers only through
  `driver_factory`.
- Read surfaces: `GET /cubejs-api/v1/meta` returns the compiled model (cubes, views,
  measures, dimensions) — shape-for-shape the same browse walk as our `Catalog` trait.
  Query surfaces: REST `POST /v1/load`, GraphQL, the SQL API over Postgres wire
  (`CUBEJS_PG_SQL_PORT`) and its HTTP form `POST /v1/cubesql`.
- AI context: member `description`, `meta.ai_context` on views/members (agent-only),
  and **certified queries** as Markdown under `agents/certified_queries/` (YAML
  frontmatter with `user_request`). The Cube MCP server is paid.
- Auth: API tokens are JWTs signed with `CUBEJS_API_SECRET`; the SQL API has its own
  user/password. `CUBEJS_DEV_MODE=true` is an authentication bypass and must never run
  outside a laptop.

## Phased plan

### Phase 1 — read-only, no new core abstraction

1. **Accept real ODCS (highest value, smallest diff). — done 2026-09-18.** `contract.rs`
   reads the real shape: name or `id`, `team.name`, nested `properties[]`,
   `semanticType`, YAML through `serde_norway` and JSON unchanged; `contracts/orders.yaml`
   is a real v3.2 example, and the loader reads `*.json`/`*.yaml`/`*.yml`. Bound by
   `crates/core/features/data-contracts.feature`.
2. **`kind=cube` catalog provider. — done 2026-09-18.** Cubes and views are the two
   namespaces, measures and dimensions become columns named as Cube's SQL API names
   them, and the configured `catalog` carries the base path (default `cubejs-api`).
   Verified against a stub serving `/cubejs-api/v1/meta`; contract in
   `crates/catalogs/features/cube-catalog.feature`.
3. **`kind=openmetadata` enrichment. — read path done 2026-09-18.** Schemas and tables
   are browsable, with column types (`dataTypeDisplay` preferred) and nullability from
   the column `constraint`. **Decided: OpenMetadata is a source we also read from**, not
   the system of record, so descriptions/owners/glossary/lineage enrichment stays a
   separate server-side step and the core `TableSchema` is unchanged. Endpoints grounded
   in upstream source: `/v1/databaseSchemas?database=`, `/v1/tables?databaseSchema=&fields=columns`.
   Contract in `crates/catalogs/features/openmetadata-catalog.feature`.
4. **Credentials for the read-only providers — open.** Cube API tokens are JWTs signed
   with `CUBEJS_API_SECRET` and OpenMetadata needs a bot token; neither has a home yet
   (`CatalogConfig` carries no secret, and the providers take an optional token only).
   This is one decision for both, and it belongs with the `SecretStore` port rather than
   in `ASTER_CATALOGS`.

### Phase 2 — write/emit behind a new port

5. **Emit Cube models and ODCS contracts. — done 2026-09-18, with two deliberate
   deviations from the sketch.** The port is `SemanticFormat`
   (`crates/core/src/semantic.rs`) with one implementation per target (`CubeFormat`,
   `OdcsFormat`) and `semantic::format` as the selection function; the target is named
   by the request (`RenderSemantic` over RPC), so there is no `ASTER_SEMANTIC_STORE`
   environment variable — a render target is per request, not per deployment. Emission
   reads a `TableSchema` plus the matching `DataContract` when one is loaded: Cube gets
   `sql_table`, dimensions (type inferred from the engine's type name) and measures from
   `semanticType: measure`, using the contract's `transformLogic` as the aggregation and
   assuming `sum` (flagged in the file) when there is none; ODCS gets a v3.2 document
   with `version`/`apiVersion`/`kind`/`id`, `team.name`, `schema[].properties[]`,
   `required` and `semanticType`. The emitted contract is checked by parsing it back
   through `contract.rs` rather than by dragging in `jsonschema`; validating against the
   published schema stays a possible follow-up. Generation stays out of the `Catalog`
   trait. Bound by `crates/core/features/semantic-models.feature`; the response also
   carries the suggested path (`model/cubes/<table>.yml`, `contracts/<table>.yaml`).
6. **Optional `kind=cube` query engine** (`POST /v1/cubesql` first, Postgres wire later),
   giving semantic queries first-class execution.

### Phase 3 — governance

7. OpenMetadata as the contract authority (publish our contracts to
   `/api/v1/dataContracts/odcs`), lineage-aware AI context, and a decision on whether the
   Cube MCP server is worth its plan.

## Decisions this needs from the human

- **Q1 — answered 2026-09-18:** ODCS v3.2 acceptance first (built, see phase 1 item 1).
  The Cube catalog provider and the OpenMetadata enrichment remain open, in that order.
- **Q2 — answered 2026-09-18:** OpenMetadata is a second metadata **source** we read from,
  not the system of record, so no core `TableSchema` change is implied.
- **Q3 — answered 2026-09-18 in practice:** Cube stays read-only semantic browsing for
  now (the provider is built); semantic queries as an engine are phase 2, item 6.
- **Q4** Does any of this belong in the current milestone, or does it wait for the live
  Trino/Polaris runtime (D6) that the catalog providers would otherwise be pointed at?
  The read-only providers are built and stub-verified; what is genuinely blocked on D6 is
  pointing them at real deployments, plus the credential decision (phase 1, item 4).

## Deliberately not proposed

A second metadata schema inside aster. Every item above either reads an existing API into
our types or emits our types into a published format; where that is impossible, that is
the signal a new port is needed, and phase 2 says so explicitly.
