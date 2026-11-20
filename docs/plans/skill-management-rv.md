# Skill management, assets and deletion

## Outcome and accepted behavior

Human seed: “on edit we should manage skill and versions. We should also have a delete button inside. Lets also taker a better look on how to declare extra assets. The naming/pathing in confuse”.

Edit is now the management screen for instructions, reference assets, immutable
version history, permissions and deletion. The separate version-management
modal/library shortcut is removed. Earlier versions remain read-only; owners
save new versions from current content. Permissions remains available from both
the library and editor. No automatic conversation upgrade or individual-version
deletion is introduced.

Assets now separate File name from Folder. New files default to `references/`;
a package-path preview explains their location relative to `SKILL.md`. Existing
root and nested paths round-trip unchanged. A filename cannot contain folders;
the folder field supports nested relative directories. Existing text-only,
32 KiB, 16-file, duplicate and traversal bounds remain enforced. This is logical
package organization, not filesystem access or bundled execution.

## Implementation and review

DeleteSkill is declared in proto and implemented through the existing core
SkillStore port, memory/PostgreSQL adapters and authenticated API. Only a current
owner with write authority can delete. Both content and sharing revisions must
match; adapter locking prevents stale deletion from bypassing those checks.
Deletion removes all version content while retaining the identifier as a
tombstone. A future registration cannot reuse that identity and give old pins
replacement content. No schema migration is needed.

The confirmation names the skill and explains the effect on all versions and
shared users. Independent review identified that historical dependencies prevent
further messages even after deselection. The warning now explicitly requires a
new conversation; existing messages remain visible. The reviewer found no other
static security blocker. No resurrection path was found; static review does not claim exhaustive
coverage of concurrent operation orderings.

First-draft source is preserved in `/tmp/aster-skill-management-draft-1.tar.gz`.
The API regression failed on the missing DeleteSkill route; browser regressions
failed on missing inline history/asset controls. A separate assertion-failing
warning test verified the conversation-impact wording before correction.

## Evidence, limits and continuing work

`/tmp/aster-skill-delete-red.log`, `/tmp/aster-skill-manage-ui-red.log` and
`/tmp/aster-skill-delete-warning-red.log` retain RED receipts. The first API GREEN
attempt lost its SSH connection before completion and is not a passing receipt.
The updated browser suite passed 24 flows. Final runtime, PostgreSQL, CI and
local deployment outcomes are recorded below only after observation. Unbound
Gherkin remains a behavior contract, not an executed test claim.

Deployment authorization remains local Podman localhost:8081, with builds/tests
on Apollo. Donsetch must be recreated after replacing its shared server network
namespace. Preserve the current image, config and database before rollout.
Deletion has no undo UI; database recovery requires the pre-deletion backup.
No user skill is to be deleted during deployment verification: use a uniquely
named disposable fixture. No commits, pushes or external MCP mutations.

## Observed verification

- Browser: 24 flows passed, including existing root/nested asset paths, invalid
  filenames, inline versions, confirmation wording and cancellation.
- API: eight tests passed; deletion checks ownership, content/sharing conflicts,
  every historical version and identifier reservation.
- Conversation regression passed: deleted guidance blocks the next upstream
  request even after deselection, while saved messages remain unchanged.
- PostgreSQL: five checks passed, including restart, content purging, tombstone
  reservation and concurrent double-delete. Initial test setup raced two fresh
  schema creations; the owner runner now serializes test initialization while
  concurrent operations within the test remain concurrent.
- Real browser/server/PostgreSQL: `AI_REGISTRY_LIVE_BROWSER_OK` includes confirmed
  skill deletion and absence after restart. A navigation correction returned to
  the library from the existing editor; assertions were retained.
- Independent final source review found no remaining verified blocker. Thirteen
  changed source/test paths matched Apollo. No schema migration was added.

Receipts: `/tmp/aster-skill-management-runtime.log`,
`/tmp/aster-skill-management-final-validation.log`,
`/tmp/aster-skill-management-ci.log`, and
`/tmp/aster-skill-management-source-check.log`. Full CI and deployment are still
pending at this checkpoint; their final observed outcomes follow below.

## Final deployment and recovery

`just ci` finished with `CI GREEN` on Apollo. The last change was a CSS-only
replacement of fixed delete-button colors with the existing theme tokens; its
rebuilt image was checked in both light and dark mode on the deployed browser.
The final 13-path source manifest matches Apollo, and `git diff --check` passes.

Local server and controller now run image
`1470894f8b130833e28df7aa8ea0f77701993f6d41aadfb26aff0759a54c27a7`.
Compose replacement succeeded after removing the disposable Donsetch sidecar;
the sidecar was recreated against the new server namespace. `/healthz` returns
`ok`, and an approved Donsetch read still returns the synthetic catalog.

`/tmp/aster-skill-management-local.log` records `SKILL_MANAGEMENT_LOCAL_OK`:
root/default reference paths, version switching, delete cancellation, confirmed
deletion of a uniquely named verification fixture, and historical access denial.
Existing user skills were not deleted or edited. The
[current screenshot gallery](openwebui-improvements/index.html) includes the
asset editor, inline versions, delete confirmation and dark theme. Final image
build/deployment receipts are `/tmp/aster-skill-management-image-final.log` and
`/tmp/aster-skill-management-deploy.log`.

Rollback artifacts remain in the local deployment directory:
`before-skill-management.dump` (mode 0600),
`compose.before-skill-management.yaml`, and image tag
`localhost/aster-server:pre-skill-management`. Restoring code alone does not
restore deleted version content. All required gates are met; no commits,
pushes, cluster changes or external tool mutations occurred.

The subsequent [file navigator refinement](file-navigator-rv.md) replaces the
flat path list and numbered labels with nested folders, compact file details,
and selection preservation. Earlier asset screenshots remain historical receipts.
