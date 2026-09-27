# aster chart

Deploys the `aster` server (API + web UI) and the metadata reconcile controller.
It is self-contained by design: after side-loading the Aster image, a default
install renders its own Postgres and secrets and uses public upstream images
for the supporting services. Use it on a private throwaway minikube cluster.

```bash
helm dependency build charts/aster        # pulls the bundled valkey chart
helm template aster charts/aster | kubeconform -strict -summary -ignore-missing-schemas
just chart-lint
```

## Installing on minikube

`just build-minikube` builds `aster-server:local` and side-loads it into the
cluster's containerd, and the chart's image default points at exactly that, so
no registry is involved:

```bash
just minikube-start
just build-minikube
just chart-install                        # helm install into the aster namespace
just chart-uninstall
```

## Values

Defaults render a private development installation. The engine and catalog
endpoints are placeholders until you supply reachable services.

| Value | Default and why |
|---|---|
| `image.repository` / `tag` / `digest` | `aster-server` / `local` / `""`. Any registry works; `digest` is optional and wins over the tag when set |
| `postgres.enabled` | `true`. The chart renders its own Postgres; set `false` to point at one you run |
| `postgres.persistence.storageClassName` | `""`, i.e. the cluster default. Set it only if the cluster has no default class |
| `database.host` | `""`, meaning the in-chart Postgres. Set it, plus `secrets.existingSecret`, for an external database |
| `secrets.existingSecret` | `""`, meaning the chart renders its own Secret with a generated Postgres password and the composed `database_url` / `state_url` |
| `ingress.enabled` | `false`. Enabling it requires configured OIDC; set `className`/`host` for your ingress controller |
| `networkPolicy.enabled` | `false`. Enable it and list the namespaces allowed to reach the server and the metrics port |
| `metrics.serviceMonitor.enabled` | `false`; needs the Prometheus operator CRDs. Set `labels` to whatever your Prometheus selects on |
| `valkey.enabled` / `auth.enabled` | `true` / `false`. The bundled instance carries session state; auth is off for a dev cluster — set `secrets.valkeyPassword` or `valkey.auth.usersExistingSecret` when it is shared |
| `server.identity.*` | Defaults to `none` for development login on a private cluster. For a shared deployment, set `kind: oidc`, the issuer, client ID, redirect URI and client Secret. An empty issuer with `kind: oidc` cannot log in. |
| `engines` / `catalogs` | Pools; ordering defines the default entry. Replace the sample endpoints before querying or browsing. |
| `server.catalogBindings` | Comma-separated `browse_catalog_id;engine_id;native_sql_catalog;unprotected|protected` connections. The policy field is required. The default sample Polaris/Trino binding is `protected`, so queries and metadata refuse until backend delegation is configured; use `unprotected` only for an isolated development fixture. |
| `server.sharedModels.*` | Disabled by default. The registry needs `enabled`, approved HTTPS `allowedOrigins`, `activeKey`, and `keysetSecretName`; that existing Secret holds `shared_model_keys` as version-to-base64-32-byte-key JSON. Set `authorityEnabled` for fresh SSO admin authority before ordinary use. It requires OIDC, `server.identity.userUuidClaim`, HTTPS `authentikApiOrigin`, stable `adminGroupUuid`/`editorGroupUuid`, and `readTokenSecretName`; the separate existing Secret holds `authentik_read_token` for a dedicated Aster read identity. `useEnabled` separately opts into ordinary shared use and requires authority. Keep both secrets out of values. Do not enable ordinary use before live group-removal proof. |

## Notes

- The chart never contains secret values. With `secrets.existingSecret` empty it
  renders a Secret and reuses an existing `postgres_password` across upgrades
  rather than rotating it.
- Shared-model keys live in a separate existing Secret. Keep old and new key
  versions during the [rekey procedure](../../docs/ai.md#shared-models);
  changing `activeKey` alone does not re-encrypt existing rows.
- The Authentik read token is a separate existing Secret and needs only the
  user/group read scope. The chart defaults `authorityEnabled: false` and
  `useEnabled: false`. Rendering either opt-in checks configuration, not live
  revocation.
- There is no ExternalSecret, Vault, Harbor or Argo dependency anywhere in the
  chart: it is meant to be installable by anyone, anywhere.
- `persistence.storageClassName` is omitted when empty so the cluster default
  applies; set it when the cluster has no default StorageClass.
- Engine and catalog pools are lists; ordering defines the default entry.
