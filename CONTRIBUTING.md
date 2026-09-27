# aster — contributing

`aster` is a git-backed SQL notebook: a Rust workspace (web + TUI + controller)
over pluggable query engines and catalogs. This file is the human contract for
working here; `AGENTS.md` is the agent-facing subset with the same rules.

## Environment

Everything comes from the declared devenv. Do not install toolchains by hand and
do not rely on a host-wide `cargo`.

```bash
devenv shell          # cargo, clippy, rustfmt, kubectl, helm, kubeconform, devspace
just                  # lists every recipe
just check            # cargo check --workspace --all-targets
just test             # unit tests, every crate
just features         # checks each *.feature has a Feature and Scenario
just repo-check       # binding for features/repo-setup.feature
just providers-check  # ports, providers and selection sites match the matrix
just ci               # fmt + clippy + tests + feature shape + repo + providers
```

Optional `just remote-*` recipes require `ASTER_BUILD_HOST`; there is no
implicit host. They sync the workspace and run the same commands remotely
inside `devenv shell`. `just check` builds locally inside devenv.

## The rules

1. **Behavior ships with its contract.** Every behavior change gets a
   `.feature` beside its owning crate (`crates/<crate>/features/`), or under
   `features/` for repository conventions, and an executable test that fails
   before the implementation. A `.feature` without bound steps is a draft,
   not a test. See [API and tests](docs/api-and-tests.md) for runner scope.
2. **Tests live with their owner.** In-file `#[cfg(test)]` for a unit;
   `crates/<crate>/tests/` for anything exercising the public API. No shared
   test-only crate unless two crates genuinely share a harness.
3. **`core` has no IO.** No `tokio`, `reqwest`, `sqlx`, `std::fs`, or
   `std::process` in `aster-core`. Ports (traits) live in core; adapters live in
   `engines`, `catalogs`, or `server`.
4. **Blocking work goes off the async runtime.** Shelling out, filesystem, and
   CPU-bound loops use `tokio::task::spawn_blocking`.
5. **Secrets never print.** No `#[derive(Debug)]` on a type holding a token,
   password, or client secret; write the `Debug` impl and redact.
6. **Errors at the boundary.** `thiserror` in libraries, `anyhow` in binaries.
   Internal detail (git stderr, upstream bodies) is logged, never returned to a
   client above a 4xx.
7. **Public enums are `#[non_exhaustive]`** unless exhaustiveness is deliberate.
8. **Keep `main.rs` a composition root.** Routing, handlers, and identity live
   in modules (`web.rs`, `ai.rs`, `identity.rs`, …), not in the entrypoint.
9. **One way to add a plugin.** A new engine/catalog implements the core trait
   and is registered by kind; do not branch on kind anywhere else. Every domain
   that talks to an external system is listed in
   [docs/provider-matrix.md](docs/provider-matrix.md) with its trait, providers
   and selection site; `just providers-check` fails if a trait is missing from
   that table or a selection site stops refusing unknown names. Adding a
   provider means a new implementation plus one arm in the selection function —
   never a `match` at a call site.
10. **No new dependency without a reason in the PR.** Stdlib or an
    already-present crate first.
11. **An endpoint is declared once, in `proto/aster.proto`.** The generated
    service (`crates/server/src/api.rs`) is what gRPC, Connect JSON and gRPC-Web
    all use; the `/api/*` JSON handlers are legacy and only stay while the
    server-rendered page uses them. Regenerating needs `protoc`, which the
    devenv shell and both Docker images provide.

## Containers and deployment

Default delivery is container + chart, both living in this repo:

- `docker/Dockerfile.server` builds `aster-server` **and** `aster-controller`
  from one image; `docker/Dockerfile.tui` builds the terminal client.
- `docker-compose.yml` is the local stack (server, controller, PostgreSQL,
  Valkey and development Keycloak) and is the first stack a change must keep
  working.
- `charts/aster/` is the Helm chart. Render and lint it before every release:

```bash
just compose-up          # local stack, real Postgres
just chart-lint          # helm dependency build + helm template + kubeconform
```

Images come from any public registry. `charts/aster/values.yaml` defaults to the
`aster-server:local` image `just build-minikube` side-loads into minikube; pin a
digest there when one is published.

## Change flow

1. Ground the change (behavior, callers, owners) before editing.
2. Write/extend the `.feature` and the failing test first.
3. Implement until `just ci` is green.
4. Update the proposal in `platform` when a cross-repo decision changes, and the
   affected `.feature` in the same change. Update the relevant
   [handbook page](docs/README.md) when shipped behavior or an owner command
   changes; keep planned behavior visibly labelled.
5. Human review: the reviewer reads the diff in `tuicr`; treat `issue` comments
   as blocking and answer every other comment.

Commit messages are Conventional Commits (`feat(core): ...`), imperative,
explaining why and what was verified. Never commit secrets, `.env`, or files
under `secrets/`.

## Reporting security issues

Do not open a public issue for a vulnerability. Report it privately to the
maintainer (see `platform` repo conventions) with reproduction steps and impact.
