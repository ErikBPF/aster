# Merged catalog review and deployment

Human seed: “check our recent catalog feature on aster and lets build up on top of it. Do a /rv and do general design improvements”

Correction: “do review and improvements on riht one then deploy”

## Outcome and baseline

This review targets merged PR #3 at `d31ca27`, not stale local main `2f2304a`.
Local and Apollo checkouts use `worktrees/catalog-current-rv` on
`fix/catalog-current-rv`. The prior catalog-design-rv experiment is preserved
but excluded. Builds, tests and image construction run on Apollo.

## Verified findings and scope

Independent review found table images sharing one namespace directory, asset
listing following directory symlinks, contract details matching by table name
instead of exact physical binding, and legacy namespace detail authorization
failing to normalize adapter names. UI review found disclosure controls unable
to collapse or retry failed discovery, plus stale detail/history responses
replacing a newer selection. Fix these boundaries and improve accessible
navigation without adding dependencies or changing query authorization.

Regression tests precede production fixes. Preserve the catalog tree, search,
details, Git-backed docs/history and permission presentation. Shared asset files
must not silently be assigned to a table: use entity-specific directories and
leave old files intact. Read-only inspection of Apollo's existing assets volume
found only a catalog README and no images requiring migration.

## Evidence and continuing work

Independent source reviews and final conformance/security review completed.
Verified findings were corrected; no blocker was found in the final patch. Tests execute inside the repository's declared devenv on Apollo.
A fresh remote worktree was created at the exact merged baseline; sync only
explicit changed source/test files, excluding credentials and local build state.

Deployment target confirmed: local Podman localhost:8081. Apollo is build/test
only; its Kubernetes deployment remains unchanged. Preserve existing
volumes and record prior image IDs before replacing a server. No commit/push
is implied. Record final source hashes, image identity, rollout/health and browser
results before reporting deployment complete.

## Regression evidence

Apollo browser runner observed seven assertion failures before fixes, then 7/7
passed. Legacy namespace details failed with 403, then all four generic catalog
integration tests passed. Contract collision returned admitted decoy instead of
the exact bound sales contract; the regression passed after correction. Asset
isolation/confinement tests observed two failures, then 4/4 passed. Independent
review requested one additional table/schema asset-path collision regression;
its equal-path assertion reproduced the issue, and the final layout uses a
dedicated table-assets root. Final asset suite passed 5/5; the complete CI gate also passed.

Known limits: catalog search caches its index for the page lifetime; existing
unsaved-document navigation behavior is unchanged. Existing document hierarchy
can collide with specially named nested schemas; redesign/migration of document
paths is outside this bounded patch. The target Compose stack has Docs disabled.
Prior local server/controller image ID:
`444466ee1d3b979604ebdeab1a23712a647dc03dcecb48a660abc7403db61f02`.

## Final verification and deployment

All build and automated test execution ran on Apollo.
`CARGO_BUILD_JOBS=8 devenv --no-tui shell -- just ci` passed formatting, Clippy,
workspace tests, features, repository/provider contracts and context checks.
`just catalog-navigation-browser` passed all seven flows on the final source.
Local and remote SHA-256 verification matched 414 source files; manifest receipt
is `/tmp/aster-current-source.sha256`. CI and browser receipts are
`/tmp/aster-current-ci.log` and `/tmp/aster-current-browser-final.log`.

`just image-build aster-server:catalog-current-rv` produced image
`b5fdf2103a837570a9d6c1674f43dc676ddb9589aab190d89188546d7ca68f9e`.
The image was transferred from Apollo and deployed locally with Compose project
`catalog-design-rv`, ports 8081/9091, `--no-build --pull never`. Existing database,
state and notebook volumes were retained. Both server and controller run the
verified image; all five services are running and PostgreSQL/Valkey are healthy.
`/healthz` returned `ok`; authenticated catalog HTML has the new tree/search/detail
workspace. Live HTTP checks traversed mock-local, aster_demo and orders, whose
detail returned five columns. Docs remain disabled by the existing local config.
Deployment receipt: `/tmp/aster-current-deploy.log`.

A live browser against the local instance was not run: Apollo SSH explicitly
disables port forwarding. No policy was changed. The final fixture browser suite
and CI's existing browser flows passed; these do not establish deployed visual
layout. No Kubernetes deployment, commit or push was performed.

Rollback image is retained as `localhost/aster-server:pre-catalog-current-rv`.
Retag it as `docker.io/library/aster-server:local`, then rerun the same Compose
project from this checkout with ports 8081/9091 and `--no-build --pull never`.
Do not remove volumes. The older catalog-design-rv worktree remains a superseded
experiment; the deliverable is this checkout based on merged PR #3.
