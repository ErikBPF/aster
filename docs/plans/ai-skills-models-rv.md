# AI registry review, validation and local deployment

## Outcome and accepted behavior

The reviewed AI registry is deployed at `http://localhost:8081/settings/llm`.
The delivery checkout is `worktrees/ai-registry-plan`, branch
`feat/ai-registry-plan`, based on merged catalog commit `d31ca27` plus the
previously reviewed catalog fixes. The original catalog checkout is preserved.
No commit, push, Kubernetes deployment or external identity configuration occurred.

The [PL record](ai-skills-models-pl.md) preserves the human seed verbatim and the
accepted decisions; the [implementation plan](ai-skills-models-ip.md) records its
boundaries. Skills contain immutable instructions/reference-file versions,
owner-only editing, team sharing, and administrator organization publication.
Conversation selections pin exact versions, require explicit upgrades, and
recheck historical dependencies before model calls. Bundled code is never executed.
Personal model registration/use requires administrator-granted subject/team
permission and current role/membership. Owners retain deletion after revocation.
Company model ACLs remain independent. Credentials are encrypted in PostgreSQL;
legacy-token migration is transactional and has no plaintext fallback.

The user answered the endpoint question with “use our litellm”
(`call_d11ab7276962475db37fe68bed18d8a3`). Owning workload configuration identifies
`https://litellm.homelab.pastelariadev.com/v1`; its liveliness endpoint returned 200.
That origin is the sole approved personal/company model destination. Separate
application encryption keys and the Compose override are retained privately under
`~/.config/aster/localhost-8081/`, with mode-0600 files. No model token or grant
was invented or prefilled.

## Verified review fixes

Independent security, correctness, conformance and UI passes corrected stale-role
model use/publication, repeated identity lookups, version-history request fan-out,
and delayed reads overwriting new drafts. Final validation corrected timestamp
units and NUL-title validation. Enabling independent AI authority now checks fresh
workspace membership without requiring an unrelated Git-target registry. When a
Git registry is configured, its denial cannot fall back to local membership.

Older isolation fixtures now supply verified sessions, current authority and
approved fixture endpoints. Their isolation assertions remain intact. A generic
“private” fixture identifier collided with new UI privacy text; a unique notebook
identifier preserves the disclosure assertion. A claimed development-login
incompatibility was discarded after verifying the actual cookie implementation.

## Observed validation

All compilation and automated tests ran on Apollo inside declared devenv.
`just ci` passed formatting, Clippy, workspace tests, bound existing scenarios,
feature/repository/provider checks and local-context checks. Receipt:
`/tmp/aster-ai-ci-delivery.log`. Local/Apollo hashes match 433 source files;
manifest: `/tmp/aster-ai-source.sha256`.

Nine AI browser fixture flows passed, including immutable selection, explicit
upgrade/clear and stale-response regressions (`/tmp/aster-ai-new-draft-green.log`).
The real-server Chromium/PostgreSQL smoke passed registration, sharing/revocation,
write-only credentials, encryption and restart persistence
(`/tmp/aster-ai-live-browser.log`). Three disposable PostgreSQL tests passed Git
policy precedence, interrupted migration/restart recovery and skill persistence
(`/tmp/aster-personal-precedence-green.log`). No real model inference was requested.
The new Gherkin contract remains explicitly unbound; direct API/browser tests
provide its recorded automation. [Skill](ai-skills-backend-evidence.md) and
[model](ai-personal-models-implementation.md) records preserve RED/GREEN receipts
and sequencing limits without treating compilation failures as RED.

Disk exhaustion initially interrupted linking/image packaging. Build cache,
scratch and an isolated image store moved to Apollo's larger data disk; the old
cache path remains a symlink. No existing workload or volume was removed.

## Rollout, limits and recovery

Server/controller now run image
`eede1994d6168cd16cfea3e1bfdfca6bed46b5ff85acb05d61bd441c5c0c33bc`.
Compose retained project `catalog-design-rv`, ports 8081/9091 and existing volumes.
Local HTTP checks passed health, AI HTML, skills RPC, default-deny personal policy,
admin registry access and ordinary-user denial. Rollout receipt:
`/tmp/aster-ai-deploy.log`; final checks: `/tmp/aster-ai-local-smoke.log`.

This local stack still uses development login. Team sharing/company-model
execution need configured SSO/team authority; company registry management is
available. Production identity revocation and real LiteLLM inference were not
claimed. Live browser validation used the isolated Apollo server, not localhost.

Rollback image `localhost/aster-server:pre-ai-registry` and the pre-rollout database
backup under the private runtime directory are retained. After tokens are stored,
recovery requires compatible code plus retained keys, or the prior database
backup. Never run old plaintext writers alongside encrypted storage or restore
plaintext as a rollback shortcut.
