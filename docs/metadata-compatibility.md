# Compatibility with OpenMetadata and Cube

Status: findings and phased plan, 2026-09-18. Research grounded on 2026-09-18 against
OpenMetadata `2.0.2-release`, ODCS `v3.2.0`, Cube `v1.7.42`. Nothing here is implemented;
each phase names the seam it touches and the decision it needs.

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
- Our `crates/core/src/contract.rs` parses a **flat JSON subset**: it ignores
  `version`/`apiVersion`/`kind`/`id`, reads `owner` (nonstandard; real ODCS uses
  `team: {name, members[]}`), and expects flat `schema[]` entries. Real ODCS `schema`
  entries are objects with nested `properties[]`, and v3.2 adds
  `semanticType: column | measure | dimension`. A real ODCS document therefore yields
  few or no fields today, and YAML is skipped outright.
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

1. **Accept real ODCS (highest value, smallest diff).** Teach `contract.rs` the real
   shape: `version`/`apiVersion`/`kind`/`id`, `team.name`, nested `properties[]`,
   `semanticType`, and YAML through `serde_norway`; keep JSON working. Touches
   `crates/core/src/contract.rs` only, and gives OpenMetadata round-tripping a chance.
2. **`kind=cube` catalog provider.** `GET /v1/meta` → namespaces (cube groups/views),
   tables (cubes/views), schema (measures/dimensions as `ColumnSchema`). Touches
   `crates/catalogs/src/lib.rs` plus a provider-matrix row; no core change.
3. **`kind=openmetadata` enrichment.** Read table/column descriptions, owners and glossary
   terms for the catalog tab and the AI context. Needs a decision: does `TableSchema`
   gain `description`/`owner` fields (a core struct change) or does OpenMetadata stay a
   separate read path in the server.

### Phase 2 — write/emit behind a new port

4. **`SemanticModelStore` port** in core, selected by `ASTER_SEMANTIC_STORE`: emit Cube
   YAML models (dimensions from `TableSchema`, measures from ODCS `semanticType: measure`)
   and ODCS v3.2 documents validated with `jsonschema`. An `aster-ctl` command is the
   smaller alternative if only file emission is wanted. Generation must not go into the
   `Catalog` trait — that trait is deliberately navigation-only.
5. **Optional `kind=cube` query engine** (`POST /v1/cubesql` first, Postgres wire later),
   giving semantic queries first-class execution.

### Phase 3 — governance

6. OpenMetadata as the contract authority (publish our contracts to
   `/api/v1/dataContracts/odcs`), lineage-aware AI context, and a decision on whether the
   Cube MCP server is worth its plan.

## Decisions this needs from the human

- **Q1** Which phase-1 item first: ODCS v3.2 acceptance, the Cube catalog provider, or the
  OpenMetadata enrichment? (Recommendation: ODCS first — it is the smallest change and it
  unblocks the other two.)
- **Q2** Is OpenMetadata a second metadata *source* for the catalog tab, or the system of
  record we publish to? That decides whether phase 1 grows a core `TableSchema` change.
- **Q3** Cube: read-only semantic browsing now, or semantic queries as an engine too?
- **Q4** Does any of this belong in the current milestone, or does it wait for the live
  Trino/Polaris runtime (D6) that the catalog providers would otherwise be pointed at?

## Deliberately not proposed

A second metadata schema inside aster. Every item above either reads an existing API into
our types or emits our types into a published format; where that is impossible, that is
the signal a new port is needed, and phase 2 says so explicitly.
