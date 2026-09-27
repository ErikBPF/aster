# Aster documentation plan

## Request and outcome

> lets double check if we have .feature files for the whole implementation. Lets also create a documentation plan for the whole project. We need sections for catalogs, compute engines, ai, git integration, how to deploy, how to contribute, etc. Lets check our example-platform docs for example

Build one navigable project handbook in this repository. A new reader should be
able to understand Aster's boundaries, run it, choose a catalog and engine, use
notebooks and AI, deploy and recover it, and contribute with the right tests.
Each page must distinguish implemented behavior, tested behavior, and accepted
work that is still planned.

ExamplePlatform's `README.md` and `docs/readme.md` in the sibling repository
informed the layout: one entry point, clear audiences, explicit document
purpose, and links to deeper material. Aster needs operating and
developer guides rather than ExamplePlatform's RFC/ADR/PRD hierarchy. No RFC, ADR,
or spec is a prerequisite for this documentation work.

## Feature-contract audit, 2026-09-23

The active Aster worktree has **38 `.feature` files**: 17 without
`@unautomated`, and 21 explicitly marked `@unautomated`. The accepted
catalog-bound execution, Polaris generic Delta, remote GitHub sync, shared
model, local Git transaction, notebook ownership and team-workspace contracts
are present.
Notebook ownership now has six bound server scenarios; the remaining draft
areas are unbound, with named executable tests covering selected slices.

`just features` checks only that every file has a `Feature` and a `Scenario`.
The server Cucumber runner deliberately skips `@unautomated`; the core runner
binds its core contracts. `features/repo-setup.feature` is checked by
`tests/repo-setup.sh`. `features/live-backends.feature` uses
`tests/live-backends.sh` against a real cluster and is outside `just ci`.
The conversation draft is exercised by separate router, PostgreSQL, and browser
runners plus a manual Build-host check recorded in the platform stage record, but
its Gherkin steps have no runner binding. A file's
presence or CI's `features OK` line therefore cannot be presented as proof that
all its scenarios ran.

| Area | Current contract | Coverage and gap |
|---|---|---|
| Catalog browsing and providers | `catalog.feature`, `polaris-catalog.feature`, `cube-catalog.feature`, `openmetadata-catalog.feature` | Draft Gherkin; adapter and router tests cover selected behavior. Document which provider browses metadata and which authorizes data. |
| Data contracts and semantic output | `data-contracts.feature`, `semantic-models.feature`, `contracts.feature` | Core contracts are bound; the server contract is a draft. Document ODCS input, contract selection, and Cube/ODCS rendering separately from catalog browsing. |
| Compute and data | `engine-pool.feature`, `spark-engine.feature`, `query-authorization.feature`, `live-backends.feature` | Authorization is bound; live Trino/Spark checks are separate from CI. Catalog-bound admission and default-disabled Generic Table metadata have named tests. Disposable Spark 3.5.6 and 4.1.3 fixtures read Generic Delta rows; the 4.1.3 fixture uses Aster's Polaris descriptor and scoped RustFS identities, with a real Bob S3 denial. A separate protected Trino fixture proves Alice/Bob policy through Aster. Protected Spark catalog/engine policy and shared Iceberg parity remain open. |
| Local notebook Git | `notebooks.feature`, `notebook-format.feature`, `notebook-ownership.feature` | Bound local save/read/authorization and legacy ownership/CAS checks. An explicit owner-only Sync passes disposable bare-remote and HTTPS Git fixtures; the production team target remains inactive. |
| Team workspaces and GitHub sync | `notebook-isolation.feature`, `notebook-remote-sync.feature` | Unbound drafts. Disposable fixtures cover designated maintainer target changes, isolated session/personal branches, PostgreSQL target CAS/audit, a team-only default-off startup path, a scoped App/HTTPS adapter, workspace-private chat/helper context with Connect parity, durable local Sync intent, and request-time team UUID membership. A direct-link team browser page has route and mocked-transport Chromium checks. Chart wiring, fresh editor-role policy, qualified Connect notebook RPCs, durable browser status, TUI parity and production workspace activation remain open; live GitHub proof needs a named team repository and installation. |
| Personal AI helpers | `ai-assist.feature` | Unbound Gherkin; unit/router tests cover parts. A manual Build-host DeepSeek E2E is recorded in `platform/docs/plans/aster-boundaries/stages.md`, outside this repository's repeatable CI. Several personal helpers and token isolation are current behavior. |
| Shared AI models | `shared-ai-models.feature` | Unbound draft. The encrypted admin registry, group/role grants, per-user/notebook selection, and first-request revocation pass disposable PostgreSQL and fake HTTPS IdP/model checks. Live Authentik read-after-write revocation and production activation remain open. |
| Private notebook chat | `conversations.feature` | Unbound Gherkin; router, two-database PostgreSQL, and browser runners are in this repository. A manual Build-host check is recorded in the platform stage record; it is not a repo test. |
| Identity, sessions, audit, UI, controller and TUI | Existing owner `.feature` files | Mixed binding. The handbook's test guide must identify the runner for each contract before calling it automated. |
| Deployment and contribution | `repo-setup.feature`, chart/Compose/live recipes | Repository and selected live checks exist. These are not a substitute for an installation, backup, upgrade, and contributor guide. |

