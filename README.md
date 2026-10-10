# Aster

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/brand/svg/aster-color-dark.svg">
  <img src="assets/brand/svg/aster-color-light.svg" alt="Aster logo" width="430" height="128">
</picture>

Aster is a Git-backed SQL notebook with a web editor and terminal client.
It browses catalogs, runs queries through configured engines, records an audit
trail, and offers personal AI helpers with private notebook chat.

**Current boundary:** Trino and Spark Connect execute real queries; StarRocks,
Nessie and Unity are stubs. Notebook commits stay in a local Git checkout.
Configured catalog/engine bindings now restrict application routing, but
per-user backend data policy, Polaris generic Delta reads and GitHub sync
remain planned. An opt-in admin-only encrypted shared-model registry exists;
group grants and ordinary-user shared use remain planned.
See the [handbook](docs/README.md) for the current behavior and accepted drafts.

## Start locally

The current ODCS first-test demo uses **Authentik**, including fresh membership
checks and registered catalog teams. See the [deployment receipt](docs/plans/odcs-ai-catalog-rv.md)
for access, verified scope and rollback. The Compose Keycloak stack below is the
older generic-OIDC evaluation fixture, not the active catalog demo identity.

Enter the declared toolchain, then choose one path:

```sh
devenv shell
just ci

# Fast browser/API loop, with mock catalog and engine and local-only dev login:
ASTER_BIND=127.0.0.1:8080 \
ASTER_ENGINES='mock-local;mock;local' \
ASTER_CATALOGS='mock-local;mock;local' \
ASTER_CATALOG_BINDINGS='mock-local;mock-local;mock-local;unprotected' \
just run
# Open http://127.0.0.1:8080/dev-login
```

`just compose-up` starts the server, controller, PostgreSQL, Valkey and
Keycloak with mock compute. `just stack-bootstrap local` starts the full
minikube stack with Polaris, Trino/Gateway and Spark Connect; its default
resource request is 12 CPUs and 64 GiB. See [deployment](docs/deploy.md)
for requirements, configuration, Helm and teardown.

## Find your path

- [Architecture and state](docs/architecture.md)
- [Catalogs, compute and data access](docs/catalogs-and-compute.md)
- [Data contracts and semantic output](docs/data-contracts-and-semantics.md)
- [Notebooks and Git](docs/notebooks-and-git.md)
- [AI assistance and private chat](docs/ai.md)
- [Deployment](docs/deploy.md) and [operations](docs/operations.md)
- [API and test contracts](docs/api-and-tests.md)
- [Contributing](CONTRIBUTING.md)
- [Visual identity and reusable assets](assets/brand/README.txt) · [Identity sheet](assets/brand/guide/aster-identity-sheet.png)

The [provider matrix](docs/provider-matrix.md) lists domain ports and adapters.
The [documentation plan and feature audit](docs/documentation-plan.md) record
the remaining test binding and delivery decisions.

## Code map

`crates/core` owns domain types and ports; `crates/engines` and
`crates/catalogs` implement adapters. `crates/server` owns HTTP, RPC,
identity, notebooks and AI. `crates/controller` reconciles metadata and
retention; `crates/tui` is the terminal client. `proto/aster.proto` declares
the shared RPC surface. `crates/*/features/` holds behavior contracts;
`features/` covers repository and live-stack behavior.

`just ci` checks formatting, Clippy, workspace tests, feature-file structure,
repository conventions and provider inventory. Real PostgreSQL, browser,
Compose, Helm and live compute checks are separate owner recipes; see
[API and tests](docs/api-and-tests.md). A `.feature` marked `@unautomated`
is a contract draft, not a passing test.
