# aster chart

Deploys the `aster` server (API + web UI) and the metadata reconcile controller.
It is self-contained by design: a default `helm install` renders its own
Postgres and its own secrets and pulls only public upstream images, so it
installs on a throwaway minikube cluster with nothing else present.

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

Every value has a working default; none points at private infrastructure.

| Value | Default and why |
|---|---|
| `image.repository` / `tag` / `digest` | `aster-server` / `local` / `""`. Any registry works; `digest` is optional and wins over the tag when set |
| `postgres.enabled` | `true`. The chart renders its own Postgres; set `false` to point at one you run |
| `postgres.persistence.storageClassName` | `""`, i.e. the cluster default. Set it only if the cluster has no default class |
| `database.host` | `""`, meaning the in-chart Postgres. Set it, plus `secrets.existingSecret`, for an external database |
| `secrets.existingSecret` | `""`, meaning the chart renders its own Secret with a generated Postgres password and the composed `database_url` / `state_url` |
| `ingress.enabled` | `false`. Enable it and set `className`/`host` for your own ingress controller |
| `networkPolicy.enabled` | `false`. Enable it and list the namespaces allowed to reach the server and the metrics port |
| `metrics.serviceMonitor.enabled` | `false`; needs the Prometheus operator CRDs. Set `labels` to whatever your Prometheus selects on |
| `valkey.enabled` / `auth.enabled` | `true` / `false`. The bundled instance carries session state; auth is off for a dev cluster — set `secrets.valkeyPassword` or `valkey.auth.usersExistingSecret` when it is shared |
| `server.identity.*` | OIDC settings. `issuer` is empty, which leaves the development seam; set it for a real IdP |
| `engines` / `catalogs` | pools; ordering defines the default entry |

## Notes

- The chart never contains secret values. With `secrets.existingSecret` empty it
  renders a Secret and reuses an existing `postgres_password` across upgrades
  rather than rotating it.
- There is no ExternalSecret, Vault, Harbor or Argo dependency anywhere in the
  chart: it is meant to be installable by anyone, anywhere.
- `persistence.storageClassName` is omitted when empty so the cluster default
  applies; set it when the cluster has no default StorageClass.
- Engine and catalog pools are lists; ordering defines the default entry.
