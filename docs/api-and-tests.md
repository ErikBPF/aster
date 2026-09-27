# API and test contracts

[`proto/aster.proto`](../proto/aster.proto) declares the Aster RPC service
once. The server serves gRPC, Connect JSON and gRPC-Web on the same listener;
the proto's `google.api.http` annotations describe a transcoding contract.
Older `/api/*` handlers still serve parts of the browser and TUI. New methods
belong in the proto and shared service implementation, not a new legacy route.

For local development with the header identity seam enabled:

```sh
curl -fsS http://127.0.0.1:8080/aster.v1.Aster/ListEngines \
  -H 'content-type: application/json' \
  -H 'connect-protocol-version: 1' \
  -H 'x-aster-subject: alice' \
  -d '{}'
```

OIDC deployments use a signed-in session instead of the development header.
`GetConversation` and `SendMessage` are Connect RPCs; the server derives the
subject from authentication and requires a saved notebook and registered
personal helper. `SendMessage.expected_revision` protects completed history
from stale concurrent writes; both calls take an optional `cell`, and a cell
conversation is scoped to its own cell while notebook chat never reads one.
Five exchange RPCs — `FetchQuery`, `FetchResult`, `FetchSummary`, `SendSummary`
and `UpdateQuery` — move cell material between conversations inside the
principal's notebook. See [AI assistance](ai.md) for behavior.
`RunQuery.catalog_context` identifies the browse catalog; its older `catalog`
field is the engine-native SQL alias. `ListEngines.catalog_context` filters
engine choices to connected, granted engines. See [catalogs and compute](catalogs-and-compute.md).

## What each check means

| Command | Scope |
|---|---|
| `just ci` | Format, Clippy, workspace tests, `.feature` shape, repository contract and provider matrix. Does not start PostgreSQL, a browser or a live compute stack. |
| `just features` | Only asserts every `.feature` has a `Feature` and a `Scenario`. It does not execute all scenarios. |
| `just notebook-validation` | Focused real-Git transaction and notebook-format tests, plus bound core Gherkin scenarios. Local commits only; no remote sync. |
| `just notebook-ownership-validation` | Legacy checkout owner assignment, content-revision conflicts, Git failure handling, and notebook regression suite. Personal/session branch isolation remains separate work. |
| `just ai-registration-validation` | Personal-helper validation and subject-isolated fake-upstream routing. No real model or shared registration. |
| `just catalog-routing-validation` | Bound REST/Connect Cucumber scenarios for catalog/engine admission and refusal. No backend table-policy proof. |
| `just ai-selection-validation` | REST/Connect, disposable Valkey, and browser checks for each user's notebook helper choice and resume behavior. No shared-model grant proof. |
| `just shared-group-validation` | Verified group transport through session storage and grant matching, with disposable Valkey. Shared-model use remains disabled pending immediate revocation. |
| `just shared-model-validation` | Admin-only encrypted shared registration, exact HTTPS destination policy, disposable PostgreSQL ciphertext/restart checks and opt-in chart rendering. Grant-based use remains disabled. |
| `just polaris-generic-adapter-validation` | Pinned fake Polaris Generic Table list/load plus REST/Connect/browser descriptor browsing, including path-encoded table links. Runtime capability stays disabled; no Delta row or engine-policy proof. |
| `just conversation-postgres` | Two disposable PostgreSQL databases: default/delegated history, reconnect, isolation and competing revisions. |
| `just conversation-browser` | Disposable server, fake helper and Chromium: history, sidebar, inert text, explicit SQL insertion and stale-read regression. |
| `just cell-panel-visual` | Disposable server and Chromium: the cell conversation panel renders and stays within its 30 percent ceiling. |
| `just compose-up` | Builds and starts the self-contained Compose stack. Verify `/healthz`, then `just compose-down`. |
| `just chart-lint` | Helm rendering and Kubernetes schema validation; it does not install the chart. |
| `tests/live-backends.sh` | Live Trino/Gateway and Spark checks against a reachable local stack; outside CI. |

The server Cucumber runner in
[`crates/server/tests/contracts.rs`](../crates/server/tests/contracts.rs)
selects `@contract` features without `@unautomated`. Core Cucumber scenarios
run through [`crates/core/tests/features.rs`](../crates/core/tests/features.rs).
`features/repo-setup.feature` is bound by `tests/repo-setup.sh`;
`features/live-backends.feature` is bound by its separate live shell script.
An `@unautomated` scenario can have equivalent unit/integration coverage, but
its Gherkin steps have not run. The
[dated feature inventory](documentation-plan.md#feature-contract-audit-2026-09-23)
names current files and gaps. Never report a `.feature` as passed merely
because `just features` printed its file count.
