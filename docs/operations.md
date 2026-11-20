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

## Durable workspace activation and recovery

The feature defaults off. Enabling it requires PostgreSQL metadata, verified OIDC
without `ASTER_DEV_LOGIN`, a signed `ASTER_IDP_USER_UUID_CLAIM`, team Git policy,
and fresh Authentik authority configured with API origin, read token, admin group
UUID and editor group UUID. `ASTER_TEAM_GIT_ENABLED=1` and
`ASTER_TEAM_GIT_POLICY_FILE` select the approved numeric repository/installation
policy. Supply `ASTER_GITHUB_APP_ID`, `ASTER_GITHUB_APP_KEY_NAME` and the named
private-key secret through the existing secret provider. The App needs scoped
contents write and pull-requests write permission for explicit publication.

Recovery needs `ASTER_WORKSPACE_S3_ENDPOINT` (HTTPS origin),
`ASTER_WORKSPACE_S3_BUCKET`, `ASTER_WORKSPACE_S3_REGION`, and secret-provider keys
`ASTER_WORKSPACE_S3_ACCESS_KEY` and `ASTER_WORKSPACE_S3_SECRET_KEY`. The bucket
must be private and permit immutable PUT, GET, prefix LIST and DELETE. Provider
names are `ASTER_WORKSPACE_CHECKOUTS=git`, `ASTER_WORKSPACE_HOSTING=github-app`
and `ASTER_WORKSPACE_RECOVERY=s3`; unknown names fail startup. Do not place keys
in chart values, source control or browser storage.

For Helm, set `server.workspaces.enabled`, `secretName` and `policyConfigMap`.
The existing Secret supplies the environment names above; the ConfigMap contains
`teams.json`. Configure `server.identity` separately. The chart mounts private
working files under a bounded ephemeral `/tmp`; PostgreSQL and S3 hold recovery
state. Compose remains off by default. A local verified-identity override must
remove the development login environment entry and supply policy mounts and
external settings before enabling the flag. No live rollout is implied by local
fixture tests.

Back up workspace metadata and recovery objects together. Restoring only an older
metadata backup can roll back publication knowledge; keep remote-operation
journals and current generation pointers intact. Never clear a pending journal
or advance an epoch merely because a browser disconnected or a timeout elapsed.
The owning operation continues independently of the HTTP request. A running
operation that survives a process crash requires an operator to establish that
the old process and its Git child are stopped and no remote request remains in
flight before marking it uncertain for the lookup-only recovery path. If that
cannot be established, keep the workspace read-only and preserve its export.
A remote branch missing or changed after an uncertain Push stays fenced.

Keep versioned snapshots and migrations when rolling back. Disable the opt-in
and preserve both stores; an older server cannot interpret draft revisions as
legacy Git blob revisions. Do not delete recovery objects to reclaim quota while
their metadata pointer is current. Maintenance retries incomplete workspace
deletion and periodically revisits tombstones for delayed uploads. Completed
workspaces are retained until explicit deletion; bucket expiry must not silently
remove live generations. Fixed initial quotas require operator capacity sizing;
no production latency or storage-cost claim has been established.
