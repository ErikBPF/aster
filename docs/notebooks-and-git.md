# Notebooks and Git

**Implemented:** A notebook is a diff-friendly `.aster` text file with cells
and SQL. `ASTER_NOTEBOOK_DIR` names a local Git checkout; Aster initializes one
if needed. `ASTER_NOTEBOOK_BRANCH` chooses one process-configured branch
(`session` by default). A successful save commits only the notebook path when
there is a change and returns the checkout's HEAD revision. Repeating an
unchanged save returns HEAD without a new commit. `GetNotebook` and listing
read the committed tree; uncommitted external edits are not notebook state.
Git hooks are disabled for this store's commands.

The production legacy branch is not created separately for each signed-in user
or browser session. It does not fetch or push a remote. Creating a notebook
requires an explicit absence precondition; updating one requires the content
revision of the committed notebook blob. A stale revision returns a conflict
without replacing newer content. New content is assigned to its creator in
the metadata store. An administrator must assign an exact owner to older
unassigned content before anyone can edit it; corrections require the current
owner, content revision and a reason. Back up both the checkout's Git
administrative data, including `aster.source`, and the metadata database's
owner records. A response commit revision is local history; the content revision is
for stale-write checks. Neither proves GitHub synchronization. IDs accept only
ASCII letters, digits, `_` and `-`, up to 64 characters. The server checks
identity and editor permission before a save.

For a legacy notebook, an administrator reads its current content revision,
then calls `PUT /api/admin/notebooks/{id}/owner` with `owner` (the exact
authenticated subject) and `expected_content_revision`. To correct an existing
assignment, include `expected_owner` and a nonempty `reason`. A content or
owner mismatch returns a conflict; unassigned material stays readable but
read-only until assignment succeeds.

Use the notebook page's **Save** action or the `SaveNotebook` RPC. A saved
notebook can be reopened through `GetNotebook`; from the checkout, `git log`
and `git show <revision>` inspect older committed content. The v2 text format
escapes SQL lines that resemble cell markers or begin with a backslash; the
reader still accepts v1 files. Back up the repository before replacing its
volume or enabling v2 writes; an older Aster version cannot read v2 files.
See [operations](operations.md).

In the browser, `Ctrl/⌘+Enter` runs the focused cell, `Shift+Enter` runs it
and advances, and `Ctrl/⌘+S` saves the notebook. The toolbar also offers
**Run all**. Running SQL is separate from committing notebook text.

**Verified:** [`notebooks.feature`](../crates/server/features/notebooks.feature)
runs against a temporary real Git repository through the server Cucumber
runner. [`notebook-format.feature`](../crates/core/features/notebook-format.feature)
checks serialization. `just notebook-validation` also exercises local Git
transaction regressions. `just notebook-ownership-validation` runs 13 owner
tests, six bound ownership scenarios and a disposable PostgreSQL owner/audit
reconnect check. The Git store also refuses a second process opening the same
checkout while the first holds its lock. These checks do not prove remote sync,
personal branch isolation or the lock's behavior on Build-host's volume.

`just notebook-isolation-validation` also passes a disposable local fixture
for two teams, isolated session branches, versioned maintainer target changes,
pinned default ancestry and owner-only read-only recovery of an expired
session's content. A branch key is derived from the session ID without putting
the bearer ID in a Git ref or checkout path. This fixture is not activated by
the production `build_state`: client route parity and a live GitHub App
installation remain rollout gates.

The team target configuration route has a separate default-off PostgreSQL/App
path. A manual server deployment may set `ASTER_TEAM_GIT_ENABLED=1` with a
PostgreSQL `DATABASE_URL`, `ASTER_OIDC_ISSUER` and signed
`ASTER_IDP_USER_UUID_CLAIM`,
`ASTER_AUTHENTIK_API_ORIGIN`, and the dedicated
`ASTER_AUTHENTIK_READ_TOKEN` secret. Set `ASTER_TEAM_GIT_POLICY_FILE` to a
server-owned JSON file mapping each team to exact member/maintainer group
UUIDs, one numeric GitHub App installation ID, and allowed repository names
with numeric IDs. Set `ASTER_GITHUB_APP_ID` and
`ASTER_GITHUB_APP_KEY_NAME` naming the App private
key in the configured SecretStore. A missing or invalid dependency fails
startup. The chart has no team Git policy mount or App secret wiring yet, so
this is a manual test path, not an Build-host deployment recipe.