No new `.feature` is needed solely because a documentation page is created.
When implementation changes behavior, update its owning feature and add or bind
a failing executable check. Keep unimplemented drafts tagged until their steps
run. Before claiming full feature coverage, produce a small generated inventory
of file, tag, runner, and last passing command; make the runner fail if zero
scenarios were selected. Do not convert a draft by removing its tag alone.

## Handbook structure

Keep `README.md` a short entry point and `CONTRIBUTING.md` the contributor
contract. Put the remaining guides under `docs/`; link each once from
`docs/README.md`. Extend existing `docs/provider-matrix.md` and
`charts/aster/README.md` instead of duplicating their tables or chart values.

| Page | Reader question and minimum content | Source of truth |
|---|---|---|
| `README.md` | What is Aster, what works now, and where do I start? Quick path to mock, Compose, and full local stack. | Verified overview and handbook index |
| `docs/README.md` | Which page serves a user, operator, or contributor? Status key: implemented, tested, planned. | This handbook's navigation |
| `docs/architecture.md` | How do server, controller, TUI, Postgres, Valkey, Git, catalogs, engines, and AI fit? Show request/data flow and durable versus ephemeral state. The local Iceberg stack's RustFS stores table objects; Aster does not own a blob store. | Core ports, providers, proto, stack manifests |
| `docs/catalogs-and-compute.md` | How do catalog browse IDs, engine SQL aliases, table format, connected engines, and per-user data permissions relate? Describe Polaris Iceberg, generic Delta status, Cube/OpenMetadata browse roles, Trino/Gateway, Spark and StarRocks. Include a capability/status table; never imply the planned catalog-bound policy already enforces access. | Catalog/engine adapters, `docs/provider-matrix.md`, bound and draft features, stack manifests |
| `docs/data-contracts-and-semantics.md` | How are ODCS contracts loaded and matched to a query? How can a catalog table render a Cube model or ODCS contract, and where does source contract metadata take precedence? Show an example and distinguish emitted content from a stored or deployed model. | Contract loader, core semantic renderer, `proto/aster.proto`, core and server features |
| `docs/notebooks-and-git.md` | What does save commit locally, how are notebook IDs/branches handled, and how is a revision recovered? Separate current local Git from planned team-configured GitHub sync and its conflict flow. | Git store, notebook format, notebook and remote-sync features |
| `docs/ai.md` | How are personal helpers registered and selected, what does the server send upstream, how does private notebook chat persist, and when does explicit SQL insertion happen? Mark shared admin models as planned. Include metadata versus secondary PostgreSQL choice and no automatic history move. | LLM/conversation stores, API/proto, AI/chat features |
| `docs/deploy.md` | How to run mock, Compose, local minikube stack, and Helm chart; what each proves; how to supply external endpoints and secrets. Link to chart values rather than copying them. Keep Build-host as a named development example, not a required service. | `justfile`, Compose, stack manifests, chart README |
| `docs/operations.md` | How to check health, logs, migrations, persistence, notebook and chat backups, restore, upgrade and rollback; what is lost with `emptyDir` or memory providers. Include credential-safe troubleshooting and actual recovery tests. | Runtime config, deployment manifests, migration and backup behavior |
| `docs/api-and-tests.md` | Where is the API contract, which legacy `/api/*` routes remain, how to run CI, real PostgreSQL, browser, Compose, chart and live backend checks; explain `.feature` binding and draft status. | `proto/aster.proto`, `justfile`, owner tests/features |
| `CONTRIBUTING.md` | Reconcile and extend the existing guide: devenv, crate/port ownership, provider addition, RED/GREEN checks, docs/features, gates, review, and secret hygiene. Remove its stale `ASTER_BUILD_HOST`/`just sync` advice and duplicated feature-placement wording after checking current recipes. | AGENTS.md, existing guide, owner recipes |

