# Operate and recover Aster

Start with `GET /healthz`, `just compose-logs` for Compose, or
`just stack-status` and namespace pod logs for the local minikube stack.
`ASTER_METRICS_BIND` serves `/metrics` on a separate listener. A healthy Aster
server does not prove Trino, Spark, Polaris or a model endpoint is healthy;
check `/api/engines` and `/api/catalogs`, then run the corresponding live
fixture check. `tests/live-backends.sh` checks independent Trino and Spark
queries, app grant denial and audit, but not shared Iceberg or Delta.

## Preserve state before replacement

| Data | Backup and restore boundary |
|---|---|
| Notebook checkout | Copy `ASTER_NOTEBOOK_DIR` and its actual Git administrative directory (a linked worktree's `.git` is a pointer), including `aster.source`, with ownership and permissions. Verify `git fsck`, `git status` and a known revision after restore. Aster's DevSpace stack mounts `emptyDir`; replacing its server pod loses the checkout unless it is restored. The Helm chart enables a notebook PVC by default; Compose uses a named volume. |
| Metadata PostgreSQL | Back up and restore the database with PostgreSQL tooling. It contains grants, audit, notebook owner/revision records, personal helper registrations, opt-in encrypted shared registrations and default chat history. Treat the backup as a credential-bearing artifact; preserve the separate shared-model keyset too. |
| Dedicated conversation PostgreSQL | Back up separately when configured. Switching `ASTER_CONVERSATION_DATABASE_URL` does not migrate history automatically; retain the old database until a verified copy/recovery is complete. |
| Valkey | Contains sessions, OIDC handshakes and working state. Losing it signs users out and loses working position; it does not erase Git notebooks or PostgreSQL conversations. |
| Data contracts | Preserve the configured files or source repository; Aster reloads them only on server startup. |

Stop writes while taking or restoring a notebook checkout snapshot. Check a
known notebook through `GetNotebook` after restore; a clean Git tree alone
does not prove the application can read it. Restore Git and owner metadata from
matching points; a mismatch leaves saves conflicted until an administrator
reconciles the exact owner and current blob. For PostgreSQL, check that a known
private conversation and registration can be read after restart with the same
identity. If shared registrations are enabled, verify they decrypt before
retiring any old key version. Keep snapshots and logs free of raw helper
tokens.

For a **populated disposable DevSpace stack**, this archives the checkout
without the volume root directory entry (which the non-root server cannot
chmod during restore):

```sh
umask 077
kubectl -n aster exec deployment/aster-server -- \
  sh -c 'cd /data/notebooks && tar -cf - .git ./*.aster' > notebooks.tar
```

After the replacement pod is Ready and writes are still stopped:

```sh
kubectl -n aster exec -i deployment/aster-server -- \
  tar -C /data/notebooks -xf - < notebooks.tar
kubectl -n aster exec deployment/aster-server -- \
  sh -c 'cd /data/notebooks && git fsck --no-dangling && git status --porcelain'
```

Check the expected HEAD and read a known notebook before reopening access.
For Compose metadata, `podman compose exec -T postgres pg_dump -U aster -d aster
-Fc > metadata.dump` creates a database archive under the current umask;
restore it into a new empty database during maintenance, then verify the app
against it. Dedicated chat PostgreSQL needs its own dump and restore.

## Upgrade and rollback

Build and validate the candidate image with `just ci`, `just build` or
`just build-minikube`, plus Compose and chart checks when their configuration
changes. Pin the image digest for a repeatable chart deployment. Back up Git
and PostgreSQL before replacing the server. The server and controller apply
idempotent metadata scripts at startup; the dedicated chat database receives
only its conversation schema. Check migration errors and readiness before
reopening writes.

To roll back application code, restore the previous image while retaining
database and notebook backups. There is no automatic destructive
down-migration. Verify the older binary can read the retained schema; if it
cannot, restore its matched backup as a separate recovery action. A newly
created GitHub remote or changed conversation database is outside this local
rollback path. [Deployment](deploy.md) lists the owning recipes and the
[notebook guide](notebooks-and-git.md) states the current local-only Git limit.
