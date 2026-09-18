# AGENTS.md — working in `aster`

Agent-facing rules. They are a subset of `CONTRIBUTING.md`; if the two conflict,
`CONTRIBUTING.md` wins.

## Grounding first

Read the whole path a change touches — trait, adapter, handler, test, feature
file — before editing. `aster-core` is the domain: engine/catalog ports, RBAC,
grants, audit, sessions, notebooks, LLM config, data contracts. `aster-engines`
and `aster-catalogs` are adapters. `aster-server`, `aster-controller` and
`aster-tui` are entrypoints.

## Where code goes

| Change | Location |
|---|---|
| Domain type, port trait, policy | `crates/core/src/*.rs` |
| Engine adapter | `crates/engines/src/lib.rs` |
| Catalog adapter | `crates/catalogs/src/lib.rs` |
| Secret, metadata, state or notebook provider | `crates/server/src/providers.rs` (one arm per provider) |
| Provider inventory | `docs/provider-matrix.md`, checked by `just providers-check` |
| HTTP route or handler | `crates/server/src/{main,web,ai,oidc}.rs` |
| Endpoint declaration | `proto/aster.proto` (one definition serves gRPC, Connect and gRPC-Web) |
| RPC service implementation | `crates/server/src/api.rs` |
| Reconcile behavior | `crates/controller/src/main.rs` |
| Terminal client | `crates/tui/src/main.rs` |
| Behavior contract | `crates/<crate>/features/*.feature`; repo conventions in `features/*.feature` |
| Unit test | `#[cfg(test)]` in the same file |
| Integration test | `crates/<crate>/tests/*.rs` |

Prefer `foo.rs` + `foo/` over `mod.rs`. Anything not part of the crate's
contract is `pub(crate)`.

## Non-negotiables

- `aster-core` never gains an IO dependency (`tokio`, `reqwest`, `sqlx`,
  `std::fs`, `std::process`).
- Blocking calls (git CLI, filesystem, large loops) go through
  `tokio::task::spawn_blocking`.
- Every external dependency is a port in `aster-core` with one implementation
  per provider, chosen by name in a selection function; no `match` on a provider
  kind at a call site, and an unknown name is refused at startup.
- No `#[derive(Debug)]` where the struct holds a secret; hand-write `Debug` and
  print `<redacted>`.
- Never return raw internal errors above 4xx; log detail with `tracing`, return
  a generic message.
- Authenticate + authorize once per handler through the shared extractor in
  `server`, not with copy-pasted preambles.
- Add `#[non_exhaustive]` to new public enums.
- New engine/catalog: implement the core trait and register by kind — no
  kind-matching outside the registry layer.

## Verify before claiming done

```bash
just ci            # fmt + clippy -D warnings + tests + features + repo contract
just compose-up    # when server/controller/config changed
just chart-lint    # when charts/ or manifests changed
```

Report evidence (command + result). If you ran on the remote host, say so.

## Behavior files

Every behavior change lands with a `.feature` beside the code it validates and a
test that fails without the change. Scenarios state observable outcomes (status
codes, files, rows), never implementation detail. An unbound scenario file is a
draft and must be marked as such in its header comment.

## Repository conventions to follow

- **devenv** is the only toolchain source: add tools to `devenv.nix`, never to a
  global profile.
- **devspace.yaml** is the local Kubernetes loop; keep it working when manifests
  move.
- **justfile** owns every workflow: add a recipe instead of documenting a long
  command. Recipes that do real work use a bash shebang with `set -euo pipefail`.
- Images are digest-pinned Harbor references; secrets come from `ExternalSecret`
  and never appear in values, manifests, or chart templates.

## Also

- Do not commit, push, or apply anything unless the user asked for it in this
  session.
- Do not create documentation files the user did not ask for.
- Keep `docs/proposals/*` in the `platform` repo in sync when a decision changes;
  update `docs/proposal-index.md` in the same change.