The existing `docs/implementation-one-pager.md`, quality audit, compatibility
study, and plans remain dated evidence. The handbook links them as background
only when relevant; it must not present an old status paragraph as current.

## Delivery sequence and acceptance

1. **Inventory and reconcile.** Verify every existing README claim against
   source. The current README still calls Spark a stub although
   `SparkEngine` has a real `QueryEngine` implementation and the live runner
   exercises it. Correct this and other drift while slimming the README.
   Record feature file, tag, bound runner, and independent test coverage.
2. **Give readers a working path.** Write the handbook index, architecture,
   deployment and contribution pages. A newcomer can follow the documented
   mock/Compose commands from a fresh checkout. Keep environment variables in
   one reference location and link to it.
3. **Explain the product boundaries.** Write the catalog/compute/data,
   data-contract/semantic, Git, and AI guides with concrete current examples
   and separate planned sections.
   For Polaris generic Delta, shared Iceberg, GitHub sync and shared models,
   link accepted feature drafts and the platform plan without inventing results.
4. **Make operations and testing honest.** Write recovery and test guides;
   connect each feature to its actual runner. Bind high-value unautomated
   scenarios as implementation slices land, beginning with conversation,
   personal AI, catalog and engine paths. Do not promise CI runs a live cluster.
5. **Review and verify.** Check local Markdown links, run documented owner
   commands in the declared devenv, run `just ci`, and run Compose/chart/live
   checks only where a guide's claims depend on them. Have a fresh reader follow
   the quick start and trace one catalog-to-query and one notebook-to-chat flow.
   Correct the guide or code if the observed behavior differs.

The documentation is complete when the README leads to every requested area,
each guide names its tested status and source, commands match owner recipes,
relative links resolve, and no draft feature is described as shipped. The
feature audit is complete when every delivered behavior has an owning contract
and a named executable check, with unbound Gherkin and live-only checks still
visibly distinguished.

## Delivery status, 2026-09-22

The handbook now has the entry point and eight guides listed above. The README,
contributor guide, provider matrix and chart guide were reconciled with current
source. The bare-server mock quick start, documented Connect request, notebook
archive/restore command and local Markdown links passed disposable checks.
`just ci` passed after the handbook and chart edits. The default chart now
renders development identity on a private cluster; rendering rejects ingress
without OIDC and an issuer. Compose's default published ports now bind to
loopback, checked against rendered configuration. The independent review found
those two deployment defects; both are fixed and their focused checks passed.

Compose startup and final chart validation are recorded in the platform stage
receipt. Subsequent local slices now exercise catalog admission, protected
Trino, Polaris Generic Delta, notebook ownership and disposable Sync, and
shared-model authority; their scope and remaining live gates are listed in
the audit above. Draft Gherkin remains unbound. The handbook does not count
file presence as runtime coverage.

## Delivery status, 2026-09-26

Notebook chat gained a per-cell conversation and the notebook session exchange.
[AI assistance](ai.md), [API and tests](api-and-tests.md),
[provider matrix](provider-matrix.md) and the
[implementation record](implementation-one-pager.md) now describe the cell
panel, the five exchange operations, the trailing statement-terminator trim,
SQL completion and the measured panel gate (`just cell-panel-visual`).
`sql-completion` and `notebook-session-exchange` are bound to runnable
Cucumber scenarios; the stage receipts live in the platform coordination repo
under `docs/plans/aster-boundaries/`.
