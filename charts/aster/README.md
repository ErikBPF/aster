# aster chart

Deploys the `aster` server (API + web UI) and the metadata reconcile controller.

```bash
helm dependency build charts/aster        # no deps today, kept for symmetry
helm template aster charts/aster | kubeconform -strict -summary -ignore-missing-schemas
just chart-lint
```

## Required values

Every value below has a placeholder default. Before a release, all of them must
be real:

| Value | Why |
|---|---|
| `image.digest` | `sha256:…` of the published Harbor image; a tag alone is not accepted in the cluster |
| `ingress.host` | the Traefik host for the deployment |
| `server.oidc.issuer` / `clientId` / `redirectUri` | Authentik application endpoints |
| `externalSecret.vaultPath` | Vault path holding `session_key`, `database_url`, `oidc_client_secret` |
| `database.host` / `database.name` | metadata Postgres |

## Notes

- The chart never contains secret values; `externalSecret.enabled=true` renders an
  `ExternalSecret` against the namespace's `ClusterSecretStore`.
- `persistence.storageClassName` is mandatory (the platform contract rejects PVCs
  without it).
- Engine and catalog pools are lists; ordering defines the default entry.