The policy file uses these exact fields; replace every illustrative identifier
with the team's verified values:

```json
{
  "alpha": {
    "member_group_uuid": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    "maintainer_group_uuid": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    "installation_id": 41,
    "allowed_repositories": { "example/team-notebooks": 73 }
  }
}
```

A freshly verified team maintainer uses
`PUT /api/teams/{team}/notebook-target` with `repository` and
`default_branch`, plus `If-None-Match: *` on creation or the quoted current
version in `If-Match` on correction. The server checks current group UUIDs,
the team allowlist, numeric installation/repository identity and exact
default ref before the PostgreSQL version/audit transaction. A current team
member may read the target with `GET` on the same path. The response reports
`active: false`: configuration does not start a workspace, clone, or GitHub
Sync. `just notebook-team-target-validation` proves this route with a fake
App and disposable PostgreSQL, including two-team denial, stale version,
identity removal, restart and audit checks. It does not prove a live GitHub
installation or safe workspace activation.

The local team-workspace fixture exposes team-qualified conversation and
helper-choice REST and Connect routes. Connect adds optional team and
`session`/`personal` workspace selectors; the server derives the branch from
the authenticated session and verifies that the notebook exists there before
reading state or sending a chat turn. A repeated notebook ID does not share a
transcript or helper choice across teams or sessions. Unqualified Connect chat
and helper requests refuse in team mode; the single-checkout wire behavior
remains available when team mode is off. `just notebook-isolation-validation`
covers same-workspace REST/Connect reads, one scoped chat turn and denied turns
that never reach the helper. The opt-in local team page routes Save, explicit
Sync, helper choice and chat through this context; `just
team-browser-validation` checks its browser requests with mocked transport
and a team page route test. Production workspace activation, current editor
write permission, team notebook list/TUI parity and live GitHub proof remain
unfinished.

`just notebook-sync-validation` exercises the opt-in team route against a
disposable local bare repository. `POST /api/teams/{team}/notebooks/{id}/sync`
pushes only the authenticated caller's server-derived personal or session
branch. It verifies the exact remote ref and commit before reporting `synced`.
The route rejects client-selected repositories or refs; a failed push retains
the local commit. A ref lease prevents concurrent remote creation or deletion
from being overwritten, and an acknowledged sync record prevents later remote
deletion from silently recreating the branch. A durable pre-push intent is
bound to the destination, ref and local commit. After a lost acknowledgement,
a retry reconciles it only if that exact remote ref still has the intended
commit; a deleted or changed ref requires explicit recovery. A failed attempt
with no remote update can also leave an intent requiring operator recovery.
This local fixture is not enabled by production `build_state` and does not
prove a live GitHub installation or complete Sync status UI.

**Still planned:** The accepted [remote sync contract](../crates/server/features/notebook-remote-sync.feature)
uses a shared GitHub repository configured separately for each team by a
designated maintainer with verified team membership. That maintainer also
configures the team's default branch. Aster must verify that the target is
authorized for that team before accepting the change. Members without the
maintainer designation and maintainers of another team cannot change it.
The repository contains authorized personal/session branches. The same
numeric repository ID cannot be assigned to two team policies while branch
names lack a team prefix; startup refuses such a policy before activation.
Save commits locally; a separate **Sync** action will push the authorized
branch and verify its remote revision. Each personal/session branch starts
from the team default branch; only its owner may sync it through Aster, and
promotion uses a separate pull request. GitHub repository readers may inspect
those branches. An administrator must assign the owner of each legacy notebook
before migration; unassigned notebooks remain read-only. Remote recovery,
accurate sync status, failure handling and branch isolation are required. This
is an `@unautomated` draft; conflict UX and a named disposable GitHub target
for live validation remain open. No global repository is assumed. Do not label
a local commit as remotely saved.
