# ODCS-first AI and catalog: RV

> **Historical evidence after sanitization:** all deployment, publication and
> verification claims below describe the original source and environment. History
> has since been rewritten locally, including runtime configuration identifiers;
> earlier statements that history was untouched or changes were documentation-only
> do not describe this candidate. Placeholder targets have not been deployed.
> Candidate validation must be recorded separately; old commit/image references
> remain original evidence identifiers, not rewritten-source verification.

## Public publication preparation, 2026-10-09

The user authorized publishing this repository and its accumulated changes, then
explicitly selected sanitization of company-derived discovery notes. Private
originals are retained outside the repository; public notes keep generic lessons
and accepted Aster decisions without company-specific observations or source
links. Existing history was not rewritten: it contains generic project/documentation
references, but not the newly removed company-specific findings.

Gitleaks v8.30.1 scanned all reachable history and the final publication tree.
Both scans reported only the two PEM delimiter strings in
`crates/server/src/github_app.rs`, independently inspected as parser code rather
than embedded keys. No actual credentials were identified. Redacted scan reports
and checksum-verified scanner receipts remain in Build-host's private publication
audit directory. GitHub had no Actions runs to expose. Publication changes affect
documentation and one comment only; the prior full CI and live-demo evidence
still describe runtime source. Final `just features repo-check providers-check`
and `git diff --cached --check` passed. The approved target is `ErikBPF/aster`,
default branch `main`; no force push or history rewrite is part of publication.

## R24 delivered: real benchmark notebooks, 2026-10-09

**Outcome / why:** the user's accepted notebook, two-catalog and richer-contract
demo is deployed on Build-host, context `aster-demo`, namespace/release
`aster`, **Helm revision 8**. Fresh real Authentik login now lands on usable local
notebooks. All four agreed behavior groups were exercised by executable checks:
notebook lifecycle, selected-catalog queries, complete metadata/contracts, and
access controls. Met: 4; unmet: 0; abandoned: 0 within r24's accepted scope. The
Gherkin file remains explicitly unbound; these are router/protocol/browser checks,
not a claim that draft Gherkin steps ran.

**Access:** open `http://localhost:8080/` as `alice`, then notebook
`benchmark-tour-1791568807`, or use
`http://localhost:8080/notebooks/benchmark-tour-1791568807`. Catalog inventory is
`http://localhost:8080/catalog`. Existing demo credentials and the private Neovim
retrieval command below remain valid. Local health returned `ok`, login returned
303, and OIDC discovery matched the configured localhost issuer. Existing
forwarding loops recovered stale connections after pod replacement; final checks
passed without retries. All five Aster/backend pods were ready, with zero restarts
on the current server/controller/Trino pods.

**Changes:** explicit local notebook mode preserves contract-team/current-identity
policy. Owner admission covers browser/REST/RPC and notebook-related helpers,
conversations, queries and exchanges. Cell catalog/schema metadata survives Git
save/reload. A bounded native Trino catalog adapter now exposes separate `tpch`
and `tpcds` inventories. Live metadata supplied **8 TPC-H + 25 TPC-DS tables**,
including `dbgen_version`; all **33** have individual pinned synthetic ODCS
contracts. The UI renders populated nested fields, including false/zero values,
with exact original source/provenance retained and declared facts distinguished
from observations. Checked-in overlay/recipes preserve existing identity Secrets,
notebook PVC and PostgreSQL ownership; mutation recipes enforce the named context.

**Observed evidence:** `ci-complete.log` ends `CI GREEN`; its bound suites pass
31 core and 90 server scenarios. `focused-final.log` reports
`BENCHMARK_NOTEBOOKS_OK`, `TRINO_CATALOG_OK`, and `BENCHMARK_UI_OK` (the latter is
fixture HTTP, explicitly not live proof). `chart-lint.log` validates 14 base
resources; `benchmark-render.log` validates 15 with the overlay. Isolated Compose
passed and cleaned up its own resources. `trino-benchmark-live.log` verifies all
native tables and columns; `bundle-intake.log` validates all 33 contracts through
runtime intake. Existing ignored tests remain ignored; the named live checks were
executed separately.

Both `live-notebooks-green.log` and `live-notebooks-restart.log` end
`BENCHMARK_DEMO_E2E_OK`. Actual browser creation, edit, Save, reload, scoped engine
selection and unqualified SQL worked for both catalogs. Independent direct Trino
queries matched notebook results: `tpch.tiny.nation` count **25** and
`tpcds.tiny.call_center` count **2**. Every table returned a bounded real-row probe;
all original contract sources/digests matched. Viewer read/write/query denial,
unchanged owner content, and fresh membership revocation/restoration passed.
After restarting the server, the same notebook retained SQL, selected catalog,
schema and ownership. Screenshots were visually checked. Receipts and screenshots
live under `build-host:/home/developer/aster/worktrees/benchmark-demo-20261009/evidence/`.

**Review / recovery / limits:** two independent reviews found and closed the UI
discovery-save race, exact-target guard gap and exchange ownership-transfer race,
with assertion RED/GREEN evidence retained below. CI fixture repairs preserved
authorization assertions; serial browser/HTTP scenarios retained the original
three-second deadline. One malformed E2E denial probe was corrected to submit a
valid notebook plus ETag, then prove denial and unchanged content. Deployed digest:
`sha256:63a1169f1c0a591ba594c459e0be61fbb64f254eb1325735b84fef332d9c910e`.
Rollback is Helm revision **7**, not the older insecure revision 6; preserve
database/PVC contents. Test/receipt edits after the image build do not change its
runtime source. No commits or pushes. Full team/GitHub activation, real AI,
semantic-sidecar execution and SQL-target contract enforcement remain deliberately
outside this synthetic-only demo. Certificate expiry remains November 8.

## R24 discovery and repair log (historical)

Accepted user seed and decisions are preserved in canonical r24 and its IP
section. The fresh plan grill found local notebook read-isolation gaps and
required ownership checks across transports before activation. The current
reviewed demo remains the rollback target (Helm revision 7); candidate source is
`build-host:/home/developer/aster/worktrees/benchmark-demo-20261009/source`.

**PL/IP outcome:** native `tpch`/`tpcds`, tiny scale, all benchmark tables,
per-table synthetic ODCS contracts, persistent owner-isolated local notebooks,
structured whole-document display and actual browser lifecycle checks. Team
GitHub activation, real AI and SQL-target contract enforcement remain out of scope.
No ownership or contract checks may be removed to make the demo work.

**RED/GREEN evidence so far:** `red-benchmark-bundle.log` fails the existing mock
bundle with `AssertionError: both real benchmark catalogs required`. UI fixture
test `benchmark-ui-red.log` failed missing structured contract details;
`benchmark-ui-green.log` subsequently reports `BENCHMARK_UI_OK`, covering literal
malicious text, nested/zero/false values, revocation clearing and selected catalog
request context after simulated persistence. This frontend check uses fixture
HTTP responses; backend persistence and live Trino remain unproven until E2E.

**Toolchain correction:** fresh-snapshot devenv evaluation could not obtain its
patched nixpkgs path from the Cache-host cache. No assertion RED is claimed for that
infrastructure failure. Tests use the existing October-2 declared devenv with
the same unchanged toolchain files, then change directory to candidate source.
No host-global toolchain was installed. Builds/tests remain Build-host-only.

**Live inventory correction:** Trino 483 exposes 8 TPC-H and 25 TPC-DS tiny
tables, including `dbgen_version`: 33 total. The initial 32-table estimate was
wrong; acceptance remains all tables, so checks now include the observed extra
table. Native `time(3)` columns also required the ODCS `time` mapping. Neither
finding permits dropping tables/columns. Trino's first startup rejected a hyphen
in `node.environment`; source was corrected to `aster_demo`, then rollout passed.

**Continuing:** notebook activation/isolation, Trino metadata adapter, generated
bundle validation, independent source review and full CI/Compose/chart checks;
then scoped deployment and real login/create/save/reload/query/restart proof.
Latest frontend compatibility review is also checking legacy single-catalog
behavior. No candidate runtime success is claimed here.

**Additional observed checks:** `trino-benchmark-live.log` reports one executed
live adapter check, zero ignored/failed, covering every table and column in both
native tiny catalogs. `bundle-intake.log` passes Python inventory/hash checks and
Rust runtime ODCS intake for all 33 artifacts. `red-live-notebooks.log` reaches
real Authentik login and then assertion-fails the actual notebook landing page;
this is the missing browser regression, not an infrastructure failure.

Independent source review `ses_ede5bf583ffeOMWoYKdb6UsV3z` found a saved-context
race during asynchronous catalog discovery and missing exact-target guards in
mutation recipes. UI repair is underway. Context guards now reject every target
except `aster-demo`; `context-red.log` captured the assertion before the
fix, `context-green.log` passes both refusal and stubbed authorized positive
controls for backend apply and Helm upgrade. Trino protocol tests and scoped
Clippy passed; UI single-catalog compatibility and legacy team/conversation
browser checks passed. These do not replace final integration/deployment gates.

**Notebook candidate evidence:** explicit local mode retains contract membership;
owner admission now covers local browser/REST/RPC lists/reads and notebook-related
queries, conversations and helpers. Cell metadata survives Git text and legacy
RPC updates. Receipts: `red-notebooks.log`, `red-notebooks-metadata.log`,
`red-notebooks-rpc-metadata.log`; `green-notebooks-final.log` (16 passed),
`green-notebooks-metadata.log` (6 passed), `green-notebooks-postgres.log`
(one executed durable PostgreSQL/Git reopen test), and `green-notebooks-boot.log`
(one executed local-mode/fresh-membership boot check).

Fresh backend review `ses_ede54731dffe2OcSiGOvpmtAtd` found a blocking exchange
ownership-transfer race: admission followed by a second unchecked Git read could
return a new owner's content. Repair and deterministic regression are in progress.
The candidate image build was deliberately cancelled before rollout. Initial full
CI stopped at formatting in the new bundle test; that formatting is corrected.
Chart lint passed 14 resources. UI discovery-race RED/GREEN is recorded in
`benchmark-ui-discovery-{red,green}.log`; saved context survives slow/failed
discovery while unresolved execution stays blocked.

**Blocking review repair closed:** `exchange.rs` now reads one ownership-locked
snapshot for FetchQuery and reuses the admitted document for FetchSummary.
Deterministic concurrent admin-transfer/Bob-save checks failed with Bob's private
SQL before the fix (`red-notebooks-transfer-race.log`), then all 18 ownership
tests passed (`green-notebooks-transfer-race.log`). The original independent
reviewer re-read source and tests and closed P1. Final CI, isolated Compose and
fresh image build are running; no application rollout has occurred yet.

**Compose gate:** `just odcs-compose-smoke s1` passed on Build-host using exact
devenv-built server/controller binaries, disposable PostgreSQL/Valkey/Keycloak,
and isolated project `aster-odcs-s1-67f434928024`. The runner removed its own
containers, network and volumes. Receipt: `compose-final.log`, ending
`odcs-compose-smoke s1 OK`. This proves the base Compose path, not live Trino E2E.

**Candidate image:** `just build-minikube` completed on Build-host and imported
`sha256:63a1169f1c0a591ba594c459e0be61fbb64f254eb1325735b84fef332d9c910e`
into the isolated profile; a digest-qualified alias is retained. Build receipt:
`build-image-final.log`. The 349-file candidate archive is
`71c31de157c78a9c45d79297f01f4ee6ae7eb82da4cfcfda2497da4a11e6cb9b`.
Parity check found only a later receipt-document difference, which was synced;
runtime source matched. Final CI must finish before Helm upgrade.

**Full-CI finding:** the first complete run reached two `shared_model_access`
failures (expected 200, observed 403) after ownership admission tightened. The
fixture ownership assumptions are being traced; authorization assertions remain
unchanged and rollout stays gated. `ci-final.log` preserves the failure. Separate
candidate-overlay validation passed all 15 rendered resources in
`benchmark-render.log`; live Helm history still shows revision 7 deployed.

**CI reconciliation:** shared-model access fixtures now explicitly establish
Alice's ownership and a real snapshot; all six original assertions pass, without
runtime changes. Full runs then reproduced the existing S6 three-second bound
failure under concurrent Cucumber scenarios; isolated S6 passed. Serializing this
host-sharing browser/HTTP harness retained the deadline assertion and passed all
90 scenarios/300 steps (`ci-serial.log`). That run exposed one additional legacy
`shared_models` fixture returning ownership 403 before its expected model 404;
its setup is being corrected without changing the expected authorization result.

## Build-host demo access and readiness, 2026-10-09

**Human seed:** “lets check our aster app, validate if roadmap is ready, deploy it
on a minikube cluster in build-host and give me access”; follow-up: “check if demos
is using latest version”. Asked about the impending demo certificate expiry,
the user selected “Extend 30 days (Recommended)”.

**Observed outcome:** the existing `aster-demo` deployment was already
healthy; reuse avoids replacing the reviewed demo with the older `main` baseline.
Renewed its isolated CA and leaf in Kubernetes Secret
`aster/aster-authentik-demo-tls`, then restarted only `deployment/aster-server`.
The final certificate expires **2026-11-08 12:27:57 UTC**. Source image remains
`sha256:9738ba8c1c641dde3fa5bbaf0707551644427fc64352689bf3544ddfd0f12b44`.
No application source, standing cluster, or production identity was changed.

**Evidence and repairs:** issuance reused the existing evidence recipe's
subjects, SANs and leaf extensions, with both lifetimes changed to 30 days.
BusyBox `wget` rejected `--ca-certificate`; a Python strict TLS check then found
the recipe's CA lacked an explicit key-usage extension. Reissued with critical
`basicConstraints=CA:TRUE` and `keyUsage=keyCertSign,cRLSign`; strict OpenSSL
verification and Python HTTPS verification of the actual served chain, localhost
hostname and identity readiness endpoint passed. Temporary private-key material
was mode-restricted and removed. No verification bypass was used.

Build-host's existing `aster-authentik-browser.py` completed successfully after the
rollout: real Alice/viewer login, callback binding/replay controls, five exact
contract sources, inventory, catalog UI, canned mock query, viewer denial and
same-session membership revocation/restoration. Its existing `browser-result.json`
and screenshots were refreshed. These are live demo checks, not evidence of real
backend/AI support or authoritative SQL/share target enforcement.

**Access:** local tmux socket `aster-odcs`, session `tunnel`, forwards loopback
8080/8081 to Build-host 18080/18081. After the remote forwarding loops recovered from
pod replacement, local `/healthz` returned `ok` and OIDC discovery matched
`http://localhost:8081/application/o/aster/`. Open
`http://localhost:8080/login`; use the existing interactive credential helper
below for `alice`. Secrets were not exported to chat or files. Access depends on
the SSH tunnel; certificate renewal remains manual.

**Source freshness:** `git ls-remote` confirms upstream `HEAD` and `main` equal
local main/task HEAD `eb397d865cf2dc156fda172d50b13c98eda9d9f7`. All 296 deployed
snapshot files match the recorded manifest; current task worktree has zero
executable-source, configuration, chart or lockfile differences. Only three docs
and the browser-binding feature's coverage-label comment differ. The demo already
contains the latest local implementation, including uncommitted work absent from
upstream main; replacing it with main would remove functionality. Its generic
chart/app version labels alone cannot prove freshness. Separate standing profile
`aster` has an unreachable API (`connection reset by peer`); its app version is
unverified and it was not modified.

**Final validation:** `devenv --no-tui shell -- just ci` passed on Build-host:
`CI GREEN (fmt + clippy + tests + features + repo + providers)`. Receipt:
`build-host:/home/developer/aster/worktrees/odcs-oidc-binding-20261002/evidence/ci-20261009.log`.
The first invocation exceeded the harness's 120-second timeout during tests;
its processes subsequently exited. A fresh invocation with a 600-second limit
completed successfully, without source changes. Local `git diff --check` passed.

**Remaining work:** roadmap remains demo-ready, with query/share target authorization, global activation,
semantic-sidecar ingestion and real AI integration still deferred as documented
below. This operation does not close those gaps or merge the uncommitted work.

**User-reported access gap:** after login the browser showed
`{"error":"team notebook context required"}`. Source confirms the successful
callback redirects to `/` (`web.rs`, `redeem_callback`), while `index` rejects
that route whenever team workspaces are configured. The usable demo landing page
is `http://localhost:8080/catalog` in the authenticated browser. Existing browser
E2E explicitly navigates to `/catalog` after checking the session; it does not
assert that the default post-login landing page is usable. Its success therefore
does not establish a working end-to-end first-login navigation experience.
Fixing the default landing route remains open; notebook authorization must not
be bypassed to hide this navigation gap.

## Roadmap continuation review, 2026-10-04

**Outcome:** two independent source reviews found no blocking defect in the
browser-bound OIDC correction. This closes its outstanding independent source
review, not delivery to `main`, a new deployment approval or platform-wide
security signoff. The correction and accepted ODCS implementation remain
uncommitted on `docs/odcs-ai-catalog-plan`, based on `eb397d8`.

**Human seed:** “check our aster roadmap and lets evaluate current status and next
steps”, followed by “continue”. The continuation starts with the recommended
security review and evidence reconciliation. Existing r23 acceptance and deferred
query/share, activation and semantic scope remain unchanged.

**Review evidence:** security/adversarial reviewer
`ses_ef719cf4effeoiRCRQZP7p9uq6` and correctness/conformance/test reviewer
`ses_ef719b35dffeK2uDH4RZ88hdwS` independently traced the callback, session and
test paths. The reviewed `web.rs` SHA-256 is
`98636b1137755f78d1d73b3725db9ad57981d378be6ce80e469a483127338468`;
`ai_registration.rs` is
`25b21360448119f0564c34dbb763e48bb0e1231c6fc64589ceb1be406008db21`.
Pre-review receipt SHA-256:
`485f76d22f4829e96cebd665baa1dc4ba2fb024f7be3059b57531fe5a8d0a1ef`.
The correctness reviewer also reviewed this continuation record and roadmap
wording; its one precision correction is applied: query/share policy is accepted,
but its enforcement seam and sharing design must be resolved before coding.

**Dispositions:**

- Verified documentation overclaim corrected below: cleanup covers a matching
  callback missing its code, not every malformed query. Axum deserialization can
  reject duplicate fields before the handler runs.
- One pending binding cookie means overlapping browser logins compete: the last
  start replaces the earlier binding, and an in-flight completion can clear a
  newer binding. Concurrent-login behavior and store-failure paths need focused
  regression coverage; reviewers found no demonstrated callback bypass there.
- Separate pre-existing security follow-up: HTTPS sessions still use unprefixed
  `aster_session`, and the reader accepts the first matching cookie
  (`web.rs` session writer; `lib.rs::principal_with_session` and `cookie`). An
  attacker with a valid session and control of a sibling subdomain can inject a
  parent-domain session cookie into a victim browser. The proposed follow-up is
  consistent `__Host-` session naming for HTTPS, duplicate refusal and explicit
  legacy-cookie migration. Source establishes the risk; no browser exploit was
  run. This review does not silently expand the callback fix into that change.

**Verification:** local `git diff --check` passed. Read-only checksum comparison
against the existing Build-host snapshot found identical executable source; only the
draft feature label and two handbook/receipt files differed. Remote-only chart
dependency artifacts were also identified by dry run; nothing was deleted or
synced. Fresh Build-host `devenv shell -- just ai-registration-validation` passed
(5 AI unit tests, 12 integration tests and 6 LLM unit tests); then
`devenv shell -- just ci` completed with
`CI GREEN (fmt + clippy + tests + features + repo + providers)`, both from the
existing snapshot source directory. Full command output is available through
`rtk recall a2921c8bb7cc`, SHA-256
`316c8674725a2127f41243fcb38ef5f8ecab6533503c9a10b6f66f772e1e6396`.
These runs validate unchanged executable source, not the later local prose edits.
Historical RED, Compose and real-browser receipts below remain historical;
no new live IdP, browser or deployed-state proof is claimed.

**Next/recovery:** fresh validation passed; package reviewed work for
explicitly authorized publication. Track session-cookie hardening alongside that
delivery. The next product priority remains authoritative query/share dependency
admission; resolve its enforcement seam and sharing design before implementation.
Explicit no-bundle migration and scoped live integration proofs follow. Never use
revision 6 as a secure rollback: retain browser
binding on every replica, or disable login/callback routes until a fixed build is
restored. No code, deployment, credentials or acceptance changed in this review.

## P1 browser-bound OIDC fix, 2026-10-02 — deployed and verified

**Review input:** independent RV `ses_f00ce6913ffeKCEay3S5wVtDtG` found that a
valid callback state could be redeemed by a different browser. The earlier
first-test readiness claim was withdrawn pending this fix. This receipt records
the implemented correction and observed tests; **no new independent signoff**.

### Fix and observed RED/GREEN

The complete path was traced: `IdentityProvider::begin` → `web::login` → shared
`HandshakeStore::put` → `web::callback` → atomic `take` → provider exchange →
verified session. The only callback route and the existing manually seeded
`ai_registration` test were corrected at the shared entrypoint.

- `/login` binds the existing unpredictable per-attempt OIDC state to a host-only,
  HttpOnly, SameSite=Lax cookie, Max-Age **300 seconds**. Shorter configured store
  expiry still wins. No PKCE verifier or nonce is moved into the cookie.
- `/callback` requires exactly one matching cookie **before calling `take`**.
  Missing, wrong, duplicate or foreign-state callbacks cannot consume a valid
  pending handshake or clear a different pending browser login.
- Matching success, refusal, failed exchange, missing-code completion or expired
  handshake clears the cookie with matching Path=/ and security attributes.
  Atomic store redemption, PKCE and ID-token nonce/signature validation remain.
- The trusted provider callback URI controls Secure. HTTPS/unknown configuration
  uses `__Host-aster_oidc_state`, Secure, Path=/ and no Domain; explicit HTTP uses
  `aster_oidc_state`. Forwarded/Host request headers cannot downgrade this policy.
  Session creation and logout clearing use the same configured transport policy.
- Existing verified-identity tests now start `/login` and carry its binding cookie;
  no pre-seeded unbound callback exception remains.

Build-host `oidc-red-separate.log` observed **both missing and wrong binding return
303 instead of 403**, plus the absent-cookie assertion. After the fix, all **12
`ai_registration` tests passed**, including zero provider exchanges and zero store
takes for foreign callbacks, subsequent initiating-browser success, replay denial,
expiry/refusal/exchange-failure cleanup, duplicate cookies and spoofed forwarding
headers. `just ci`, chart validation, image build, isolated `just compose-up`
health and `just odcs-mock-fixture-validation` also passed on Build-host.

### Current deployment and real browser proof

Only context/profile `aster-demo` was updated: Aster release `aster`,
namespace `aster`, **revision 7**, chart `aster-0.1.0`. Authentik remains isolated
at chart/image **2026.5.6**, release revision **2**. All current pods are ready;
server/controller have zero restarts. Standing `aster` and `build-host-langfuse`
containers and the default context remain unchanged.

Fresh Build-host Chromium sessions for Alice and viewer exercised real Authentik
authorization-code login. Before entering credentials, each live flow probes a
pending state's callback with a non-authorization test code from missing/wrong
cookie contexts: both return 403 and the actual Valkey handshake payload remains
byte-for-byte unchanged. The initiating browser then completes the genuine
provider exchange; its HttpOnly/Lax cookie is cleared. Replaying its actual
callback with the original binding returns 403. Callback codes/cookies remain
in memory and are excluded from receipts/log output.

The same browser run verifies five contract details and exact original sources,
all owner inventories, orders context/manual review, permitted canned mock query,
viewer denial, and Alice membership removal → same-session denial/UI clearing →
restoration to five contracts. `browser-binding-final.log` and
`browser-result.json` contain the results; screenshots were visually inspected.
No real AI helper, semantics-sidecar ingestion or SQL/share target enforcement
is claimed.

**Local reachability:** `2026-10-02T22:23:28-03:00`, from the user's machine:
`http://localhost:8080/healthz` **200**, `/login` **303**, official Authentik
discovery on port 8081 fetched and exact issuer/authorize/token/JWKS endpoints
validated. Existing persistent forwarding loops recovered stale connections
after pod replacement; the final check passed without retries. Receipt:
`oidc-local-reachability.json`. App: **http://localhost:8080/login**;
catalog: **http://localhost:8080/catalog**.

### Effective reader privileges and credential correction

The deployed Secret's actual bearer credential was authenticated through
Authentik's native `TokenAuthentication`, then its effective model was inspected
read-only in the isolated Authentik container, including inherited memberships,
roles, global permissions and object grants. The final result is:

- principal `svc-aster-authentik-membership-reader`, active, **not superuser**;
- no direct or inherited groups; exactly one direct/effective role,
  `aster-identity-reader`;
- exactly `authentik_core.view_group` and `authentik_core.view_user`;
- **zero object-level grants**; deployed token resolves to this exact principal.

This is stronger evidence than one denied write. The audit also found the old
credential no longer authenticated. Deployed 2026.5.6 source explains why:
`TokenSerializer.validate` replaces requested API-token expiry with its default
30-minute interval, and `Token.expire_action` rotates expired API keys. The prior
receipt's requested seven-day expiry was not effective. The supported standing
demo API token is now explicitly **non-expiring**, confined to this read-only
identity and the existing Kubernetes Secret; revoke it when retiring the demo.
The demo TLS certificate still has its original seven-day lifetime.

An attempted expiry-only PATCH also exposed Authentik's default-to-requester
ownership behavior; validation caught the wrong principal before the new rollout.
That intermediate token was deleted. The replacement was explicitly assigned to
the reader, and subsequent PATCH includes its owner and intent. Final native
authentication/effective-permission assertions passed before and after rollout.
No production identity was changed. Evidence: `oidc-token-source.py`,
`oidc-refresh-reader.py`, `reader-effective-privileges.json`,
`reader-audit-supported.log`, `reader-audit-post-rollout.log`.

**Credential access:** the consumed local `aster-authentik-demo.secrets.json`
handoff was removed after checking that the sanctioned Secret retains `alice`
and `viewer`. No replacement file was exported. An authorized user can open one
account's existing Secret value directly into a plugin-free, read-only Neovim
buffer on Build-host, with swap/backups/history/logging disabled and no saved file:

```bash
ssh -t build-host 'bash /home/developer/aster/worktrees/odcs-oidc-binding-20261002/evidence/oidc-demo-credential.sh alice'
```

Use `viewer` instead for the negative control; close with `:q!`. The helper is
syntax-checked and refuses noninteractive execution before reading any Secret.
Values are not printed in chat or support logs. The runtime store remains Secret
`aster-authentik-demo-login` in namespace `aster`, task context only.

### Source pins, evidence and rollback

Evidence and exact source: `build-host:/home/developer/aster/worktrees/odcs-oidc-binding-20261002/{source,evidence}`.
The **296-file** snapshot matches its manifest with zero mismatches after the
build and deployment. Final handbook/receipt edits and the Gherkin coverage-label
clarification are later; executable source is unchanged. The `.feature` is a
draft Gherkin description, with actual executable router tests rather than a
Gherkin step runner. Local checkout
is still `docs/odcs-ai-catalog-plan` at base `eb397d865cf2dc156fda172d50b13c98eda9d9f7`
plus uncommitted work. No commits, pushes, nested agents or local builds/tests.

| Pin/evidence | SHA-256 |
|---|---|
| Source archive | `6cc8ae27328b7628db013ebf9daf54f6f092e9e3c011c68c206645ed2218cf55` |
| Source manifest | `1726b2e62f7b99abf012ab2ab0d41b15595db5dea45266bea414748f236f7491` |
| Aster server/controller image | `9738ba8c1c641dde3fa5bbaf0707551644427fc64352689bf3544ddfd0f12b44` |
| RED assertions | `389cd612bf4c1eb214fb53189b7ba868a70fa1c4de51a5e62fa948ecd7e9e9ea` |
| Focused GREEN | `25d2910e60c69676602fdeace9bfc81aaa4fc2789a657d8189e46a845f3b2447` |
| Full CI | `7fa8f7ea98137eea8dbe96e6071cea272d3457132bee8b4f8742a205fac7dc2b` |
| Browser proof | `ab2874a402c2aee63f0797b87f683fea66e8c4e141ef76428c22e4ac7fd98f4a` |
| Effective privilege proof | `2e151424b24934fa8748906604f720079b05de6ac37718b149aec25b828e0d5e` |

`runtime-verification.json` records exact running image/binary/mount digests,
Helm revisions and all other gate hashes. Operational rollback is prepared to
**revision 6**, image `sha256:a085f037a1767b2ef13f75b250dce6a2b4fca3fd7bf711062977239edec7f683`:

```bash
ssh build-host 'bash /home/developer/aster/worktrees/odcs-oidc-binding-20261002/evidence/oidc-binding-rollback.sh'
```

Syntax-checked, not executed. It reuses the current valid scoped Secret and
**reintroduces the reported login-CSRF defect**, so it is not a secure ready state.
Previous revision-3/revision-1 rollback artifacts remain intact and their image
aliases were checked. The current Build-host coordination snapshot's `just test`
still stops at `references/repos/aster/.git` (missing directory, trace recorded);
`just docs-check` still reports 14 pre-existing broken-link occurrences. These
coordination gates are **not green**; Aster's security/build/runtime gates are.

## Authentik first-test receipt, 2026-10-02 — usable catalog verified

**Historical pre-P1 receipt:** superseded by the browser-binding correction above.
Its original exported credential file has been removed and its requested API-token
expiry proved ineffective; use the current access and credential instructions.

**Human correction:** “why are we using keycloack as main. It should be authentik.
Change and do all necessary preparations dor a first test”. Authentik is the demo
identity platform. This supersedes the blocked Keycloak receipt below and its
proposed Keycloak fresh-membership adapter. Independent RV is the next gate;
this is execution evidence, not independent approval.

### Access and test scope

- App: **http://localhost:8080/login**; catalog: **http://localhost:8080/catalog**.
  Use a fresh/private browser session. Sign in as **alice** (owner/editor); **viewer**
  is the nonmember control. The original mode-0600 credential export was consumed
  and removed during the P1 correction. Kubernetes Secrets remain the runtime
  store; current interactive access is documented above.
- Authentik: **http://localhost:8081**. Both browser and Aster use the exact issuer
  `http://localhost:8081/application/o/aster/`, through a task-only forwarding proxy.
  OIDC signatures, issuer, audience, nonce and PKCE remain verified. The separate
  current-membership API uses `https://localhost:8443`, a proper CA-signed localhost
  server certificate and `ASTER_AUTHENTIK_CA_FILE`; no TLS verification bypass.
- Existing persistent task forwards remain: local tmux `-L aster-odcs`, session
  `tunnel`, ports 8080/8081 → Build-host 18080/18081 → the task Aster pod. Both public
  local endpoints were checked here after the rollout. Browser tests ran on Build-host.
- All five compiled contracts, bindings and table-specific grants are unchanged.
  Owner mappings cover the three exact namespaces `aster_demo`, `aster_demo.sales`,
  `aster_raw`; `aster-editors` resolves through the registered Authentik group UUID.
- `semantics.yaml` sidecars are **not ingested**. SQL/share target enforcement is
  still unimplemented. Mock query rows are canned, not measured business results.
  No real AI helper is configured: verified context preparation is manual, with
  declared meaning, exact selected object, provenance and mock-catalog observation.

### Runtime and implementation

Only context/profile `aster-demo` changed. Aster namespace/release `aster`
is **Helm revision 6**, chart `aster-0.1.0`. Authentik is an isolated installation
in namespace `aster-authentik`, release `authentik-demo`, **revision 2**, upstream
chart and image version **2026.5.6**. Production Authentik was not modified.
Authentik has its own PostgreSQL/PVC; the old Keycloak sidecar is absent from the
active Aster pod. The nginx sidecar only forwards to the real Authentik service.

`ASTER_TEAM_POLICY_FILE` enables the existing `TeamWorkspaces` registry with
distinct stable member/maintainer group UUIDs and the existing fresh Authentik
reader. It removes the unnecessary runtime dependency on GitHub credentials or
shared-model setup for catalog team registration. Invalid UUID mappings and
simultaneous team-Git registration are rejected; Git targets remain inactive.
The owning startup test was observed RED (missing fresh identity), then GREEN.
No fake identity adapter, injected verified session, dev-login seam, broad data
grant or dummy GitHub App identity was used.

Native Authentik API setup created a confidential authorization-code client,
strict callback, RSA signing key, standard scopes and `aster_identity` mapping:
`aster_user_uuid = str(request.user.uuid)` and real group names. Aster uses
`sub_mode=user_username` for the isolated existing `alice:mock-local` grant.
The service identity **svc-aster-authentik-membership-reader** has only
`authentik_core.view_user` and `authentik_core.view_group`, through its own role
and API token. Its write request returned 403. The original request specified
token expiry 2026-10-09, but the P1 audit above found Authentik ignored that value
for an expiring API token. The task TLS certificate lasts seven days.

### Observed verification

| Check, all builds/toolchain/tests on Build-host | Result |
|---|---|
| Exact source manifest | 295 files, zero mismatches after build/deployment |
| New catalog-team startup test | Assertion RED before wiring; GREEN after |
| `just ci` | PASS: formatting, Clippy, workspace tests, features, repo/provider contracts |
| `just chart-lint` / final overlay | PASS: 14 default / 15 overlaid resources; zero invalid/errors/skipped |
| `just build-minikube` / chart rollout | PASS: pinned image imported; both namespaces' current pods ready |
| `just compose-up` / health / `just odcs-mock-fixture-validation` | PASS in separate project `aster-authentik-regression-20261002`; only that test project's containers/volumes cleaned up |
| Real browser login | Authentik authorization-code/PKCE callback; Valkey readback `verified=true`, expected signed UUID and editor group |
| Contract list/detail | Five admitted contracts; every detail and original source matches the exact pin; mismatched table/digest returns 403 |
| Owner inventory | All three namespaces visible; actual mock tables and contract status rendered |
| Selected context | All five preparation APIs observe the mock schema; browser selects orders and reviews the matching manual SQL draft with provenance |
| Mock query | Permitted Alice request returns canned mock rows; no business measurement claim |
| Nonmember | Viewer has empty contract list, detail 403, denied/empty inventory and query 403 |
| Live revocation | Remove Alice's group; same session immediately lists zero, detail 403, UI clears document/preparation/review; restore membership → five contracts again |
| Read-token least capability | Real user/group GETs succeed; group creation denied 403 |

Earlier attempts and corrections remain in evidence: explicit Authentik
`authorization_code` grant enablement, nginx writable temp paths under the
nonroot pod identity, a proper CA/leaf certificate pair, and actual typed browser
password entry for Authentik's dynamic form. The successful browser receipt is
`browser-orders.log` / `browser-result.json`; screenshots were visually inspected.

Coordination checks were also attempted on an Build-host snapshot of platform's
`docs/aster-odcs-plan` worktree. `just test` is blocked by the existing
`references/repos/aster` target lacking a `.git` directory; `just docs-check`
reports the same 14 historical broken-link occurrences. These are not claimed
green or repaired as part of identity preparation. Logs: `coordination-test.log`,
`coordination-test-diagnostic.log`, `coordination-docs-check.log`.

Installation followed upstream [Kubernetes installation](https://docs.goauthentik.io/install-config/install/kubernetes/)
and [automated bootstrap](https://docs.goauthentik.io/install-config/automated-install/)
guidance, with setup fields checked against the deployed 2026.5.6 API schema.

### Immutable evidence, pins and rollback

Source/evidence: `build-host:/home/developer/aster/worktrees/odcs-authentik-20261002/{source,evidence}`.
Source comes from this task checkout at base `eb397d865cf2dc156fda172d50b13c98eda9d9f7`
plus its uncommitted work. The archive predates this final documentation receipt.
Evidence includes the source archive/manifest, provisioning and chart overlay
scripts, non-secret values, RED/GREEN/CI/build/Compose logs, real-browser results,
screenshots and `runtime-verification.json` with every mount and binary digest.

| Artifact | SHA-256 |
|---|---|
| Source archive | `b5e1bd2f8ba0494a8b1ad39bedad00eab9f124528e0692b1637279919b7387d7` |
| Source manifest | `8507f3f9d43e6f2b92b409b2a539310421c5da0e4631d0404234832bfee07e6b` |
| Aster containerd image | `a085f037a1767b2ef13f75b250dce6a2b4fca3fd7bf711062977239edec7f683` |
| Authentik running image | `ed120caf710ccf82ef0026f0bc74e51615bc95ebff228a7a2d6fc60c441c3868` |
| Contract manifest | `624a6339d761d00826effa497328858a08af6289ac80a5c96e60a3f7fce6ea15` |
| Browser result | `420b4b12e786293a561bc8f372621da467c593fb45f9a52596933fbf860b677c` |

Rollback restores the exact pre-change **Aster revision 3** and its pinned image
`sha256:70efef66ac0748066b036e0925fc8a91b311daee58babaffce6f3f303336b248`.
Its image, Helm configuration and old scoped OIDC Secret remain available. The
prepared script stops the single-checkout server before scoped Helm rollback:

```bash
ssh build-host 'bash /home/developer/aster/worktrees/odcs-authentik-20261002/evidence/aster-authentik-rollback.sh'
```

Inspected, not executed. It restores the prior **Keycloak/inventory-blocked** demo;
the isolated Authentik database remains available for recovery. The older revision-1
rollback receipt below remains historical. No commits, pushes or nested agents.

## Historical demo update receipt, 2026-10-02 — Keycloak inventory blocked

**Human seed:** update the existing isolated Build-host demo to the current task
worktree, five per-table compiled fixtures, schema owners/grants and real verified
Alice identity. Preserve standing stacks, default context, URL and rollback;
Build-host-only builds/tests; no agents, commits, pushes or production policy bypass.
**Outcome:** deployment and real SSO succeed; **usable unified inventory is not
complete**. This is a partial deployment receipt, not approval or policy completion.

### Scope, source and delivery

- Local source: `/home/developer/projects/aster/worktrees/odcs-ai-catalog-plan`,
  branch `docs/odcs-ai-catalog-plan`, base
  `eb397d865cf2dc156fda172d50b13c98eda9d9f7`, including intended uncommitted files.
- Fresh immutable source archive/294-file hash manifest and deployment evidence:
  `build-host:/home/developer/aster/worktrees/odcs-demo-r23-20261002/{source,evidence}`.
  Git metadata, dependency/build directories, environment files and secret
  handoffs/key files were excluded; the checked-in development realm fixture was
  retained. The archive predates this receipt. All 294 files were compared again
  after deployment with zero source mismatches.
- Target only: profile/context `aster-demo`, namespace/release `aster`.
  Its kubeconfig remains
  `/home/developer/aster/worktrees/odcs-minikube-20261002-0946/evidence/kubeconfig`.
  Build-host's default context remains `aster`; `aster`, `build-host-langfuse` and this
  demo's rootless containers remain running. The earlier snapshot is untouched.
- Build-host's declared devenv ran `just build-minikube`, `just chart-lint` and
  `just --set chart "charts/aster --values ../evidence/demo-values.json --post-renderer aster-odcs-demo --wait --timeout 10m" chart-install`.
  Helm revision **3**, chart `aster-0.1.0`, is deployed. A deployment-local Helm 4
  subprocess postrenderer uses native `kubectl kustomize`; it supplements the
  existing chart rather than duplicating its workload definitions. No production
  source/chart edits were made.
- Aster is pinned to the imported containerd manifest
  `sha256:70efef66ac0748066b036e0925fc8a91b311daee58babaffce6f3f303336b248`,
  `image.pullPolicy=Never`. Both running binaries use this image. Host OCI digest
  differs because `just build-minikube` exports Docker archive format; the complete
  image IDs and binary SHA-256 values are in `runtime-verification.json`.

### Actual runtime configuration and access

`ASTER_CONTRACT_BUNDLE=/etc/aster/bundle`,
`ASTER_CONTRACT_BUNDLE_VERSION=mock-catalog-r23`, and
`ASTER_CONTRACT_MANIFEST_SHA256=624a6339d761d00826effa497328858a08af6289ac80a5c96e60a3f7fce6ea15`.
The read-only ConfigMap subPath mounts preserve exact selected bytes and compiled
paths: five documents (orders, customers, daily_revenue, events, clickstream),
five physical bindings, the five existing `aster-editors` grants, and a separate
`schema-owners.json`. Owners map `mock-local` with each literal namespace
`["aster_demo"]`, `["aster_demo.sales"]`, `["aster_raw"]` to `aster-editors`.
The mounted files and all pins were checked against the exact source.

Login: **http://localhost:8080/login**, user **alice**, using the existing password
in the Alice entry of `keycloak/aster-realm.json`; no credential values in this
receipt/chat. The app URL remains **http://localhost:8080**. Keycloak is the public
Compose image `quay.io/keycloak/keycloak:26.7.4`, confined as a demo server sidecar.
The realm/client credential is supplied by Secret `aster-odcs-demo-oidc`, not
values or generated plaintext manifests. Its in-memory import overlay changes
the group mapper to emit full paths, preserving Alice's seeded `/aster-editors`.

Actual fields: `ASTER_IDP_KIND=oidc`,
`ASTER_OIDC_ISSUER=http://localhost:8081/realms/aster`,
`ASTER_OIDC_CLIENT_ID=aster`, `ASTER_IDP_SCOPES=openid`,
`ASTER_IDP_GROUPS_CLAIM=groups`, `ASTER_IDP_USER_UUID_CLAIM=sub`,
`ASTER_OIDC_REDIRECT_URI=http://localhost:8080/callback`, and role mappings
`ASTER_OIDC_EDITOR_GROUP=/aster-editors`, `ASTER_OIDC_ADMIN_GROUP=/aster-admins`.
`ASTER_DEV_LOGIN` is absent. The existing local tmux socket/session
`-L aster-odcs` / `tunnel` retains port 8080; an added `oidc` window forwards
8081 to Build-host 18081. Remote `aster-odcs-forward` retains the app forward and
adds an `oidc` window. Both local health/discovery endpoints returned HTTP 200.
Temporary Build-host browser-only forwards were stopped after verification.

### Verified results, corrections and hard blocker

| Check | Observed result |
|---|---|
| Exact snapshot / build | 294 files match; `BUILD_COMPLETE`; image side-loaded only into the scoped profile |
| `just odcs-mock-fixture-validation` | PASS: one network-isolated fixture check, six intake tests and the real sibling-table grant isolation test; no new cases ignored |
| Chart/default and final overlay validation | PASS: 14 default and 15 overlaid resources valid, zero invalid/errors/skipped |
| Rollout | Four pods / five containers ready; controller, PostgreSQL, Valkey and server with Keycloak sidecar |
| Actual browser SSO | Real authorization-code/PKCE callback succeeds; Valkey session readback has `verified=true`, role `editor`, group `/aster-editors` and signed user UUID |
| Mounted contract count/granularity | Five pinned, distinct per-table compiled documents and five bindings/grants verified; this is not a successful UI/API count |
| Actual catalog/owner view | `/catalog` renders HTTP 200 with **inventory unavailable** and denied contract selection; namespace view returns 403 |
| Authenticated contract/inventory RPCs | `ListContracts`, exact `GetContract`, `ListCatalogInventory` each return 403; no fabricated authorization or successful owner view |
| Browser evidence | `browser-result.json`, `login.png`, `catalog.png`; screenshot visually inspected |

**Blocking implementation gap:** `current_identity::current_principal` requires
both a verified session and an independently fresh `CurrentIdentityProvider`.
The sole runtime provider is the Authentik HTTPS user/group API adapter; Keycloak
OIDC alone cannot configure it. Runtime `team_workspaces` is always `None`;
the other team registry requires the Authentik/GitHub team-Git setup and group
UUIDs, not the seeded Keycloak path claim. Therefore mounted owner/grant team IDs
cannot become usable through current supported configuration. No Authentik-shaped
fake, verified header, manually seeded session, unrelated identity deployment or
production-code weakening was used. A supported Keycloak fresh-membership and
team-registration path is the next implementation handoff, not completed work.

Deployment corrections were observed and resolved: Helm 4 requires a named
postrenderer plugin; Kustomize requires a recognized kustomization filename;
containerd needed an explicit repository-at-digest alias for `Never` pull policy.
RollingUpdate reproduced `notebook checkout already in use` against the shared
PVC; the scoped overlay now uses **Recreate**, recorded in revision 3. Four server
restarts occurred during that correction, not an ongoing healthy-state claim of
zero restarts. Earlier failed attempts/logs remain in evidence. Direct author
review covered security, reliability, compatibility, operations, test assertions
and deployment ownership; no independent reviewer approval is inferred.

`semantics.yaml` is not mounted as an intake input or claimed loaded. SQL/share
target enforcement remains unimplemented. Keycloak uses disposable sidecar data;
pod replacement reimports the seed and may require a fresh login. This update
does not rerun the full previously passing S8/CI chain or claim production policy.

### Evidence hashes and rollback

| Evidence artifact | SHA-256 |
|---|---|
| `aster-demo-source.tar` | `161b315c41fe058597653951566da72629aa50667e876d16973c03ae7e087f4d` |
| `aster-demo-source-manifest.json` | `dddd11252fb29ce38d1868c79740b68deab6b18e1d7bc974d956a6daad3c1068` |
| `build.log` | `265f770a0ff5f65a2cd73bb8531767a09efba4c22e0057204475977878ebb2e0` |
| `check.log` | `7c07fdef5bc805f843d48ecff6abef3f730bcbb477f3b0852aad46fa27cb1e29` |
| `install-final.log` | `fd9506a49128de6dd06636e0cb616678c6bea692f0c22a6a3d45186cfbdc98bb` |
| `runtime-verification.json` | `cde01d99c7e481fb6c8f69acc92dae44607bdd17fe72090aa2528aae730c72f1` |
| `browser-result.json` | `4a2ed89a4dca062191cdb62b93874eceefef9f70b3b00ec7e853c57944a243bd` |
| `catalog.png` | `6d55d56f69a53e2b1be9aed5d79f48ef9edcf32644365e7dc427be793d91b396` |

Rollback remains revision **1**. Its original containerd manifest
`sha256:7d02c6f5e608c4df45f1c374f2d2f89706c194382401c4bec410b4f64ce41228`
is retained as `docker.io/library/aster-server:odcs-rollback-revision-1`; its
config digest was checked against the previous pods' exact image ID. Plain
`helm rollback` is insufficient because revision 1 used the mutable `local` tag.
The prepared command restores that tag, stops the current single-checkout server,
then runs scoped `helm rollback aster 1 --wait --timeout=10m`:

```bash
ssh build-host 'bash /home/developer/aster/worktrees/odcs-demo-r23-20261002/evidence/rollback.sh'
```

The script is inspected, **not executed**. It restores the earlier development
demo, not an equivalent policy configuration. No commits, pushes or other-cluster
changes. The unmet usable-inventory requirement is explicitly handed back.

## Independent r23 RV ses_f01458357ffeG13RoBziy3aSeB

User-relayed independent review reported **no verified findings**, source-only,
with no new runs. Per-table contracts and S8 reviewer fixes are complete after
review. Prior Build-host receipts and hashes remain the execution evidence. SQL/share
target enforcement, global activation/legacy-mode migration and runtime semantics
ingestion remain pending. This receipt changes documentation only: whitespace
checks only, no build/test repeats, commits or deployment.

## R23 materialization-level mock contracts

The user interrupted the prior handoff with the [exact materialization-level
correction](odcs-ai-catalog.md#human-correction-r23-materialization-level-contracts).
Inspection found the S8 reviewer-fix gate had completed successfully, including
cleanup; its immutable r22 receipts are recorded below. Existing partial edits
were preserved. This continuation changes the known mock artifacts and their
consumers, not generic ODCS parsing or the future kind of every materialization.

The bundle now contains five full compiled documents with distinct stable IDs,
one object and one exact physical binding each. Whole-contract grants therefore
select orders, customers, daily_revenue, events or clickstream independently.
The existing `aster-editors` grant population and aggregate five-table scope are
unchanged. Three schema-level semantics descriptions remain; format 2 places an
exact contract path/ID/version/digest and object pointer on every table entry.
Manifest, bindings, grants and fixture sidecar pins are regenerated from exact bytes.
No semantics ingestion, SQL authorization or deployed-policy claim is added.

Only the three task-created r20 mock documents were replaced, after tracing
manifest/binding/grant/sidecar consumers, the fixture test and gate, and handbook/
planning references. General multi-object intake and preview fixtures remain.
An isolated copy of the actual bundle supplies the route tests; published fixture
grants are narrowed in that disposable copy, never in the working fixture itself.

Build-host RED before artifact replacement:
- `red.log`: existing bundle had 3 documents, expected 5. Its first admission-test
  compilation failed on a test JSON macro expression; that is setup failure.
- `admission-red.log`: the actual orders grant returned schema objects
  `["orders", "customers"]`, expected only `["orders"]`.
- `red-source.tar` preserves the original three-document bundle and failing tests.

`devenv shell -- just odcs-mock-fixture-validation` now passes: 1 network-isolated
fixture check, all 6 intake tests and the actual sibling-isolation route test.
The route test switches a non-owner's grant in both directions for orders/customers
and events/clickstream, checking List/GetContract, preparation, inventory and legacy
table listing. It excludes the sibling's raw fields and separately proves their
authorized positive disclosure. New bound S8 coverage requires **6 Rust tests /
6 scenarios / 6 steps / zero S8 skips**. The full canonical S8 gate passes,
including CI (90 server scenarios), the previous S1–S7 regression chain and all
seven isolated Compose smokes. The general multi-object intake/preservation and
preview tests pass unchanged. No fixture/admission test was skipped.

New source/evidence:
`build-host:/home/developer/aster/worktrees/odcs-mock-r23-20261002/{source,evidence}`.
Earlier S8/r20 snapshots and logs are retained; only the prior build cache is reused.
Final S8 project `aster-odcs-s8-ebc0abd9979a` removed its containers, network and
volumes. The independent post-cleanup query found no volumes for either this
project or the r22 final project. `build-host-langfuse`, `aster` and
`aster-demo` remain running. No executable or pinned fixture edits followed
the successful gate; subsequent edits are handbook/receipt reconciliation only.
Checksum comparison found only those documentation differences before final sync.
The verified source archive precedes its own receipt hashes. Subsequent independent
source-only review is recorded above; it adds no runtime execution evidence.

Platform's existing three task documents carry the exact seed, updated artifact
boundary and S8 dispositions. Its isolated Build-host `just test` still stops at the
missing `references/repos/host-config` link; `just docs-check` still reports
14 pre-existing broken targets. These separate coordination blockers are recorded,
not repaired or counted as all-repository GREEN.
No agents, source commits, pushes, Minikube changes or standing-stack deployments.

| R23 evidence artifact | SHA-256 |
|---|---|
| `verified-source.tar` | `06e90d636226c8cda77103ca7111afc3230384174814a027ac71827250018414` |
| `red-source.tar` | `2d4f4b2d9864e1846ef6f073f0b37620e45633f3159656974cf8019c3697fe30` |
| `red.log` | `ead00cd407344b47a4c4e4319b74cc2ead2c3b18e9c33cfe063ff38c9b898ccf` |
| `admission-red.log` | `e8b9914dd526dd2759b59171b76ef872c1b7b6be995a4277f64ecb48f274e376` |
| `mock-gate.log` | `7a9d284a70befeaf72586dd238dea18f9865a8a4705c2efcbbe4725c47ffaeac` |
| `s8-gate.log` | `a5e6037ba84b0f0ddba3ab26ee14e670645bc2ec70edb0c11721f4334d4b703e` |
| `platform-checks.log` | `938f7e1053ae9916b2e36bd15c0f0633619c146bd9660f47bbe21622460cf090` |
| `post-cleanup.log` | `0addcbd06af7aed72b96297a61cf73a578d99be4cd2d01403ee75729544dc8e2` |

Selected bundle `mock-catalog-r23` manifest SHA-256:
`624a6339d761d00826effa497328858a08af6289ac80a5c96e60a3f7fce6ea15`.
All document, binding, sidecar and example-grant pins are checked by the fixture gate.

## Independent S8 RV ses_f029ed1c9ffe2awvOuIQ06x5Xh: r22 corrections

The user relayed three source-verified P2 findings and authorized fixing them
before another slice. All three have observed Build-host assertion RED and implemented
corrections. The full canonical S8 gate passed before the r23 artifact correction.
This is a disposition of the independent review, not a new
reviewer approval. No next slice, deployment, commits, pushes or agents.

| Finding | Observed RED | Correction and controls |
|---|---|---|
| Unknown schema versus configured unauthorized schema leaked configuration | Actual `ListCatalogInventory` returned 403 for unknown and 200/denied for configured; exact status/payload comparison failed | Both use the same undisclosed response. `ListTables` shares it. Valid authorized discovery is the positive control; denied probes make zero catalog calls. A follow-up RED reproduced the same distinction with malformed grants, so global grant validation now precedes either schema outcome. |
| Two identity refreshes combined stale ownership with revoked membership | An alternating authority returned member then nonmember within one request, yet the response included owner-only objects | Inventory resolves one fresh principal and supplies it to existing contract-grant admission. Multi-schema page and namespace operations reuse that principal across their scopes. Four alternating RPC requests and allowed/denied two-schema page/REST requests assert one resolution per operation, coherent owner/grant outcomes, and no IO on revoked requests. Standalone contract reads still refresh independently per request. |
| Nested property annotations produced a false `not_declared` facet | A schema-valid nested-only `semanticType` returned `not_declared` | Traverse ODCS `properties`, array `items`, and map `key`/`value`. Twelve route controls cover both annotation kinds and annotation-free objects/arrays/maps. Annotation-shaped custom data is ignored. This detects declarations, not completeness or executable semantic support. |

The existing physical inventory test now also exercises successful table detail
for contracted and owner-only uncovered tables, asserting returned column/type
sentinels and exactly two schema reads. Revocation prevents further schema reads.
That additional positive control was already GREEN on the reviewed implementation;
it is not claimed as a fourth bug. A registered engine refuses any execution.

Evidence is isolated under
`build-host:/home/developer/aster/worktrees/odcs-s8-20261002/evidence/rv-f029ed1c/`.
`before-source.tar` preserves the reviewed tree, SHA-256
`4a66f2fad0f3a7e1675438b7b538f964014ccb24262dfa690a9a6ce4f190e2f2`.
`red-assertions.log` contains all three actual behavioral failures before production
edits. The first `red.log` nested fixture used an invalid array shape; its schema
validation error is setup failure, not semantic RED. The corrected fixture uses
the vendored ODCS schema's supported structure. `existence-error-red.log` records
the malformed-grant follow-up. `red-source.tar` retains test-first source; it also
includes later-added multi-schema controls. Earlier S8 evidence is untouched.

The canonical gate now requires **5 named Rust tests / 5 bound scenarios / 5 steps /
zero S8 skips**, then the same full transitive CI/regression/Compose chain. Bound
review scenarios were added after the Rust assertion failures. Full CI (89 server
scenarios), prior regressions and all seven isolated Compose smokes passed. Final
S8 project `aster-odcs-s8-7ac7ea0fd6a3` removed its containers, network and volumes.
The stopped local tool call had not cancelled this remote gate; completion was
confirmed from its success marker before starting r23 fixture work.

| R22 review-specific artifact | SHA-256 |
|---|---|
| `before-source.tar` | `4a66f2fad0f3a7e1675438b7b538f964014ccb24262dfa690a9a6ce4f190e2f2` |
| `verified-source.tar` | `80f3b8eddf4be795447621723becc29c8ac200efb59501b836812d79b3225fae` |
| `gate.log` | `8f78ec2d1b3b19ff3a047a2382b42d1a16d912c181ca4f655b274dc7d2e48b96` |
| `red-assertions.log` | `88640002b549832a27510d814d45626580d343731ed2fda7b00e7cae6eacda81` |
| `existence-error-red.log` | `a18e4267d1799863203e6172d5520d4b4aaa2fcbbe8019d120eb9e3fc8a7cf0b` |

All three review findings are resolved in this candidate. R23 reruns this same
regression chain with one additional table-grant scenario; the earlier five-case
receipt is not relabelled as a six-case run.

## S8 / r21 runtime candidate, 2026-10-02

Historical pre-review candidate and receipts follow; the r22 dispositions above
supersede its independent-review handoff and two-case owning gate counts.

The user authorized the first runtime slice in the existing task worktree after
r20 source-only review. The configured-bundle S8 candidate now passes its full
Build-host gate and is ready for independent RV. This is not independent approval
or a completed r20-wide enforcement claim.

Build-host identity verified as `developer` / `build-host`; isolated source/evidence:
`/home/developer/aster/worktrees/odcs-s8-20261002/{source,evidence}`. Prior snapshots and
the running `aster`, `aster-demo`, `build-host-langfuse` stacks were preserved.
All Rust, devenv, browser and Compose execution is remote. The S6 target directory
is reused only as a build cache; its recorded source/evidence is not overwritten.

Observed assertion RED anchors before their behavioral corrections:
- `red.log`: ListCatalogInventory 404 versus expected owner inventory 200.
- `legacy-red.log`: ListTables disclosed both objects after current group removal.
- `ai-red-assertion.log`: actual HTTPS helper capture contained physical metadata
  from unselected discovery in compiled mode. `ai-red.log` was an initial missing
  fixture-module compilation error, not RED.
- `history-red.log`: catalog-only saved history replay returned 200 versus 403.
- `authority-red.log`: a non-authoritative index still produced physical entries;
  the catalog port now defaults that capability off. Only Polaris and fixture
  Mock opt in; no caller switches on provider kind.
- `marker-negative.log`: deliberately wrong inventory count failed the owning
  gate before its success marker; exact test source was restored.

The final `devenv shell -- just odcs-inventory-validation` passes **2 Rust tests,
2 bound scenarios / 2 steps / zero S8 skips**. It includes S6/S5/S7/S3/S2/S1
regressions, full CI (86 server scenarios), browser/PostgreSQL checks, and all seven
isolated Compose smokes. Existing unrelated external-service/doc-test ignores
remain. Bound scenarios were added after Rust assertion RED; no preimplementation
Gherkin failure is claimed. The deliberately broken count proved that the canonical
gate fails without its final success marker.

The final S8 project `aster-odcs-s8-1e0b2e0ac619` removed containers, network and
volumes. `post-cleanup.log` independently confirms its volume query is empty and
the original three standing containers remain running. No Minikube/deployment
changes occurred. No production/test edit followed the successful final gate.
Checksum rsync found only subsequent handbook/receipt edits, no executable-source
differences. `verified-source.tar` predates final receipt wording. Prior source/log
archives remain intact; the initial checkpoint gate is not substituted for final
verification after the authority correction.

Direct author review covered conformance, correctness, security, reliability,
performance, architecture, compatibility, tests, operations and basic accessibility.
Verified findings corrected: legacy browse bypass, unselected AI and catalog-only
history disclosure, and treating semantic/discovery index listings as physical
evidence. The UI escapes metadata and keeps artifact context separately labelled.
No independent reviewer or nested agent was used; no new independent approval is
inferred. Ownership defaults and global activation remain explicit review concerns,
not silently accepted exceptions to r20.

| Evidence artifact | SHA-256 |
|---|---|
| `verified-source.tar` | `2eb265524b1582c8a6cc9c38c39b4d843a8efc6dd805165a7d2381b71550c91e` |
| `gate-final.log` | `1333f28045b06ab5c16a7558e0246644f7ca77bb3b7095ae092a62dc1180151a` |
| `red.log` | `c0d94eee2cc4f81b26ddaef95af0087cbecb0a0ddeeb7c9dd405d12b6e2deb81` |
| `legacy-red.log` | `a1dd2c19385d4406022377b0a58b860cf4ca8650a7fa8baf10f1524806346a1f` |
| `ai-red-assertion.log` | `89d3f8cf8eb592c2102ffd65c29523cc4ce26baf88b882e8a4ca2dc8d65e1400` |
| `history-red.log` | `48600461a20bf8ca00856e1f504160976fdf124118ba1737107718a43ac8d2df` |
| `authority-red.log` | `f9a74db868a1e59c86581db1acc1a505d9504e23dcf8472e74a79a275951653b` |
| `authority-red-source.tar` | `d77a5d97cd476f021b642edca1f03e462fd900ad315f79c4d95d9281f990da94` |
| `marker-negative.log` | `bccf6907125da270649181295c77fdac80990cc3a90f322c6bf1e780ee8d9d9c` |
| `first-implementation.tar` | `752ba1d7022973b3f68a2598365b783e87563c3cf778ab7d6e60826fd08229a3` |
| `platform-checks.log` | `95adb0b8ab01c0e7fcbad7759c36e306ea640038638c5a5eaa3b65995b9cd324` |

Platform's proposal/index/README are synchronized in the existing task worktree.
On the isolated Build-host copy, `just test` still fails at missing
`references/repos/host-config` (traced), and `just docs-check` reports the same
14 existing broken targets. These are coordination blockers, not an Aster gate
failure or an all-repository GREEN claim.

Scope: configured compiled-bundle metadata inventory, not universal SQL or sharing
enforcement. Independent review must include default-mode migration/activation,
provider authority, caller closure, revocation/history, fault states, bounds and
the explicit separation of artifact context from physically observed entries.
No-bundle development mode remains legacy; removing a bundle is not a safe policy
rollback. Sidecars are not runtime inputs. Independent S8 RV is the next handoff.

## Historical r20 docs/mock review

**Historical record / revision:** r20 docs + mock fixtures independently reviewed;
no verified findings. [Exact seed/rules](odcs-ai-catalog.md#human-correction-r20-physical-inventory-ownership-and-mock-contracts-2026-10-02)
and [fixture inventory, pins, Cube sources and integration proposal](../../contracts/mock-catalog/README.md)
govern this step. Read-only Build-host inspection confirmed five mock tables in three
literal namespaces. No runtime policy, semantics ingestion or cluster update.
The earlier admin/private exceptions and contract-only accessible inventory are
superseded; owner-only uncovered metadata does not authorize owner queries.

**Independent r20 RV:** user-relayed session `ses_f030e0250ffeIPVS1cgeiNPZIZ`
reported no verified findings. The reviewer verified source read-only and performed
no runs; the Build-host receipts below remain the author's execution evidence.
Fixture consistency proves neither Cube execution nor actual business grain.
Unified runtime UI, schema-owner enforcement, query-target coverage and sidecar
ingestion remain unimplemented; no deployed behavior or deployment approval is claimed.

### R20 Build-host fixture receipt, 2026-10-02

Snapshot: `build-host:/home/developer/aster/worktrees/odcs-mock-r20-20261002/source`;
logs under the adjacent `evidence/`. Verified login `developer`, host `build-host`.
All tooling used the declared devenv with `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, and the existing S6 target
directory as build cache. No local builds/tests or live stack modifications.

| Build-host command | Observed result |
|---|---|
| `devenv shell -- just odcs-mock-fixture-validation` | PASS: 1 network-isolated fixture test, 6 existing intake tests, zero failed/ignored/filtered. Pinned official ODCS schema, manifest/document/binding/sidecar digests, 3 full documents, 5 exact bindings, raw bytes/tree and all source columns checked. |
| `devenv shell -- cargo fmt --all -- --check` and `just features` | PASS; 48 well-formed feature files. R20 feature remains unautomated, not newly bound or executed. |
| `devenv shell -- just ci` | `CI GREEN (fmt + clippy + tests + features + repo + providers)`; 31 core and 84 server bound scenarios pass. Existing external-service/doc-test ignores remain; the new fixture has none. |
| Platform snapshot `just test` | Blocked at missing required `references/repos/host-config` symlink; `platform-blocker.log` traces the exact assertion. |
| Platform snapshot `just docs-check` | Blocked on the same 14 pre-existing missing-link targets recorded in r19. No new missing path reported. |

Logs: `fixture-gate-final.log`, `ci.log`, `platform-checks.log`,
`platform-blocker.log`, `features-final.log`. The final draft-feature clarification
(artifact context is not accessible physical inventory) passed a fresh 48-file
shape check; no executable code changed after CI. Checksum rsync of all tracked
and nonignored task files found zero Aster or platform snapshot differences before
this final receipt addition. Existing `aster`, `aster-demo` and
`build-host-langfuse` containers remain running.

Receipt hashes:
- `fixture-gate-final.log`: `0a0f643229e450ef46409bfd0028763d8a4500aae20764405e6ac4ea2e941300`
- `ci.log`: `bde0f6d176e76955866fd018ae80563cbd15b014c958705f09cf391883abeb2c`

The initial fixture validation caught a YAML flow-scalar
comma incorrectly creating extra keys; quoted descriptions and repinned affected
bytes before the final successful run. Initial test compilation also required
reusing workspace `serde_norway` as a server test dependency; only that existing
locked dependency edge was added. These are fixture authoring corrections, not
RED/GREEN evidence for new runtime policy. No compiler or production code changed.

Manifest SHA-256 is `8ed7ada3b1a912092ef0551c235e9e581f1f365551b5d6e0142d9179f5fae00c`;
official schema remains `edb41f33ec46e84780e99872ab2bd67f074959d2bf3e9c9fc54e61f8982b0d93`.
The fixture README records all exact paths, the read-only live inventory and Cube
source URLs/pinned implementation. Semantics association is verified as fixture
data only; no sidecar ingestion or Cube execution is claimed. No Compose/image
or deployment run was required for this docs/fixture/test-only step.

Historical S1–S7 evidence below is preserved and does not prove r20 enforcement. Feedback
binds to r20 in the existing Aster/platform task worktrees and bases, with independent
review recorded above. No nested agents, commits or pushes.

## Historical r19 review record (superseded by r20)

**Historical record / revision:** r19 source-grounded discovery amendment; new UX/access
scope pending Q9–Q11, not implemented or independently approved. See the
[canonical correction](odcs-ai-catalog.md#human-correction-r19-unified-catalog-and-access-scope-2026-10-02).
R18 closure remains historical acceptance, superseded only as the current
destination. Prior runtime receipts do not prove unified inventory or
contract-required query admission.

Source review traced web navigation and both contract views, JS selection and
preparation, whole-contract grant/current-team admission, physical metadata guards,
and REST/RPC convergence on role/engine/routing/backend checks. The current query
body has no contract selection and performs no coverage admission. No private
product classification/owner exception was found on those paths. Separate facets
must not imply semantic completeness or authorization. Findings and precise source
symbols are in canonical r19; this is direct author review, with no agents or new
independent review session.

Only planning documents and the existing unautomated behavior contract change.
No runtime checks/builds are needed for this source-only revision; future builds
remain Build-host-only. Documentation validation on 2026-10-02: `git diff --check`
passed in both task worktrees; explicit `git diff --no-index --check /dev/null`
checks passed for all four untracked Aster plan/stage files and the unautomated
feature. New relative links/heading destinations were source-inspected. Platform
`just test` failed; `bash -x tests/contracts.sh` confirms the existing missing
`references/repos/host-config` symlink assertion. `just docs-check` reports the
same 14 previously recorded broken-link occurrences. No all-repository GREEN.
Feedback binds to r19 and Q9–Q11 in the existing Aster/platform task checkouts.

## Historical RV record through initial-scope closure

**Stage / revision:** RV record / r17, initial scope accepted complete
**Status:** Initial implementation scope complete after prior per-step RV and explicit human acceptance in canonical r18. Original-document preview is the initial export; remaining S4 transformed exports, optional Cube and new-document generation are explicitly deferred, not passed. Q3/Q4 are future scope; Q6 awaits authorized targets and credentials for live validation, not production proof. Earlier Build-host evidence, review dispositions and archives remain intact; platform coordination gates remain blocked.
**Owner / date:** Aster implementation author / session 2026-10-02 UTC
**Basis:** [plan and original seed](odcs-ai-catalog.md), [PL](odcs-ai-catalog-pl.md),
[IP](odcs-ai-catalog-ip.md); independent architect review
`ses_f1b194920ffeAVrdTlVZ0G8Rfy` (r2 only), and independent r3 review
`ses_f1ab8829cffefXA3JcQf0narDN` (four findings relayed by the user).

## Outcome and why it matters

### Initial scope closure accepted, 2026-10-02

The user's explicit “yes” is recorded with scope and checkout provenance in
[canonical r18](odcs-ai-catalog.md#human-acceptance-r18-initial-scope-closure-2026-10-02).
S1/S2/S3/S7/S5/S6 and narrow S4 are complete after the recorded per-step RV and
corrections. Remaining optional S4 output and Q3/Q4 policy are explicitly deferred
to future scope; `odcs-semantic-validation` remains unimplemented/unrun, not GREEN.
Q6 is an activation/validation handoff after authorized targets and target-specific
credentials are supplied through the existing mechanism. No live production proof
or new independent reviewer approval is inferred from closure.

This documentation-only acceptance supersedes earlier pending-scope language;
historical receipts below remain unchanged. No builds or runtime tests were repeated.
Known platform blockers remain the missing host-config reference and 14 existing
broken-link occurrences; no all-repository GREEN claim.

Closure checks ran locally in the named task worktrees: both `git diff --check`
checks passed, as did per-file whitespace checks for all nine edited documents
(including untracked plans). Targeted relative-link/anchor checks resolved all
127 Aster links; the three platform files contained 101 links with 13 known missing
targets and no unresolved anchors. New acceptance links resolve. Platform `just
test` failed; `bash -x tests/contracts.sh` confirmed the missing
`references/repos/host-config` symlink assertion. `just docs-check` reported the
same 14 broken-link occurrences repository-wide. No new artifacts were created.

### Independent narrow S4 RV ses_f04fc5bd4ffe252GUe2zkjtRIW, 2026-10-02

The user relayed independent source-only review of the narrow preview candidate
below: **no verified findings and no found regression**. The approved original-source
preview is complete after review. No runtime tests were rerun by that review or
this documentation-only disposition; prior execution receipts and hashes remain
the runtime evidence.

Review clarifications: exact-byte preservation means the **decoded UTF-8 value** of
`originalSource`; its JSON wire representation necessarily escapes content. Intake
limits bound admitted source inputs, not the serialized response size, and are not
a response-size guarantee. Neither note is a verified regression or a new policy.
Whole S4 still awaits Q3 transformed-export rules and Q4 optional Cube/new-document
decisions; Q6 live targets/credentials remain pending. This disposition updates
the same checkout to canonical plan r17 without changing production or tests.

Disposition checks: whitespace passes in both worktrees and all nine edited
documents; all 121 relative-link targets in the six edited Aster pages resolve.
Platform's link check still reports the same 14 pre-existing broken targets.
Only documentation checks ran; no builds, CI or runtime reruns.

### S4 original-source preview: tested candidate for independent RV, 2026-10-02

**Scope and checkout:** the user accepted only the full original ODCS v3.2 preview,
with whole-document team admission, provenance and zero execution. Work remains in
`/home/developer/projects/aster/worktrees/odcs-ai-catalog-plan`, branch
`docs/odcs-ai-catalog-plan`, base `eb397d865cf2dc156fda172d50b13c98eda9d9f7`,
canonical plan r16 at handoff. This execution receipt precedes the source-review
approval recorded above; its runtime evidence is unchanged.

**Discovery / RED / minimal GREEN:** actual S3 GetContract returned the complete
parsed `declared` tree and provenance after fresh team authorization, but omitted
stored source bytes. The new endpoint assertion first passed whole-tree equality
and independent pinned-schema validation, then failed with `originalSource = None`.
The production change only adds `originalSource` to that same admitted response,
using the already-preserved UTF-8 source. No new endpoint, source reconstruction,
generated document, drift policy, catalog dependency or execution was introduced.
The initial `red.log` compile failure (test-only unavailable serde_norway import)
is setup failure, **not RED**; corrected `red-assertion.log` is the observed failure.
The initial devenv acquisition timed out before testing; subsequent runs succeeded
despite the existing Cache-host cache timeout warning.

**Exact verified gate:** Build-host `devenv shell -- just odcs-original-preview-validation`
passed **1 Rust test / 1 bound scenario / 1 step / zero S4 skips**. The fixture reuses
S3's real Connect router, verified session and current-identity authority. It checks
all objects, two servers, nested properties, semantics, description/quality/team,
support/roles/price/SLA/custom properties, identity/version, full tree equality and
byte equality including comments, CRLF and Unicode. The response is validated
against the SHA-pinned published schema without calling Aster's validation helper;
an invalid `logicalType: bigint` control must fail. The same endpoint test passes
under `unshare -Urn`. Missing catalogs, denied observation and available observation
all retain the original declaration with zero catalog calls. A registered engine
panics on execution; notebook writes panic; the full fixture file snapshot remains
unchanged. Object selection cannot truncate disclosure; S3's fresh membership and
grant revocation/default-deny checks run in the reused fixture.

The gate also passes S3/S2/S1 regressions, full CI (including Clippy with warnings
denied), existing S5/S6/S7 Rust/BDD/browser coverage within CI, and isolated Compose
smokes for S1/S2/S3. The full server BDD runner reports 84 passing scenarios.
Existing unrelated external-service ignores remain; no S4 cases are ignored.
Final project `aster-odcs-s3-c6b80de4ab5f` removed its containers/network/volumes;
the post-run project volume query is empty. `aster` and `build-host-langfuse` remain
running. This session did not rerun the separate S5/S6/S7 Compose gates.

**Evidence:** `build-host:/home/developer/aster/worktrees/odcs-s4-preview-20261002/evidence`.
All builds/tests/formatting/devenv/Compose ran on Build-host with
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`;
the existing S6 target directory was reused as a build cache. No production/test
edits followed the successful gate. Checksum rsync found only subsequent document
edits; after synchronizing those, it found zero local/Build-host source differences.
`verified-source.tar` precedes this final receipt wording. Earlier S6 source and
gate archive hashes were rechecked and still match their receipt below.

| Artifact | SHA-256 |
|---|---|
| `red-assertion-source.tar` | `1029b77963524806c64dbfb9873b8615f0d6c7056aec848525b87b3dffc206fc` |
| `red-assertion.log` | `e2af52ea3d6e427a434723b01c3bd25acfb41488860b118d8b094fe0c6c01634` |
| `focused-green.log` | `360ee5da3c95290bf61287432bcc7cec6a3a3380aa62b8cacee18d28faed3941` |
| `verified-source.tar` | `5295f0c1fadc86070506380f54dd60f865725e2ef46692cbe3389ae960b92bbb` |
| `gate.log` | `0a242409b519cdb4d9309bb10444c6605e743e6babf9033878a3a1468fe7dc7d` |
| `platform-test.log` | `27e0e634177b59486221ddb2eb1c57ffe4af986c347c9ffcdf14fcc2b717c158` |
| `platform-test-trace.log` | `c330131bc72043a09d95508107a0df0b2b1b4d86e35ed3d7d358783ab5a6860e` |
| `platform-docs.log` | `a3d00c287f2ad76cc7082fb9570d3be95d06271878fcb1abf7fdd87a71c239bc` |

The three existing platform coordination files were updated in their existing
`aster-odcs-plan` worktree. Build-host `just test` still stops at the missing
host-config reference (confirmed by the trace); `just docs-check` still reports
the same 14 pre-existing broken targets. These separate coordination blockers do
not change the passing Aster gate.

**What waits after review:** Q3 transformed-export
drift rules remain undecided; original declared-source inspection does not choose
them. Q4 optional Cube inclusion/subset/dialect and new-document identity/version
remain an explicit handoff, including an independently executed Cube oracle if
chosen. Whole S4 and `odcs-semantic-validation` are **not green**. S6's two P2
corrections remain complete; Q6 live activation still needs an explicitly authorized
target and target-specific credentials through the existing secret mechanism.
No nested agents, commits, pushes, real credentials or deployments were used.

### Independent S6 RV ses_f05412269ffeyYuCSDjOoOprQU: two P2 corrections

The user authorized these fixes with TDD and Build-host-only builds/tests, preserving
accepted scopes and previous evidence. Review-specific receipts are under
`build-host:/home/developer/aster/worktrees/odcs-s6-20261002/evidence/rv-f0541226`.
Before edits, `before-source.tar` preserved the reviewed source with SHA-256
`e48320ae426ef1737345dd8289740959ef0ff9f0667aa69c124f71382af1612c`.

| Finding | Observed RED before production edits | Root correction and focused GREEN |
|---|---|---|
| Partial `POLARIS_CLIENT_ID` / `POLARIS_CLIENT_SECRET` silently became anonymous | Both separate half-configured server startups remained alive and each made one anonymous upstream request; expected failed startup and zero requests | The shared core environment parser now returns `Result<Option<String>>`. Only both absent selects no OAuth credential; partial, empty or non-UTF-8 input returns a value-free configuration error. The default-catalog branch propagates it to both server/controller callers. Both startup halves now refuse before catalog IO, with zero requests and no credential values in diagnostics. Both-absent and complete-pair startup/wire controls remain successful. Explicit catalog-pool selection retains its existing precedence. |
| Polaris health restarted the deadline after OAuth | A real HTTPS fixture delayed OAuth six seconds and health headers six seconds; health returned `Healthy` after 12.009 seconds | JSON and status-only reads now share one budgeted response path. Health keeps the same budget through OAuth and its status request, returning unavailable near the ten-second deadline. Positive non-JSON status-only health, HTTP degradation and failed-auth controls still pass. |

`cargo test -p aster-catalogs --test catalog_quality` passed **5 tests** and
`cargo test -p aster-server --test catalog_credentials` passed **2 tests**, all
with zero ignored cases. A separate cumulative-page regression was already GREEN
on the reviewed implementation: two delayed one-MiB pages complete in order,
while two individually valid three-MiB pages fail the aggregate four-MiB budget.
This is added regression coverage, not a third production defect or claimed RED.

The existing bound S6 scenario calls all seven checks; its success-only report now
requires **7 tests / 1 scenario / 1 step / zero S6 skips**. The canonical
`devenv shell -- just odcs-catalog-quality-validation` passed on Build-host using
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.
The full rerun was justified by changes to core configuration and shared transport:
CI (including Clippy with warnings denied), provider/routing/Generic checks,
S1/S2/S3/S5/S7 regressions, offline validation, PostgreSQL/browser checks and all
six isolated Compose smokes passed. S5 retained 12 tests and 3 scenarios/steps.
Existing unrelated external-service ignores remain; zero S6 cases are ignored.

Final Compose project `aster-odcs-s6-9732c246c513` cleaned its containers, network
and volumes before the gate marker. The post-run project volume query was empty;
`aster` and `build-host-langfuse` remained running. No production/test edits followed
the gate. The executable manifest compared 248 local/Build-host files with zero
mismatches. The verified source archive precedes final receipt wording only.

| `evidence/rv-f0541226/` artifact | SHA-256 |
|---|---|
| `red-source.tar` | `4a20f075c64d9efddb1f2d0587e000e47f02f3686727b47a44cebc6f04c6b938` |
| `red.log` (both real assertion failures) | `8790f69ff35aa6fda3c87817f32a2aad1278848a096b71b19e63c95866080051` |
| `focused-green.log` | `66b31a2ce7a1129fae35b8d963609ad51d6273d7be53b0ff5824e9b04ae43287` |
| `focused-source.tar` | `3ceef86da6e769bd9e7a5a14a3f15c934703bd8ac6e9ccdf3ab1b8c4f8a5f85a` |
| `verified-source.tar` | `5435b8cfba4354e2e8fec9bdc527394eee9adb9d157d8fd1cbea7b8929112ce2` |
| `executable-manifest.json` | `c9e832410addbea6c8d4d0fde5e841cd6ca058e94481865b6229ab8843be2a21` |
| `gate.log` | `ce3ac2b3fb29f3a4b941b844ee84ba2462e3fdb6143d9298ed8b9f6c998b2638` |
| `platform-test.log` (unchanged prerequisite failure) | `063349d4b7114838c71f45a49cd683076fddc40da75ce52c11702d338db9282f` |
| `platform-test-trace.log` | `5e5bfde33ce97c5cb775c235582db6f734fcb595b785955e99991db7108f820b` |
| `platform-docs.log` (same 14 broken targets) | `1aad44bd0bdb721a81f642cb0ed97c7785c661eccaeea5727ddaa710b19d2ebe` |

The existing three platform coordination files are synchronized. Their Build-host
`just test` rerun still stops at the missing host-config reference; `just
docs-check` still reports the same 14 pre-existing broken targets. The traced
failure and logs above confirm those separate blockers, not an Aster gate failure.

No agents, source commits, pushes, real credentials or live deployment. Q6 remains
an explicit blocked live-target handoff; S4 scope is unchanged. Independent review
was performed; these fixes do not imply a second reviewer approval. Prior receipts
below are historical and are not overwritten or relabelled.

### S6 authorized continuation, 2026-10-02

The user authorized remaining catalog reliability in the same Aster task worktree,
with observed fixture RED before production, Build-host-only execution, existing secret
resolution, preserved S2 identity and no nested agents, commits, pushes or deployment.
Q6 is an explicit blocked live-target handoff, not a coding prerequisite. S4's
optional Cube output is still undecided; read-only Cube metadata reliability is S6.

Source/evidence are isolated under
`build-host:/home/developer/aster/worktrees/odcs-s6-20261002/{source,evidence}`. S6 now has
its own target directory. The initial run reused the earlier S5 build cache;
that external directory disappeared after CI and S1/S2/S3 Compose passed, causing
`gate-first.log` to fail before S7 Compose with `Not a directory`. Its active
fixture stack had already cleaned up. The S6-owned dangling link was replaced by
a dedicated directory; earlier source/evidence were not altered. Both S5 final
archive/log hashes were rechecked and still equal the historical receipt below.
This infrastructure failure is not RED or S6 completion. New logs are append-only.

| Observed pre-fix failure | Implemented correction |
|---|---|
| HTTPS Iceberg capture returned `old`, expected `current` | Select by current schema ID; explicit v1 legacy path; missing/duplicate/unsupported identity refuses |
| HTTPS OM namespace list returned 1 row, expected 2 | Provider-specific namespace/table pagination, encoded cursors/filters and complete-result failure on limits/errors |
| HTTPS Cube 401 became successful empty metadata | Shared checked, streamed transport; bounded aggregate bytes/reads/deadline; redirects and malformed shapes refuse |
| Actual server sent a reference instead of the selected secret | `secret://KEY` through existing selected `SecretStore`, central registration and exact target token wiring; missing selected-store key refuses startup |
| Malformed Polaris OAuth input silently became anonymous | Preserve invalid configuration until validation and fail before IO |
| Review fixtures accepted a missing Iceberg schema type, OM namespace without FQN, and empty OAuth token | Three additional observed assertion failures in `review-shapes-red.log`; require struct schema shape, preserve only fully qualified OM namespace identity, and refuse empty token responses before metadata IO |

The initial credential fixture used the wrong development identity header and
received 403; that setup failure is **not RED**. Corrected `credential-red-2.log`
observed the missing secret-resolution assertion before its production fix.
TLS captures trust a generated local CA; certificate verification stays enabled.
Server registration and Compose use isolated HTTP fixture services, not live
provider targets. BDD was bound after Rust assertion RED; no earlier BDD RED is
claimed. Additional cursor, streamed-body, OAuth and controller controls extend
the original failing fixtures.

S5 port inspection: `table_schema_bounded` must account for caller-specific total
source bytes/reads, including auth and pagination. The ordinary browse cap does
not implement that variable allowance. S6 does not add an AI provider capability;
the three providers retain explicit unavailable/zero-IO behavior, asserted by the
fixture. No silent unbounded AI fallback or abandonment of required S6 behavior.

Fresh direct review passes cover conformance, correctness, security, reliability,
resource bounds, architecture, compatibility, tests, operations and simplicity.
Accessibility is unchanged (no UI changes in S6). The malformed-credential
fallback was reproduced and fixed; prior ready-token precedence remains compatible.
No nested or independent reviewer was used; independent S6 RV is the handoff.

The first implementation was frozen before review as `first-implementation.tar`,
SHA-256 `c4efa417f960c0252d19143abc9f2ecaefd46e134937bf192bd223ad4933b095`.
Initial RED source archive SHA-256:
`6b34f964b30034cdb5899206d7d9fa37685a094fe3979194be69876f0ff2b81e`.
These are checkpoints, not the final verified candidate.

The review corrections passed all three adapter tests on Build-host. Negative-marker
proof then changed the expected current column to `S6_GATE_SENTINEL`: the canonical
gate failed its actual fixture assertion and printed no S6 success marker.
The driver restored exact source bytes, SHA-256
`bfd3246872d6549d6de163bc9915634d8efc0868f331b4393116b96346e94688`, before
the final gate. The earlier focused server credential test also passed.

**Final implementation verification:** `devenv shell -- just
odcs-catalog-quality-validation` completed on Build-host from the isolated source,
using `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.
The recorded shell first ran formatting/focused review checks and the negative
marker driver, then the canonical gate. Exact S6 report: **4 tests, 1 bound
scenario, 1 step, zero S6 skips**. Provider/routing/Generic Table checks, full CI,
S1/S2/S3/S5/S7 regressions, offline validation, PostgreSQL persistence and browser
checks all passed. S5 retained **12 tests and 3 scenarios/3 steps**.

All six isolated Compose markers passed: S1/S2/S3/S7/S5/S6. The S6 fixture checked
registered target authentication, complete pagination, controller reconciliation
of all three authenticated providers, and startup refusal with the empty selected
memory secret store. Final project `aster-odcs-s6-ed82bd80aa0e` removed its
containers, network and volumes before the S6 success marker; the independent
post-run volume query was empty. `aster` and `build-host-langfuse` remained running.
Existing unrelated external-service ignores remain; no S6 test is ignored.

No production/test changes followed the final gate. The executable manifest
compared **248 files**, with zero local/Build-host mismatches. The source archive
predates final receipt/status wording; its implementation is the verified code.
No source commits, pushes, agents, real credentials or live provider requests.
Independent S6 review is next. No required S6 implementation scope was abandoned.

| S6 evidence artifact | SHA-256 |
|---|---|
| `verified-source.tar` (before final receipt edits) | `cddb2f5d8420f7b1c71414ab0b9625a742dff26e232b5afbcdc199b84744d1a3` |
| `executable-manifest.json` (248 exact files) | `ef3136bd09c341974f0cd411c0a824740aa7202afd3bb2c848e4bd1f6ef73fe6` |
| `gate-final.log` | `d71790fb57de12707b5883ec0038fc849c5c825ce6a1500c1982c8d6a4096308` |
| `quality-red.log` | `2f692eb4db67f1a62f600be223e988456c0029007f798b82dd6da23a5a7429ca` |
| `credential-red-2.log` | `5f26e37ca0306e26c1ac97faecb18e36a10dc8eba4eeb1591c79488b77c15f5f` |
| `malformed-credential-red.log` | `16e66786521999ae26e2f8d7d2ee776de13a05b824077bf633b68dd5f8ca166e` |
| `review-red-source.tar` | `126e6fdfdc87ff59222788c29ef3cf10a30413d349ca3480aa7ad05f62175add` |
| `review-shapes-red.log` | `22014b30a42374966748179e74c5a705d6a502c02203ce0af192f65059804be0` |
| `marker-negative.log` | `62305e9bf442e4d4fc6ef5d90f99625767f64d18e03119c1143fc116d7f10873` |
| `gate-first.log` (cache disappearance; not completion) | `efe9dfdf10367d203d9b6ed986de8c6d29fc2c922c7ef12867f461b28698c7ef` |
| `platform-test.log` (prerequisite failure) | `063349d4b7114838c71f45a49cd683076fddc40da75ce52c11702d338db9282f` |
| `platform-test-trace.log` (missing host-config link) | `5e5bfde33ce97c5cb775c235582db6f734fcb595b785955e99991db7108f820b` |
| `platform-docs.log` (14 existing broken targets) | `1aad44bd0bdb721a81f642cb0ed97c7785c661eccaeea5727ddaa710b19d2ebe` |

Coordination synchronization is applied to the existing platform proposal, index
and README in `platform/worktrees/aster-odcs-plan`. On its isolated Build-host copy,
`just test` still fails at the required `references/repos/host-config` symlink
(confirmed by the traced check), and `just docs-check` still reports the same
14 pre-existing broken targets. The source task worktree also lacks that reference
directory. No fake links or unrelated document repairs were introduced. Final
receipt wording changes no link targets. These remain explicit coordination
blockers; the Aster implementation gate is not an all-repository GREEN claim.

**What waits:** Q6 live activation is blocked on an explicitly authorized provider
installation, target-specific nonhuman identity and credential handoff through the
existing mechanism. No real target/credential was supplied or used. S4 optional
Cube export scope remains undecided. Neither item blocks this fixture-backed S6
implementation, and neither is silently abandoned. A faulty provider can be
disabled without reopening protected metadata, Generic Table enablement or S5's
bounded-context guard. No additional human decision was needed for this implementation gate.

### Independent S5 RV ses_f05b08380ffePmVosl4XKql3ZF: two P2 corrections

The user requested these two verified fixes before any S4/S6 work, with assertion
RED before production edits, Build-host-only execution and no agents/publication.

| Finding | Observed RED | Correction and focused GREEN |
|---|---|---|
| Cell send denial retained cached replies and replacement buttons | A real development-role revocation returned 403 from `SendMessage` while the already loaded panel retained two articles | One `invalidateCellChat` function serves failed reads and sends, advances the read generation, clears transcript/cache/revision and removes replacement controls; composer and stored exchange remain unchanged. Browser proof also releases an earlier allowed read after denial and verifies it cannot restore the cache. No helper call occurs on the revoked send. |
| Ninth distinct selection could persist unreadable history | Eight distinct selections plus a successful repeated-selection control were followed by a ninth distinct selection returning 200 instead of 400 | The same pure dependency-limit check validates stored history and the prospective `with_exchange` result before helper IO. Limits are eight distinct contract/object selections and 32 observation catalogs across history plus the proposed exchange. Repeats consume no new slot. The ninth selection now refuses with zero helper calls, unchanged revision/messages and readable prior history; a subsequent repeat remains allowed. |

Both RED failures were observed before either production correction. The focused
Build-host run passed the new route regression and `just conversation-browser`.
The browser denial uses the existing isolated development identity seam; it is
not a deployed OIDC revocation claim. Contract admission remains covered by the
certificate-verified HTTPS route suite. The new limit regression is also called
by the bound reference scenario; the exact gate is now **12 tests / 3 scenarios /
3 steps / zero S5 skips**. No new independent sign-off is inferred.

Existing isolated source: `build-host:/home/developer/aster/worktrees/odcs-s5-20261001/source`.
Review-specific receipts: sibling `evidence/rv-f05b0838/`. Before changing the
remote source, `before-source.tar` preserved the reviewed checkout with SHA-256
`c7c48b9ce7119aba148553bdb0018c205cb042c6f12ac80663b3620288963658`.
Earlier S1/S2/S3/S7 and S5 completion archives/logs were not overwritten.

Final verification (2026-10-02 UTC), from that isolated source directory, used
declared devenv and `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0`:

- `cargo test -p aster-server --test odcs_ai_context proposed_exchange_respects_unique_dependency_limit -- --exact`
  under the isolated S5 PostgreSQL wrapper: assertion RED, then **1 passed**.
- `just conversation-browser`: assertion RED, then **GREEN**, including the real
  revoked send, retained storage/composer, zero helper calls and delayed-read fence.
- `devenv shell -- just odcs-ai-context-validation`: **GREEN — 12 tests,
  3 scenarios, 3 steps, zero S5 skips**. AI registration, PostgreSQL ledger
  persistence, full CI, browser and S1/S2/S3/S7 regressions all passed. Isolated
  Compose passed S1/S2/S3/S7/S5; final project `aster-odcs-s5-c7edd543c307` removed
  its containers/network/volumes before the success marker. Existing `aster` and
  `build-host-langfuse` remained running. Existing unrelated external-service ignores
  remain; no source commits, pushes, deployments or agents were used.

No production/test edits followed this gate. Only documentation/receipt wording
was reconciled afterwards. This resolves the two reported findings, not a new
independent approval. The known coordination prerequisite/link failures remain
separate from the Aster gate.

| `evidence/rv-f05b0838/` artifact | SHA-256 |
|---|---|
| `verified-source.tar` (before final receipt edits) | `7edadd858847ce5ac5151a10505a878cdead72452107f6dbfd89edd4cb6367f0` |
| `gate.log` | `c3eb59735a888dd45a9882359b75ad3313512092f9df002eb5f6b07a3f2bd3ef` |
| `dependency-red.log` | `eabdfd56baf6e56b453069185615f0f5472bebffc72eaaf1b17aad0e272a7b44` |
| `cell-red.log` | `a87c4192cc842e2e07271cfccfa3a014b799f786f3bb670418184d88aa71daa5` |
| `focused-green.log` | `d738a028029fc7276eb042b2e49dec268a7811a391ea0d461da03c1026cd7f1e` |
| `platform-test.log` (unchanged prerequisite failure) | `063349d4b7114838c71f45a49cd683076fddc40da75ce52c11702d338db9282f` |
| `platform-docs.log` (same 14 broken targets) | `9b1cad2c1438fc3503654d708b3b716a720d8da22a4b69645f40c4e8f10d22fb` |

### Pre-independent-review completion receipt, 2026-10-02 UTC

The completion artifact is **`s5-complete-source.tar`** and the final gate log is
**`s5-gate-complete.log`**, under
`build-host:/home/developer/aster/worktrees/odcs-s5-20261001/evidence`.
The nine/ten-case archives below are evidence of earlier review checkpoints,
not rollback targets or completion claims.

From the isolated `source` directory, declared devenv with
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`
ran `just odcs-ai-context-validation` successfully. Exact final report:
**11 tests, 3 bound scenarios, 3 steps, zero S5 skips**. The gate also passed
AI registration, real PostgreSQL reconnect/persistence for both dependency ledgers,
browser checks, S1/S2/S3/S7 regressions, full `just ci`, and isolated Compose for
S1/S2/S3/S7/S5. Every Compose marker and the terminal
`odcs-ai-context-validation OK` are present. S5 project
`aster-odcs-s5-388585eb1f8d` removed its containers, network and volumes before
success; `aster` and `build-host-langfuse` remained running.

Final authority covers **both contract and catalog dependencies**, including
legacy observation grounding. Every provider attempt uses a fresh upstream ID
and explicitly supplied admitted history, so failed/conflicting attempts cannot
leave implicitly reusable provider context. Local conversation IDs stay stable.
Both additional failure paths were observed RED on actual HTTPS captures before
their production fixes. The final gate includes their executable bindings.

No production or test code changed after this final gate. Only receipt/handbook
and coordination wording was reconciled afterwards. No independent approval,
source commit, push, deployment or agent was used. Coordination remains blocked
as detailed below; this is an Aster verified-scope handoff, not all-repository GREEN.

| Final/checkpoint artifact | SHA-256 |
|---|---|
| `s5-complete-source.tar` | `9272fc33e9bfde6627439387c1e7ece216c5ce2a6ab1ff6227c125106140a9ba` |
| `s5-gate-complete.log` | `2724bcb2fe3498e402844a1dd8ef7a97a2bfd55b5704696630e2682a3ef5e470` |
| `s5-platform-test-complete.log` (final coordination wording) | `063349d4b7114838c71f45a49cd683076fddc40da75ce52c11702d338db9282f` |
| `s5-platform-docs-complete.log` (same 14 broken targets) | `9b1cad2c1438fc3503654d708b3b716a720d8da22a4b69645f40c4e8f10d22fb` |
| `s5-catalog-history-red.log` | `9fa554bdc0f03e077a241aea8d659177a53605d5cf7a8a573816c3fc08a0a825` |
| `s5-catalog-history-green.log` | `a49abd79ba29104dafea13ea0475087f4f76a5d2f1bd7295e065ce8168ff4102` |
| `s5-failed-context-cache-red.log` | `fa1bd1e2d0e4d21f5b28446a8ceea658e042e3e0b213a38cb092b30944445205` |
| `s5-failed-context-cache-green.log` | `02dbb114397a9111be6248b1d25d2fbec522f2bb44dbb24d96c54eb8f7424cd4` |
| `s5-catalog-history-source.tar` (superseded ten-case checkpoint) | `5b663640c7c56f2ea663bb31dd8d928bc3748323887d353e25fbc541f537b93c` |
| `s5-gate-catalog-history.log` (superseded ten-case gate) | `3be092ee995dcc78bbbc1f4fee87e5666b70eb430d3675f08d0e5845c2c29969` |

### Failed-attempt upstream cache isolation, 2026-10-02 UTC

A second, concrete cache case followed from the failure path: a provider can retain
an authorized reference even when its reply is rejected and local history remains
unchanged. Reusing the provider session on a later unselected turn then revives
that failed context after grant revocation. The HTTPS cache fixture observed this
RED: the second request contained no local contract history, yet its returned reply
contained `NET_AFTER_REFUNDS` from the failed attempt.

The request builder now always generates a fresh upstream attempt ID and accepts
no caller-supplied header map. Aster's local conversation ID stays stable; the full
currently admitted message history still accompanies each completion. Existing
private-history, stale/failure and cell-scope assertions remain; the previous
provider-affinity expectation is replaced by independent attempt IDs. This also
isolates context after a storage failure or revision conflict without mutating
saved history. The focused run passed **11 S5 tests and four conversation tests**.
The new case is included in the bound reference scenario and final eleven-case gate.

### Catalog-observation history follow-up, 2026-10-02 UTC

Final admission tracing found that contract provenance alone did not cover an
earlier assistant reply containing admitted catalog observations. A subsequent
selected-contract turn could bypass the legacy global metadata guard, retaining
contract access while replaying a now-denied observation. Actual Build-host route RED
returned 200 instead of 403 after removing the catalog binding, with the same
session, history and cached compiled artifact.

Messages now retain the exact catalog IDs whose observations were actually sent,
independently of their compiled-contract dependencies. History reads/replays
recheck both authorities before any helper request; no catalog IO is needed for
that admission. Both explicit selected observation and legacy unselected grounding
are covered. Missing either ledger is unverifiable history and refuses replay.
Dependency identities share the 256 KiB transcript capacity. The focused target
now has **10 passing tests**, including allowed-observation controls followed by
binding removal/protection with zero subsequent helper/catalog calls. The bound
reference scenario calls these cases, and the gate now includes the PostgreSQL
reconnect assertion for both ledgers. Its ten-case gate passed before the failed-
attempt cache case required the final eleven-case gate recorded above.

### S5 continuation and Q5 acceptance, 2026-10-01

The user explicitly accepted all three Q5 policies in the canonical S5 section;
continuous implementation/RV and conservative numeric defaults are authorized.
Build-host identity was reverified as `developer` on `build-host`. New source and evidence use
`build-host:/home/developer/aster/worktrees/odcs-s5-20261001/{source,evidence}`. Existing
S1/S2/S3/S7 snapshots are retained. No source commits, pushes, deployments or
agents were used. The base remains `eb397d865cf2dc156fda172d50b13c98eda9d9f7`;
implementation is the existing task worktree's uncommitted candidate.

### S5 verified outcome and scope

Both `/api/ai` and `SendMessage` consume the shared exact path/SHA/object builder
using S3 admission/binding preparation. Notebook and cell UI questions attach the
S7 selection identity. Captures cover personal/shared helpers, notebook, cell,
team session and personal workspace paths. Current team membership and grants are
read again for selected context and historical dependencies; reference blocks
are transient, while exact dependency identities and manifest provenance persist.
Revoked, unknown legacy or changed-manifest history refuses replay rather than
dropping exchanges. Failed admission clears client transcript caches, retains
drafts and storage, and fences older in-flight reads.

The semantic projection preserves declared roles/expressions, nested field
definitions, standard `dataGranularityDescription`, relationships, provenance and
explicit bindings. A deterministic HTTPS fixture derives different returned drafts
from selected net/gross definitions and bindings; missing/ambiguous bindings
produce explanations, not guessed SQL. Engine probes forbid automatic execution.
The tests establish context/draft plumbing, not arbitrary model correctness.

Hard limits and accepted refusal/omission behavior are documented in
[AI assistance](../ai.md#authorized-odcs-first-assistance-s5-candidate). JSON
serialization is capped while writing, including escaping. Optional observations
use a bounded catalog port: existing live adapters currently decline it, so their
AI observations are labelled unavailable rather than read without bounds. S3's
ordinary browse/preparation remains intact; S6 still owns live adapter readiness.
Legacy discovery uses only adapters declaring bounded source reads. Cube is not
required; no retrieval framework or service was introduced.

### S5 review fixes and observed RED anchors

Fresh inline conformance, correctness, security, resource, compatibility,
operability, accessibility and simplicity passes were performed without agents.
No independent reviewer approval is claimed.
RED anchors are the route/browser assertions below. Gherkin bindings were added
after those initial assertions; no pre-implementation Gherkin run is claimed.

| Verified finding | RED and final disposition |
|---|---|
| Selected meaning absent from both AI entrypoints | Initial HTTPS route RED missed `NET_AFTER_REFUNDS`; shared builder and exact UI identity now reach actual helper requests |
| Revoked stored context replayed | Allowed control followed by grant removal returned 200 instead of 403; dependency-aware read/replay now refuses before helper calls |
| Mandatory oversized field and legacy reply overflow | Actual route RED returned 200 for multibyte field/reply overflow; bounded construction and shared reply reader now refuse |
| Bundle removal bypassed legacy-history protection | RED returned 200 with untracked history and no configured bundle; unknown provenance now always requires a fresh conversation |
| Standard grain/nested definitions omitted | HTTPS RED missed `dataGranularityDescription`; the projection now uses actual pinned-schema fields and bounded recursive properties |
| Client-selected upstream session could reuse legacy context | Captured repeated session headers were identical; stateless assistance now generates a fresh upstream identifier per request |
| Denied history left browser replies visible | Browser RED retained six articles; notebook/cell caches now clear on failed admission and delayed reads cannot restore them |
| Cold S7 fixture build consumed readiness window | Diagnostic captured compilation still running at timeout; compile first, then start Cargo's exact artifact under the readiness clock |
| Disposable PostgreSQL readiness raced initialization | Connection resets from init-only Unix socket readiness; S5 fixtures and the exercised persistence script use TCP readiness and remove anonymous volumes |

One unchanged conversation regression initially lost its positive schema control
because its static fixture had not declared its zero-byte source-read capability.
The fixture now declares that capability; its original assertions remain intact.
Clippy also found a duplicated support module; both runners now share one fixture
module through the existing support layout.

### Earlier nine-case verification receipt (superseded checkpoint)

On Build-host, from `odcs-s5-20261001/source`, using declared devenv and
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`:

- `devenv shell -- just odcs-ai-context-validation`: **GREEN**. Exact S5 report:
  **9 tests, 3 scenarios, 3 steps, zero skips**. Includes AI registration,
  notebook browser, S7/S3/S2/S1 gates, full `just ci`, and asserting isolated
  Compose for S1/S2/S3/S7/S5. All five Compose success markers and the terminal
  `odcs-ai-context-validation OK` are present in `s5-gate-final.log`.
- S5 Compose project `aster-odcs-s5-80b9fe024ef6` verified packaged binaries against
  Cargo outputs, exercised selected-assist revocation before an unreachable helper,
  and removed its containers/network/volumes before success. The actual positive
  helper captures are certificate-verified HTTPS route tests, not a live-provider
  or deployed OIDC proof. Existing `aster` and `build-host-langfuse` remained running.
- After the full gate, a **test-only** persistence assertion was strengthened to
  verify the nonempty dependency ledger through PostgreSQL reconnect. No production
  code changed. `just conversation-postgres`: **1 passed, 0 failed, 0 ignored**
  (68 filtered); final `cargo fmt --all -- --check` passed. This is an additional
  focused check, not a second full CI/Compose claim. Full CI retains its existing
  unrelated external-service ignores; none of the nine S5 tests is skipped.
- The copied platform task's `just test` remains blocked at the required missing
  `references/repos/host-config` symlink. The initial host bashrc/nounset issue
  was bypassed with `__ETC_BASHRC_SOURCED=1` for the check; a traced run confirms
  the missing-link assertion. `just docs-check` reports **14** existing broken
  relative targets. No fake links or unrelated repairs were introduced.

Evidence is under `build-host:/home/developer/aster/worktrees/odcs-s5-20261001/evidence`.
Archives precede these final receipt/handbook edits. `s5-verified-source.tar`
identifies the full-gate source; `s5-final-source.tar` additionally includes the
subsequently verified test-only persistence assertion and fixture cleanup change.

| Artifact | SHA-256 |
|---|---|
| `s5-verified-source.tar` | `a71063bbca375a4cc4a0e7e90c3487ea76e27a7df3d27a207164c9e2554ff8e7` |
| `s5-final-source.tar` | `9120699f7e0917879b8421ce6d4d04605a4a088824e0985f7415d957d676a647` |
| `s5-gate-final.log` | `5912b40bb86ef3242db472ea1377230d89533764e8cd0ab5be97a62eebad3907` |
| `s5-persistence-final.log` | `23f4042fe15e7494c48eed6bcf6406c167cb5c336cb4c13325b12d3d66895acd` |
| `s5-red.log` | `5e7b6ce9471e95f130e956f52edc4cbae1c5de4ad587393779371135735cdc2b` |
| `s5-history-red-4.log` | `a13d1c9728f4a1bacab1a3225cca90e451d2e4094b21780de3e305fe602a1ffa` |
| `s5-budget-red.log` | `1f79edc4695d8d37073e7cbe29cd93b52474f3be12278375cd9181bdca258331` |
| `s5-ui-red.log` | `2dbb8912a7f71abe7586f799012503e7294b4ca58af39dd347f704ff0d4199e5` |
| `s5-reply-red.log` | `7770d2754ef40148b6f3fa8a9bf1f6d60bcf4c457c084e317235575ac88bbe03` |
| `s5-legacy-rv-red.log` | `5b0e4c16070bc4dc22564fbef93b00d46a1eed56c761be7211e8d80ec21e26ca` |
| `s5-semantic-rv-red.log` | `55a1e6c05525f1336cd9f1e7f1445d54bcf07f28b2d207af7761167f719513b0` |
| `s5-session-rv-red.log` | `6d0774e4d66bfabea3857caa207d4c21680bd87c022e78f3d0f7d3203b70afc9` |
| `s5-history-ui-red.log` | `e437d0363a632d44659915a3ad661a9ba1b2a12892cd00fe9ddbeb091d83d3ae` |
| `s5-history-ui-green.log` | `d64419ec585a07a52a38cb28d2899df29eb7cb71675e80a707f046a2263360c5` |
| `s5-platform-test-trace.log` | `d61887827ea845ca4bcf1f24543ea4f8a20dfc348f505336348514b6ab8c7378` |
| `s5-platform-docs.log` | `f407bad48bc83f593c26eb80db08e5ec44f8dfcf45c8f22704eb6da86ebc9ba8` |
| `s5-platform-test-final.log` (after coordination edits) | `ab8d0e78f6513cf5a3f57e2a914ecbe4532cf87645544981d4b470307efcf23a` |
| `s5-platform-docs-final.log` (same 14 broken targets) | `79a0ab2104bb9387c983246174a6e381314568620f8fced281680fe114c81c8b` |

The earlier inline-review snapshot `s5-gate-fifth-source.tar` is retained with
SHA-256 `65b94ae0a094ae2817509c81cfd5bb53790ddcaf9715dbc27902eae1a7ab40c5`;
its gate failed and it is not the completion artifact. Earlier failed gate and
fixture diagnostics remain in the same evidence directory without being relabelled.

### Independent S7 RV ses_f15b2449fffedU9hRLqXjSsA0x: stale completion draft P2 resolved

Reviewer identified that keyboard completion changes the editor's `.value`
without an input event; refocusing an already focused editor emits no focusin,
leaving the contract review draft stale. Traced all editor assignments: initial
detached `cellTemplate` construction is synchronized on subsequent focus;
completion, notebook assistant insertion and cell assistant replacement are
live mutations. All three now call the same `syncContractDraft` function used
by input/focusin, synchronizing SQL and clearing the previous review. No S5
context integration or completion behavior refactor was added.

Observed Build-host RED before the production fix: actual keyboard Control+Space /
Enter completion produced editor `SELECT` while the draft remained `SEL`.
The regression asserts retained editor focus, synchronized draft without
refocusing, latest SQL in the rendered review, and clearing on the next edit.
This notebook UI test supplies contract RPC fixtures; the separate S7 browser
still tests real admission/router responses. The notebook helper's captured
calls are explicitly empty during manual review; S7 additionally asserts zero
requests to AI, SendMessage and chat-completion endpoints.

GREEN on the existing isolated Build-host S7 snapshot: `just conversation-browser`,
`cargo test -p aster-server --test odcs_catalog_view` (1 passed),
`cargo test -p aster-server --test contracts -- --tags @odcs-s7-bound`
(2 scenarios / 2 steps passed), and `tests/odcs-s7-report.py` (zero skips).
Declared devenv and the existing debug/incremental settings were used. No full
CI or Compose repeat; prior full-gate receipts remain historical. Existing
`aster` and `build-host-langfuse` stayed running; no stack operations, agents,
commits, pushes or independent second approval.

SHA-256 source/receipts under `odcs-s7-20260928`:

| File | SHA-256 |
|---|---|
| `source/crates/server/assets/app.js` | `6b2535894bb50b5a6bc4a5c3e01da46bf25e4d9aa7d7c6e6693a4e9789bf7941` |
| `source/tests/conversation-browser.py` | `2030e76fb0ccfd95046fc40949dcc289efef847b7018d9b3f2d76f7f1a27deb2` |
| `source/tests/odcs-catalog-browser.py` | `7238e1ccf4ec1a82d826b2f674218566da969e807b0eba76d67c7c9f74e27e60` |
| `evidence/s7-rv-completion-red.log` | `b4a6a54d4d06d6c2e58a37db8f869156dc983f03f3ee76153b06012cc6e0cf39` |
| `evidence/s7-rv-completion-green.log` | `4851ce07125aa0e0928e02a8594b755052dd76abd2d5a6e1caa5ecd4ec10f665` |

### S7 continuation: implementation candidate and verification receipt

User authorized S7 after the S3 review correction. All new builds and runtime
checks use `build-host:/home/developer/aster/worktrees/odcs-s7-20260928/source`; logs live
in sibling `evidence`. Existing S1/S2/S3 snapshots and stacks are preserved.
Observed router and browser assertion RED precede the contract context UI;
notebook browser assertion RED precedes focused-cell draft wiring. Browser
checks exercise actual S3 routes, keyboard review, narrow viewport, escaped
metadata, net/gross meaning, missing/defensive ambiguous binding, revocation
and absent denied sentinels. Ambiguity is injected after valid intake only to
test the resolver's defensive state; real ambiguous configuration is rejected.

Direct conformance/security/reliability/accessibility/simplicity passes retain
fresh admission, generation-fenced rendering, text-only metadata and manual
review with zero query requests. The shared S2 physical-link check now uses
the explicit `/catalog/physical` expert route. No independent RV approval is
claimed. First full `just odcs-catalog-ui-validation` passed: one router test,
two bound S7 scenarios/two steps (each runs the browser flow), notebook browser,
S1-S3 gates, full CI and isolated S1/S2/S3/S7 Compose. The S7 project
`aster-odcs-s7-9c2ed8bffd4c` removed its containers/volumes before success;
existing `aster` and `build-host-langfuse` remained running. Earlier external-service
test ignores remain; zero S7 skips. Builds used declared devenv with
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.

After that gate, added the edited-draft clearing assertion, the S7 Build-host guard,
moved the two draft scenarios to their bound owner, and updated descriptions.
Final `cargo fmt --all -- --check`, two S7 browser scenarios/two steps and
`just features` (45 files) passed. These focused reruns do not constitute a
second full CI/Compose run. BDD bindings were added after the observed router,
browser and notebook RED assertions; no claim of a pre-implementation BDD run.

The existing platform task worktree's proposal/index/README are synchronized.
Its isolated Build-host `just test` remains blocked on the first missing required
`references/repos/host-config` link (trace captured); `just docs-check` also
fails on pre-existing missing document targets. No fake links or unrelated
repairs were introduced. This is a coordination validation blocker, not GREEN.

Receipts under the S7 evidence directory (SHA-256):

| Receipt | SHA-256 |
|---|---|
| `s7-router-red.log` | `21bef0faac2b791508d5d11f69ffaef44d8189b78d9f0eff0f5b4c7ab6dd81f5` |
| `s7-browser-red.log` | `69ca464f0a1ac7dadf43adc0593c6b1d2de3ab36fd4a0d9fafa71450dd461ec7` |
| `s7-notebook-red.log` | `d907b13bf04435ff4a0c8066a9cd6f76248c2c44a0eda8759312baddbaf2e2f8` |
| `s7-gate-first.log` | `47429f0b0dde32f6779c55a5323745f1b5a12e4f4419e1faf8bb48c2a1f2cd97` |
| `s7-final-focused.log` | `812df1e4418ad3e9f82ca22424ad674bcb07f1f4994b4339cffd1c0879bf83d3` |
| `s7-source-manifest.json` (260 files, before this receipt edit) | `41610321d874c56b94d1aeee7470f14f91c6a18c7c6388d30949a522bd3d14f9` |

The source manifest identifies the final focused snapshot, not a retrospectively
invented first-gate manifest. Source and evidence remain isolated for review.

### Independent S3 RV ses_f15e0b90effeSHGNwFqVfPuLd7: comparison coverage P2 resolved

The user-relayed reviewer found that `matched`, `declaredOnly` and
`unmappedDeclared` could be empty without failing the existing fixture, and
declared/observed type and nullability differences were unproved. Extended the
existing route fixture with a declared-only physical field and an unmapped field;
the matched field declares `physicalType: bigint`, `required: true`, while its
observation reports `type: decimal(12,2)`, `nullable: true`. Exact full-entry
assertions retain both original facts, semantic metadata and mapping evidence.

Production already satisfied the stronger checks: initial focused run GREEN,
not missing-behavior RED. No production fix was needed. A temporary Build-host-only
mutation driver independently emptied each of the three arrays and changed each
of the four declared/observed type/nullability facts. All seven runs failed the
actual route comparison assertion (one failed test each); the driver restored
production bytes in `finally`. These are mutation failures, not initial feature
RED. Driver and individual `s3-rv-mutation-*.log` files remain in the existing
S3 evidence directory; earlier receipts/manifests were not overwritten.

Final Build-host checks after restoration: `cargo fmt --all -- --check`,
`cargo test -p aster-server --test odcs_resolution` (7 passed, zero skipped), and
`cargo test -p aster-server --test contracts -- --tags @odcs-s3-bound` (5 scenarios,
5 steps passed, zero skipped). All ran in declared devenv with the existing
debug/incremental settings. This test-only correction did not rerun full CI or
Compose; the prior canonical gate remains historical evidence, not a new full
gate claim. No stack operations, S7, nested agents, commits or pushes.

| Receipt / source | SHA-256 |
|---|---|
| `s3-rv-comparison-green.log` | `a41675deb4409b269616ce2a2c3153fee1eb6456e06845058744302f53556580` |
| `s3-rv-mutations.log` | `51821e386631725ab3532769b81f24b65a01ef967ba5257977fa4a257e307677` |
| `s3-rv-mutations.py` | `0c3e3b3183686c97a2a7db5ecaea3ee4ee52dd409f5aebc14546275d2632ac2e` |
| `s3-rv-comparison-final.log` | `a1cedbf7b488bfcb87e46a666a2972abfc6d7bb7d56aa971eb794d9a3f04fa03` |
| `crates/server/tests/support/odcs_resolution.rs` | `c72650ae37f0ac3ddab4ba0e486459f6ed05ceb11de282596ce8e9972938de8f` |
| unchanged `crates/server/src/contract_reads.rs` | `d3717560cd9871032d25bcd5d93fddec4d96d207d3ece50e83a8a4ddb04c4260` |

Disposition: verified test-quality finding fixed. Independent review was performed;
no independent re-review or second approval is attributed to these checks.

### S3 implementation candidate: Build-host full gate GREEN, independent RV ready

User continuation authorizes isolated S3 after image cleanup; shared stacks were
not restored, restarted or modified. Source: the named Aster task checkout,
remote `/home/developer/aster/worktrees/odcs-s3-20260928/source`, receipts in sibling
`evidence/`. No commits, pushes, deployment or nested agents.

Implemented `ListContracts`, `GetContract`, `PrepareContractQuery`, exact
path/digest admission, versioned separate `grants.json`, existing verified-session
and fresh membership authority, exact object-pointer binding resolution, declared
versus observed context/provenance and separate-facts comparison. Unknown teams
and malformed/revoked grants fail closed; catalog bindings confer no contract
access. Preparation has no execution path. Full document reads are authorized
source inspection, not S4 generated exports. Legacy RenderSemantic remains the
restricted legacy path; compiled content is not fed into its lossy renderer.

Observed assertion RED before each production behavior: discovery 404 instead
of 200; preparation 404 instead of 200; binding missing instead of resolved;
observation unavailable instead of observed; unknown-team config 200 instead of
403; missing comparison facts. BDD subsequently binds shared route assertions,
not a separately observed BDD RED. Three additional named regression wrappers
reuse these fixtures; seven test names do not mean seven independent fixtures.

`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
devenv shell -- just odcs-resolution-validation` passed on Build-host: seven S3
tests, five S3 scenarios/five steps, zero S3 skips; catalog routing, complete S2
gate (including offline network-isolated validator and five S2 RV regressions),
complete S1 gate, full CI, and S1/S2/S3 isolated Compose. Existing external-service
ignores remain unchanged. S3 project `aster-odcs-s3-495596118d23` cleaned its
containers/volumes before success. Existing `aster` and `build-host-langfuse` remained
running. Deliberately broken route assertion failed the canonical gate with no
success marker; exact source bytes were restored and verified against the manifest.

Compose exercises the actual server, PostgreSQL-backed existing team policy,
Valkey seeded verified session and disposable HTTPS membership authority. The
fixture needed explicit trusted CA support and correct HTTP body framing; earlier
smoke failures are retained, never counted as passes. TLS verification is enabled.
This is not live OIDC login or production backend policy evidence.

Direct review passes covered security, correctness, reliability, compatibility,
test evidence and simplicity. Verified/fixed unknown-team fail-open configuration
with RED; preserved S1/S2 boundaries and exact source evidence. No independent S3
review session or approval is claimed. Remaining review focus: response wire uses
`contextJson`; production reuses existing team Git authority wiring; comparison
does not decide Q3 drift/export policy. No S7/S5 implementation. Future UI/AI must
use these admitted APIs rather than reusing raw source without admission.

| Evidence file | SHA-256 |
|---|---|
| `s3-access-red-2.log` | `4d216f941e92e05191acb49abbd9537e03cef97266f962d819ac2239aec1e52b` |
| `s3-preparation-red.log` | `6d52e8117f2151bf3f04fbf5f544f58e2770e64b2708baf14706586fa2afdef4` |
| `s3-binding-red.log` | `83f50f70f7553f67f3b8d73c38744540fdb7d13453af242902f54c957292cd67` |
| `s3-observation-red.log` | `b72120ee44591bb4c2ba08eb4d49919f16dd2d6efd129d1dc7841b3a663fb27c` |
| `s3-unknown-team-red.log` | `70fd674d09c91ff0b54c0c28910411bdf918da614cb426afb7ce7af7125ffad5` |
| `s3-comparison-red-team-green.log` | `32743867466abb54c9499829c65e4820f334f0b237fc51a5dcb5961eb6ad4ccd` |
| `s3-marker-negative.log` | `c89162b6fc5031114881d1df0e150a2c7363b7b161731e0cd7834d0ba12e2104` |
| `s3-full-gate-final.log` | `fc4401c4da36706e3f8ac217d3d1389c7877f26dbf0cb810e101c3ae1e841b49` |
| `s3-executable-manifest.json` (236 files equal local/remote) | `b4520af9a4bd4b433a38efcfdb2cacc41f03366888cdc7b14a62d0dc0a64ebbb` |

The draft feature's five S3 scenarios moved to the bound owner after the full
gate; remaining draft scenarios are nine. No scenario count is inferred from the
shape checker. Original S2 review receipt follows unchanged.

Platform three-document synchronization is applied in `aster-odcs-plan` (proposal,
index, README). Its Build-host snapshot `just test` is blocked at the required missing
`references/repos/host-config` symlink; `just docs-check` also reports 14 existing
missing documentation targets unrelated to S3. Logs: `platform-test-trace.log` and
`platform-docs-check.log`. These coordination checks are not claimed GREEN.

### Independent S2 RV ses_f16d503e4ffe5PuK5zPFF0AHpj: all five P2 fixed

User-relayed independent review verified five P2 findings. All are addressed;
no S3 work or independent re-review approval is claimed. This receipt supersedes
the original completion candidate where it described these paths as complete.

| Finding | Verified cause and disposition | Regression |
|---|---|---|
| 1 opaque provider namespaces | Core fallback refuses dotted components while OM/mock emit one opaque dotted component. Provider overrides accept exactly one validated opaque component without splitting; core fallback remains conservative. | Mock `aster_demo.sales` and OM `trino.platform.sales` list/descriptor/schema round trips; multi-component mock request refused |
| 2 browser namespace collapse | Links used display name and handlers called legacy methods. Non-simple namespaces now use a URL-safe encoded JSON array token; handlers decode/validate and use qualified methods and TableRef segments. Simple legacy links remain compatible. | Real router emits distinct links for `[sales.eu]` and `[sales,eu]`; both namespace and table links return 200 |
| 3 segments-only semantic rendering | TableRef kept the empty legacy namespace. Normalize supported single-component namespace; reject dotted/multiple components pending S4 renderer support. | Segments-only `sales` emits `sales.orders`; dotted/multi-component requests return 400 |
| 4 unbounded/nonregular intake | All three filesystem reads were unbounded. Shared reader opens with O_NONBLOCK, checks opened-inode regular-file metadata, enforces metadata/read limits and caps count/aggregate bytes. libc supplies the platform flag, avoiding a guessed constant. | Oversized manifest/document/bindings, document count, aggregate bytes and FIFO at all three read sites; FIFO subprocess checks have a five-second failure bound |
| 5 offline proof outside gate | Canonical recipe now calls the offline driver before success. Driver resolves exactly one test executable from Cargo JSON and runs unshare -Urn with checked exit status and one-test count. Missing/broken isolation fails the recipe. | Recipe ownership assertion plus actual isolated validator execution in final full gate |

Observed RED on Build-host before fixes: four Rust assertions failed (opaque provider
refused, browser links equal, semantic `.orders` identity, oversized manifest
reached parsing); separate Python assertion failed because offline proof was absent
from the canonical recipe. Additional FIFO/aggregate cases broaden the initial
size RED. Earlier receipts and S1 snapshot remain intact.

Final `devenv shell -- just odcs-document-validation` GREEN includes 2 core tests,
6 intake tests, 5 S2 scenarios/steps, **5 RV regression tests**, one network-isolated
validator test, complete S1 gate, full CI, adapter regressions and isolated S1/S2
Compose. Existing external-service ignores remain unchanged; S2/RV tests have
zero skips. Final S2 project `aster-odcs-s2-7e58bcf2f872` cleaned containers/volumes
before success; existing `aster` and `build-host-langfuse` remain running.

Remote evidence under `/home/developer/aster/worktrees/odcs-s2-20260928/evidence`:

| Receipt | SHA-256 |
|---|---|
| `rv-five-red.log` | `cb338d2d03fdfadad4e3ad8812edaa0515a559500110d5ee353f7cc2c5a7c040` |
| `rv-offline-recipe-red.log` | `9e9d0e7d2390bc35023b46d6a5d2c28dd15744ebed0ec96390338d7c0149a67b` |
| `rv-five-final-gate.log` | `a887e72f4d1e8bde103215017321bc4ff9d55d1362ade649be414d349ffb7ea6` |
| `rv-five-executable-manifest.json` | `bc2f8f0d3cb8d09d2a9fb1d371c9d53a76f03925682ab453afe89f58071d18de` |
| `rv-five-discovery.patch` | `26f9cb57162121349636a412f70f3c895e375d90ffdd96cf7a612528977789c7` |

Manifest verifies local executable-source bytes against remote; discovery patch
preserves tracked/untracked candidate before this documentation receipt. Scope:
catalog adapter overrides, server web/API/intake, libc dependency/lock, RV tests,
shared fixture, two offline scripts and justfile. No local build or deployment.

### S2 completion candidate: Build-host gate GREEN

Continuation supersedes the partial receipt below. `devenv shell -- just
odcs-document-validation` passed on Build-host, including exact 2 core tests, 6 intake
tests, 5 named S2 scenarios/5 steps with zero skips, the complete S1 gate (3 tests,
3 scenarios, backend identity, notebook isolation, full CI and S1 Compose), Polaris
adapter regressions and S2 Compose. Existing regression ignores remain (notebook
isolation: 23 passed, 2 external-Postgres cases ignored); S2 has zero ignored tests.

Startup loads configured compiled artifacts in `spawn_blocking`, refuses incomplete
configuration/invalid bundles and keeps the legacy contract vector empty for that
mode. Raw content cannot enter legacy API/UI/semantic/AI projections. S3 admission
is not implemented. Optional `bindings.json` is Aster-owned versioned config pinned
by the manifest; exact artifact path and `/schema/N` object pointer avoid invented
optional IDs. Bindings contain no grants. Namespace arrays cross the core port,
Polaris transport and additive Connect fields; ambiguous legacy dotted requests
are refused rather than split. OpenMetadata retains exact table `name` rather than
truncating its FQN. Paging/current-schema/credential improvements remain S6.

New assertion RED observed before startup, namespace and binding production edits:
invalid configured bundle kept server running; provider namespace segments were
absent; invalid binding config was ignored. Later API transport and positive-binding
assertions broaden these checks; they were not separately observed RED. BDD binds
the shared assertions after their Rust RED, not a separately captured BDD RED.

Final snapshot remains `/home/developer/aster/worktrees/odcs-s2-20260928/source`.
All local executable-source hashes compared equal to remote after formatting.
Receipts in sibling `evidence/` (earlier files retained):

| Artifact | SHA-256 |
|---|---|
| `final-executable-manifest.json` | `41da605d31237a60e8e2c28d5fc881a41eade254b5cbf6fef92b7818d38ab960` |
| `final-discovery.patch` | `53d07e89d3697b5534951a2fed2363ffc6fd6f436e1b3c59def0333b01e6f965` |
| `s2-gate-final.log` | `067cad6f45fd5c929df9249c9f8c84bd161b387a7328518c3724a1df592a7d6f` |
| `startup-red-2.log` | `d4cc92867587b603628cb1f566f97555dea47e08056493c2025b7d7fa4bfd643` |
| `identity-red-startup-green.log` | `7a826c34386bb159dc19ceaa0cd763d8e0b17cafdc38b7daaceb77d5050e8bae` |
| `binding-red.log` | `c984ea19713fb2ad6947a605ecab824948c05e0eb0a0ba90dcc8e0ef052411be` |
| `s2-marker-negative.log` | `fc8957a9fde5bdf1e63f52741dbced8cd1c3bbd2221e7f7c25b654a628937233` |
| `offline-network-proof.log` | `7ef3c7aa5865d6a59dd30852f945e4bc405c318cfa16e67e53f9cd084730dc0a` |

Negative marker proof changed expected document count to 999: assertion failure,
exit 101, no S2 success marker; exact support bytes restored before final gate.
Offline proof: `unshare -Urn target/debug/deps/odcs_intake-56cc7f2cefcaa395
offline_schema_validation_is_distinct_from_support --exact`, one passed test with
network namespace disabled. This additional proof is separate from the recipe.

S2 Compose project `aster-odcs-s2-8b2f9baba353`: health/allowed catalog control,
compiled raw sentinel absent from `/api/contracts` and `/contracts`, content-digest
mutation refuses startup and health, then containers/volumes cleaned before marker.
Exact packaged binaries match Cargo outputs, retaining the S1 RV fix. Existing
`aster` and `build-host-langfuse` remain running. No deployment, commit or push.

Independent RV should inspect the preserved discovery diff, compatibility of
qualified namespace transport, internal-only intake boundary, fixture completeness
and the distinction between Rust RED and subsequently bound BDD. No S3 advance
until that review. Historical policy-blocked/partial statuses below are superseded.

### R9 S2 authorization and remote inspection

The [exact user acceptance](odcs-ai-catalog.md#human-acceptance-r9-s2-selection-rejection-and-binding-ownership)
settles manifest selection, configured-bundle rejection and physical-binding
ownership. Earlier Q1/Q2 blockers below are historical. S2 remains internal;
S3 authorization is not implemented by this step.

SSH identity reverified as `developer` on `build-host`. Existing S1 snapshot/evidence
inspected without modification. The four SHA-256 values for the RV-P2 RED log,
GREEN log, executable manifest and fixed smoke source match the receipt below.
Build-host uses Podman (no Docker on the login PATH); existing `aster` and
`build-host-langfuse` containers are running. No S2 RED/GREEN or gate completion is
claimed by this inspection.

### S2 partial execution receipt (not the slice gate)

Isolated snapshot: `build-host:/home/developer/aster/worktrees/odcs-s2-20260928/source`;
append-only receipts in sibling `evidence/`. Initial 238 source files verified.
No S1 receipt overwritten. All compilation/acquisition/formatting ran on Build-host.

- Core assertion RED: existing parser returned artifact path instead of document
  ID and accepted v3.1. Two failures, zero ignored. New pure `odcs` module retains
  bytes/tree and object-aware projection; both tests now pass. Test seam moved
  from the legacy parser to the separate internal parser, not legacy serialization.
- Intake assertion RED: supplied nested compiled manifest loaded zero documents,
  expected one. Existing loader's own unit test passed in that target. New intake
  module is exercised via the integration test's path import; it is **not wired
  into server startup or AppState yet**. Four intake tests now pass (schema versus
  support, v3.1 rejection, compiled-only intake, explicit selection/pins).
- Exact official schema bytes/license vendored; schema digest matches plan.
  `jsonschema = 0.58.2`, default features disabled, lock generated on Build-host.
  Runtime validation succeeds with embedded schema; an OS network-disabled proof
  remains required. No claim that the broader server cannot access the network.
- Focused Clippy passes with `-D warnings`; `cargo test -p aster-core` passes,
  including existing core behavior runner. Full workspace CI/S1 regression not run.

| Receipt | SHA-256 |
|---|---|
| `red-source-manifest.json` | `c2ccfeb3e780ce19596b568de5b50028407bda00b878781ac994a327600a74d1` |
| `partial-executable-manifest.json` | `7b722bb4bcaf7f0e70861a968a8a734553956f6ea6628807a8b4441122ed8fab` |
| `s2-core-red.log` | `712e3826991f071b1bed5c0c1ec1237c949e25fb5ca61081ac9a25e7c0fc705a` |
| `s2-intake-red.log` | `6fdea1bc06d5be965e409e6b1996de3cf8b78ac3943371273ec7e44a648fb1c7` |
| `s2-core-green-formatted.log` | `7a6a407b42bcdd12e44388534cde7ca1195ff7e71da0d2051df1ce4fdcd8526a` |
| `s2-intake-green-expanded.log` | `3b7d48535aef9d2266adfa7f72d2dc8aaab789a358d80f9729ea72e49d82e5a3` |
| `s2-focused-clippy.log` | `9f30ed71b23359c4d04b7243cd0dadaa9e44b2106c3760a9a4e3eac8d048684e` |
| `partial-discovery.patch` | `72937fadb4228a590652992cc4fed9be00a0a3c26f0036a184c2e226a6fb6108` |

Executable manifest compared each local non-documentation file to remote bytes.
Discovery patch includes tracked and untracked work and predates this receipt.
The RED manifest covers initial core tests; intake test was added afterward, so
it is not claimed as the exact intake-RED source manifest. Remaining work must
preserve exact per-phase manifests rather than relying on this partial receipt.

**Incomplete at session limit:** startup fail-closed wiring, binding config,
lossless core/catalog/proto/API identity, full nested/unsupported fixture coverage,
public disclosure regression, five S2 BDD bindings, exact-count recipe/negative
marker proof, network-disabled validation, full CI/affected regressions and S2
Compose smoke/cleanup. No S2 success marker or independent-RV readiness. Platform
`just test` was attempted locally (read-only shell check) and returned exit 1;
the historical missing-reference prerequisite remains uncorrected.

### Independent S1 RV: target-directory P2 resolved

User-relayed reviewer `ses_f17468f77ffeTjB282W27JUbxH` found one P2:
`tests/odcs-compose-smoke.py` let Cargo honor `CARGO_TARGET_DIR` or
`build.target-dir` while copying binaries from `root/target/debug`, risking a
false green from stale binaries. Fixed with explicit `--target-dir root/target`.
The existing smoke now asserts Cargo's JSON executable paths before copying,
then compares SHA-256 of the selected build outputs, copied build context and
both executables inside the running container. No new test framework or Rust
changes. This receipt supersedes the earlier pending-independent-RV status.

On Build-host, from `/home/developer/aster/worktrees/odcs-s1-20260928/source`, ran:

```sh
CARGO_TARGET_DIR=/home/developer/aster/worktrees/odcs-s1-20260928/conflicting-target devenv shell -- just odcs-compose-smoke s1
```

- RED before adding the flag: build succeeded in `conflicting-target/debug`,
  artifact-path assertion failed, exit 1; no Compose start or success marker.
- GREEN with the flag: Cargo reported both `source/target/debug` outputs;
  packaged bytes matched, health and allowed/denied controls passed, cleanup
  completed, `odcs-compose-smoke s1 OK`, exit 0. Project
  `aster-odcs-s1-9230f10d7d20` left no running containers or volumes. Existing
  `aster` and `build-host-langfuse` stacks remained running.
- Only the affected smoke was rerun. Previous Rust tests, BDD, regressions and
  full CI receipts remain valid for unchanged Rust. S1 executable gate complete;
  no unresolved finding from this review, no new independent re-review claimed.

Receipts under remote `evidence/rv-p2/` preserve the prior manifest and smoke
source without overwriting earlier logs. SHA-256:

| Artifact | SHA-256 |
|---|---|
| `red.log` | `5619fd547b40fa9bd5a6861aea1749bd70a1d5331d32e5c2c6193012eaf6d8df` |
| `green.log` | `3a2d7cf73138d05515a50b3266aaf9e04f5fb8602291ffa6933b5a7adf440ff3` |
| Fixed `tests/odcs-compose-smoke.py` | `d4eff07bb3b2495751486b9560265fb27ae0e697c172588c8c7e1f41470d4e74` |
| `executable-manifest.json` (217 non-documentation source files) | `11481f80eedda6d2a8922fb2fc62bd9d9bde05d1c5e6d75da85855045d85aa84` |
| Packaged `aster-server` | `afb207b6f12116c7970d7be43f6eeb56cde87434f71d5f14106c178fd0d68bb5` |
| Packaged `aster-controller` | `abf4dc3bcc7a160db7af3fb635688a0f7ce0b833de3bfb4032db9bc628313625` |

Only the smoke source differs from the prior executable manifest. The earlier
full-GREEN source hash below remains historical, not a claim that the old smoke
included this fix. No local builds, S2, commits or pushes.

### S1 Build-host execution receipt

This receipt supersedes historical planning-only statuses below. S1 alone is
implemented; 3 bound scenarios pass and 19 later scenarios remain draft. No S2,
commit, push, nested agent or existing-stack deployment occurred. Independent
implementation RV is still required; this is not its sign-off.

Remote snapshot: `build-host:/home/developer/aster/worktrees/odcs-s1-20260928/source`;
receipts are in sibling `evidence/`. Existing `/home/developer/aster` was a non-Git
source copy, not a checkout to overwrite. Its running `aster` and
`build-host-langfuse` containers remained intact. A new task-only directory received
tracked/nonignored local source, excluding secrets, `.git` and build trees.
237 files were SHA-256 verified before the final gate. Tested manifest:
`531e1befe1e7c78edd3993db038878bd49e26956746146fe6c8ef2c1eabc2520`.
Documentation updates after this gate do not change tested code.

| Check on Build-host inside declared devenv | Observed receipt |
|---|---|
| `cargo test -p aster-server --test ai_context_boundary` before production changes | 3 assertion failures: protected SendMessage 200 instead of 403; unbound catalog 10 reads instead of zero; admitted ALPHA_FIRST absent while GLOBAL_FORBIDDEN was sent. HTTPS positive control passed. `s1-red.log` |
| `cargo test -p aster-server --test contracts -- --tags @odcs-s1-bound` before production changes | Exact 3 scenarios / 3 steps failed on the same boundaries; `s1-bdd-red.log` |
| `devenv shell -- just odcs-context-boundary-validation` | Passed: exact 3 Rust tests, 3 scenarios / 3 steps, zero S1 skips; backend-identity and notebook-isolation regressions; full CI; isolated Compose health and allowed/denied API controls; cleanup then both success markers. `s1-final-gate.log` |
| Deliberately change capture-count assertion from 3 to 999, run the same gate | Exit 101, observed left 3/right 999, no success marker. Restored support source byte-for-byte SHA-256 `31dd5352d2f1ce23d5ed0fdd42a8c1a92afb2dd100d942249d6a48947ae39518`. `marker-negative.log` |

Receipt SHA-256:

- `s1-red.log`: `1ad89c33da751669f01c7a07f0a2b50ae5e0443baf9fad972be775f11bcff927`
- `s1-bdd-red.log`: `0cf41f7dd6c44da6d0eed7c33f98c69933dedfb50738a0cba2c7372f02f0989e`
- `s1-final-gate.log`: `94eb6187357a7fe28593cc9a19582ca19a561e798009f63c6e35ba9bdf5fbcf9`
- `marker-negative.log`: `157b88300f896e2adbe2f026ec5ff0cbec6102b14bd6037a88497a49f8ba8fbc`

Implementation: `ai.rs` admits each catalog and omits unadmitted contracts;
`conversations.rs` guards before retrieval/model access and consumes the admitted
snapshot; `api.rs`/`lib.rs` retain that snapshot through workspace admission.
The HTTPS test CA signs a separate server leaf; certificate checks stay enabled.
Connect notebook/cell and legacy `/api/ai` are covered. Team cell rejection stays
400. No separate legacy cell-send HTTP route exists at this base.

Compose uses a unique project/image, random loopback ports and disposable volumes;
existing `just compose-up`/`compose-down` own lifecycle. The smoke packages exact
devenv-built binaries into a disposable Debian image with read-only Nix store
mount, rather than compiling through the existing Dockerfile's separate Rust
toolchain. This proves scoped server integration, not production image packaging.
Final project `aster-odcs-s1-af8eaf0ea241` left no running containers and its volumes
were removed. Existing regression suites retain their pre-existing ignored
external-service tests (notebook isolation: 23 passed, 2 ignored); S1 itself has
zero ignored tests. Full CI passes with its normal existing ignore policy.

Initial attempts were not accepted as RED: the baseline 240-second CI bound
expired during compilation; the initial HTTPS fixture incorrectly used a CA as
the server leaf and was corrected before behavioral RED. The first Compose
context unnecessarily included build output; only that task's compose process
was cancelled and cleanup checked, then the context was reduced to two binaries.
A wrong smoke endpoint and a missed formatted BDD attribute were corrected before
the complete gate passed. No remaining S1 execution blocker; independent RV next.

Platform coordination check rerun: `just test` remains blocked at the existing
missing `references/repos/host-config` symlink (confirmed with `bash -x
tests/contracts.sh`). This read-only shell contract check ran locally; no local
toolchain acquisition, compilation or build-backed tests ran. Both task worktree
diff whitespace checks pass. This coordination prerequisite is not an Aster S1
runtime failure.

2026-09-28 user-relayed independent documentation RV receipt:
`ses_f1782bc2bffeAhfnGJ3S7hiCru`, no blockers. This satisfies the documentation
review prerequisite for S1 only; it is not an implementation sign-off. The user
authorizes S1 in this checkout, assertion RED before production edits, Build-host-only
devenv gates and isolated Compose smoke, then independent S1 RV. No S2, commit,
push or deployment is authorized by this handoff. Historical stage receipts below
describe their original revisions.

Execution correction: "always build on build-host. Cancel builds on my pc". No local
compilation, devenv acquisition, Cargo tests or container builds. Preserve partial
work; inspect remote state and verify an isolated exact-source snapshot's hashes,
excluding secrets, `.git`, ignored files and build trees. Existing remote stacks
are outside scope. SSH verified Build-host/erik; no unrelated processes are stopped.

R8 records the user's "start by docs and them do the rest. Lets do a /rv after
each step" as Q0 authorization. This step updates existing documentation only;
a separate independent RV must precede code, then RV follows every implementation
step before advancing. The accepted r5/r6/r7 behavior and compiled-only boundary
are retained. Remaining Q1/Q2 decisions block S2, not S1. Later product policies
remain open; execution authorization is not blanket policy approval.

Handbook corrections distinguish current flat parsing, bare-name matching,
unvalidated emitters and retrieval/workspace gaps from target contract-first
query building and whole-contract team grants. No runtime fixes or test bindings
are delivered. The feature's r8 header changes only; all 22 scenarios stay draft.
This author's consistency/check pass is not independent RV. The next reviewer
should inspect the r7-to-r8 documentation diff, policy/gate consistency, current
source claims and the [minimum S1 setup](odcs-ai-catalog-ip.md#minimum-next-s1-verification-setup-after-documentation-rv).

R7 records [the authoritative Q8 correction](odcs-ai-catalog.md#human-correction-r7-accepted-q8-outcome):
contract/semantic context materially informs existing manual/AI query building and
reviewable drafting, beyond browse/selection. S3/S7/S5 add three unautomated
API/UI/helper scenarios with selected-definition/binding sentinels, missing and
ambiguous bindings, denied-context absence and ZERO execution calls. Exact UX
and budgets remain open; no new semantic engine, mandatory Cube, universal
executability or guaranteed SQL is implied. Direct consistency review only;
no independent review or implementation approval is attributed to r7.

R6 records [the exact v3.2-only selection](odcs-ai-catalog.md#human-acceptance-r6)
and option description. First-release intake rejects v3.1 via an explicit
apiVersion support gate independent of schema validity; conversion stays upstream
outside Aster. Q1 selection/pins, invalid/unsupported handling, startup/quarantine
and exact errors/statuses remain open. One added unautomated S2 scenario brings
the r6 mapping to 19 scenarios. At r6, Q8's minimum preparation outcome was next.
This is direct consistency review, not a new independent review or runtime proof.

R5 records [the exact human acceptance](odcs-ai-catalog.md#human-acceptance-r5):
whole-contract grants to existing Aster teams, default-deny, independent of
observation and execution permissions. Mapping storage/wire and exact
denial/revocation responses remain design/open as needed; Q8 was undecided at r5.
Four added unautomated scenarios give 18 exact scenario-to-gate mappings.
Direct consistency review only; no extra review agent or independent sign-off.

R4 applies all four findings from the independent r3 review, preserving the
[human correction](odcs-ai-catalog.md#human-correction-r3): full compiled bundles
and contract/semantic-first interaction. The user supplied the session ID and
findings; this revision does not claim a new independent r4 review. Historical
eight-finding review receipts remain below. No implementation was approved at r4.

## What changed

R3 excludes author-tree/compiler integration, separates artifact identity from
physical bindings and preserves full compiled documents. It adds Q7 rather than
inventing authorization for contract-only discovery; observation authorization
and engine grants/backend policy remain separate. At r3, reference compiler v3.1
acceptance/migration remained open; r6 settles rejection and upstream conversion,
not verified v3.2 support. Direct conformance,
security and dependency consistency passes retain urgent S1 and move primary
delivery to S1 -> S2 -> S3 -> S7 -> S5, with S4/S6 supporting branches. Draft
scenarios remain unautomated. R4 moves unresolved Q7/Q8 policy examples out of
the feature into the canonical pending ledger and preserves useful proposed test
seams there and in S3/S7/S5. Q8 remained open until the r7 correction.

### Independent r3 findings applied in r4

| Finding | Verified planning defect / disposition | Remaining gate |
|---|---|---|
| 1: S2 depended on admitted inspection before S3/Q7 | Compiled-intake scenario now exercises internal loading/preservation only; raw retention cannot expand legacy/public projections. Restrict existing projection or deny; authorized inspection belongs to S3. | Q7 and future legacy-disclosure regression |
| 2: Selection/pinning/version support lacked intake prerequisites | Q1 explicitly blocks S2; multi-document same-ID/different-target/version, nested and noncontract/intermediate fixtures use supplied compiled bundle/config only. No overwrite/latest guessing; bundle/document pin mismatches tested. | Exact Q1 selection/support policy remains undecided |
| 3: Denied observation and contract revocation lacked cross-surface seams | Proposed API/UI/AI tests retain authorized meaning with ZERO catalog calls on observation denial; next request/turn after contract revocation stops disclosure, including cached/replayed references. Pending examples live in ledger, not asserted as agreed feature scenarios. | Q7 mapping/response policy and Q8 minimum preparation result |
| 4: S4 could preserve intake yet truncate preview | Actual full-document endpoint test compares the entire parsed tree (original bytes if unchanged), all objects/servers/nested/standard content and offline validity; selected-object Cube is separate. Returning unchanged raw original suffices under full-document admission. | Future S4 endpoint proof; Q7 full-document scope |

All four findings are applied; none discarded. Seven implementation slice gates
remain future/unmet, none abandoned. Planning checks do not prove runtime behavior.

### Frozen r3 receipt (before r4 edits)

Copied all seven current planning/index/feature files with `cp --parents` to
`/tmp/opencode/odcs-ai-catalog-r3-ses_f1ab8829cffefXA3JcQf0narDN` before editing.
SHA-256 values below identify the preserved source revision; no snapshot is tracked.

| Snapshot-relative file | SHA-256 |
|---|---|
| `docs/README.md` | `ce5eb54254f4db81887a1a6e8eb6013b9266f187e723f58d650545b1ade3b602` |
| `docs/plans/example-platform-contract-learnings.md` | `055700ce342607693b0b3440436fae4197525ac1691d91f57339bf30c5aeaaf6` |
| `docs/plans/odcs-ai-catalog-ip.md` | `946814830e67ccc45d8892b87ec3a1540e3410b03046a0cbe250752a1e93bb33` |
| `docs/plans/odcs-ai-catalog.md` | `865623ff8cc7f0d76f948a430e56cb67f98d94b6a8939e0b3cd0350ba0b1f601` |
| `docs/plans/odcs-ai-catalog-pl.md` | `8e653f5261881724f6bc6271e8af57294fa22507aea96526cd6220e7cc3ad0b1` |
| `docs/plans/odcs-ai-catalog-rv.md` | `18ba5e786f33ba6121ba78edd0482193f508429c51949ba38c341b43ae9d0d69` |
| `crates/server/features/odcs-ai-catalog.feature` | `5afbbccfe651b238814722b79b4770559298ebf002b8c9e7d161001fd37fc055` |

Historical author self-review preceded the independent architect review. R2 corrects the
coarse metadata guard, the globally closed protected-policy fixture, feature-level
BDD filtering, attribution, review provenance, undecided examples, identity
sequencing and server smoke requirements. No runtime implementation is included.

The exact five-file first draft is preserved outside the worktree at
`/tmp/opencode/odcs-ai-catalog-r1-ses_f1b194920ffeAVrdTlVZ0G8Rfy`.
[Appendix B](odcs-ai-catalog.md#appendix-b-rv-of-the-planning-candidate) records
each SHA-256 and snapshot-relative path. No duplicated draft is tracked.

## Evidence and limits

### Frozen r7 receipt (before r8 edits)

Preserved the six planning/index files, three affected handbook pages and draft
feature outside the worktree at `/tmp/opencode/odcs-ai-catalog-r7-before-r8`.
This is a local pre-edit receipt, not a tracked new document or independent review.
Core planning and behavior hashes:

| Snapshot-relative file | SHA-256 |
|---|---|
| `docs/plans/example-platform-contract-learnings.md` | `594732a51de8646ef66283e114e97542cd2c33697115c807093915798a362826` |
| `docs/plans/odcs-ai-catalog-ip.md` | `45d0dc839cb7aebb36b93cf19e9fda098f86744dfda69c06f31ba2a4952abeb1` |
| `docs/plans/odcs-ai-catalog.md` | `9511d0daf0c5b44897a41442ec2a6836f89afd7262798580a8a71dd63f8a1d57` |
| `docs/plans/odcs-ai-catalog-pl.md` | `c6f56e00855012ff78785725808e006648891699d543bc7a291a15e2c8f5118a` |
| `docs/plans/odcs-ai-catalog-rv.md` | `b9c46e81029363f883a25036b919e668ed0f2be15d7067e3b95d2068f42b2b4b` |
| `crates/server/features/odcs-ai-catalog.feature` | `60c7ad9fa4cb2d29fe4e7a6fe9d245bc9795ef8c10d718029d79cbf99f183b0d` |

### R8 checks

Local validation on 2026-09-28 in the named task worktrees:

| Check | Result and limitation |
|---|---|
| Aster `just features repo-check` | Passed: 41 feature files and repository contract, including Compose configuration validation; no scenario execution or running stack |
| Read-only Python link/whitespace/manifest check | Passed: 122 relative links/anchors across nine Markdown files, ten file whitespace checks including the draft feature, 22 unchanged scenarios mapped exactly once, seven frozen r3 hashes intact and ten r7 snapshots present; reused `/tmp/opencode/check-odcs-r4.py` with expected count 22 in memory and additional handbook checks |
| Both worktrees `git diff --check` | Passed |
| Platform `just test`; diagnostic `bash -x tests/contracts.sh` | Blocked at the existing missing ignored `references/repos/host-config` symlink, line 55; later gates not reached |
| Platform `just docs-check` | Same baseline 14 broken-link occurrences / 13 missing targets before and after edits; no new target introduced by this sync |

Historical CI remains incomplete: sccache startup timed out before Clippy analysis and tests;
r8 does not rerun full CI or change its configuration. Seven implementation gates
remain unmet, zero met, zero abandoned; documentation RV is a separate pending gate.

### Historical r7 checks

Local checks on 2026-09-28 in the two named task worktrees:

| Check | Result and limitation |
|---|---|
| Aster `just features repo-check` | Passed: 41 feature files and repository contract, including Compose configuration validation; no scenario execution or running stack |
| Read-only Python link/whitespace/manifest check | Passed: 104 relative links/anchors, seven file whitespace checks including untracked files, 22 draft scenarios mapped exactly once, seven frozen r3 hashes intact; reused `/tmp/opencode/check-odcs-r4.py` with expected count changed in memory to 22 |
| Both worktrees `git diff --check` | Passed |
| Platform `just test`; diagnostic `bash -x tests/contracts.sh` | Same missing ignored `references/repos/host-config` symlink blocks the gate; later checks not reached |
| Platform `just docs-check` | Same 14 pre-existing broken-link occurrences / 13 missing targets; r7 introduces no new platform link target |

The existing platform proposal/index/README synchronize r7. Direct consistency
review retains r5 authorization, r6 compatibility and historical review receipts.
All 22 scenarios remain unautomated; zero implementation gates are met. Full CI
was not rerun; its historical sccache blocker remains unverified. No agents,
runtime implementation, commits or deployments were performed.

### Historical r6 checks

Local checks on 2026-09-28 in the two named task worktrees:

| Check | Result and limitation |
|---|---|
| Aster `just features repo-check` | Passed: 41 feature files and repository contract, including Compose configuration validation; no scenario execution or running stack |
| Read-only Python link/whitespace/manifest check | Passed: 101 relative links/anchors, seven file whitespace checks including untracked files, 19 draft scenarios mapped exactly once, seven frozen r3 hashes intact; reused `/tmp/opencode/check-odcs-r4.py` with expected count changed in memory to 19 |
| Both worktrees `git diff --check` | Passed |
| Platform `just test`; diagnostic `bash -x tests/contracts.sh` | Same pre-existing blocker: missing ignored `references/repos/host-config` symlink; later checks not reached |
| Platform `just docs-check` | Same 14 pre-existing broken-link occurrences / 13 missing targets; r6 introduces no new link target in platform |

Platform proposal/index/README synchronize r6 in the existing task worktree.
R5 authorization remains accepted; historical review receipts below are retained.
Full CI was not rerun; its historical sccache blocker remains unverified. All
seven implementation gates remain future/unmet. No runtime implementation,
commit, deployment, agent spawning or new independent sign-off occurred.

### Historical r5 checks

R5 checks ran locally on 2026-09-27 in the two named task worktrees. Full CI is not
repeated for this planning-only revision; the historical sccache blocker has not
been rechecked. No independent r5 sign-off or runtime evidence is claimed.

| R5 check | Result and limitation |
|---|---|
| Aster `just features repo-check` | Passed: 41 feature files, `repo-setup OK`, including Compose configuration validation; no scenario execution or running stack |
| Read-only Python link/whitespace/manifest check | Passed: 98 relative links/anchors, seven file whitespace checks (including untracked files), all 18 draft scenario names mapped exactly once, seven frozen r3 hashes intact; reused `/tmp/opencode/check-odcs-r4.py` with expected scenario count changed in memory to 18 |
| Both worktrees `git diff --check` | Passed |
| Platform `just test`; diagnostic `bash -x tests/contracts.sh` | Blocked at line 55: missing ignored `references/repos/host-config` symlink; later checks not reached |
| Platform `just docs-check` | Blocked by the same 14 pre-existing broken-link occurrences / 13 missing targets; no new link target introduced by r5 |

Platform proposal/index/README now synchronize r5 acceptance in
`platform/worktrees/aster-odcs-plan`; existing dirty task work is retained.
Seven implementation gates remain unmet future work, zero met, zero abandoned.

### Historical r4 checks

| R4 check | Result and limitation |
|---|---|
| `just features repo-check` | Passed locally: 41 feature files and repository contract, including Compose config validation; no running stack or scenario execution |
| `python3 /tmp/opencode/check-odcs-r4.py` | Passed: 95 relative links/anchors, seven file whitespace checks, all 14 retained draft scenario names mapped once, seven frozen r3 hashes verified |
| `git diff --check` and per-file `git diff --no-index --check /dev/null FILE` | Passed, including untracked planning files; included in the Python check |

### Historical r3 checks

| R3 check | Result and limitation |
|---|---|
| `just features repo-check` | Passed: 41 feature files and repository checks; shape only, not scenario execution |
| Read-only Python relative-link/anchor check | Passed: 93 relative links across six Markdown files |
| `git diff --check`, per-file `git diff --no-index --check /dev/null FILE`, whitespace scan | Passed for all seven touched files, including untracked planning artifacts |
| Draft scenario-to-gate consistency | Passed: all 17 draft scenario names assigned once to planned slice gates; none executed |

### Historical r2 checks

| Check | Result and limitation |
|---|---|
| `just features` | Passed: 41 feature files; shape only, not scenario execution |
| `just repo-check` | Passed: `repo-setup OK`; includes Compose configuration validation, not a running stack |
| `devenv shell -- just ci` | Incomplete: format check completed, Clippy failed before analysis because sccache startup timed out (exit 2; Cargo/recipe 101). Workspace tests were not reached. |
| Environment acquisition | Also reported `http://cache-host:5000/...narinfo` timeout; shell subsequently entered. This warning is distinct from the terminal sccache blocker. |
| Read-only Python relative-link check | Passed after final evidence update: 86 relative links across README and four planning pages, including Markdown anchors |
| `git diff --check` and per-file `git diff --no-index --check /dev/null FILE` | Passed after removing Markdown hard-break trailing spaces found by the initial new-file checks. All five new files returned exit 1 without diagnostics, denoting new-file differences, not whitespace errors. |

No HTTPS capture, offline ODCS validation, Cube execution, Compose smoke or
deployment was run. The new feature remains draft and unautomated. All seven
slice gates remain future work. Full CI must be rerun when cache infrastructure
is available; a missing prerequisite is not an assertion RED.

## Decisions and corrections

| Finding | Disposition | Remaining gate |
|---|---|---|
| 1: coarse guard admits mixed unbound metadata | S1 now checks every catalog, asserts zero unbound calls, omits unassociable contracts and uses allowed schema control | Future capture tests |
| 2: positive fixture contradicts globally closed protection | Separate protected denial, all-unprotected control and mixed-unbound examples | Future BDD |
| 3: feature-level runner filtering | Separate bound feature per slice, exact runner, named/nonzero scenario assertions before every marker | Future bindings |
| 4: user/agent attribution | Only ODCS-first and reviewed-plan-first recorded as user answers; parent completed platform sync in its dedicated worktree | Integration reconciliation and platform checks |
| 5: review provenance | Exact r1 snapshot plus author self-review and architect session/dispositions recorded | Human review |
| 6: undecided examples | Removed concrete budget and Cube-subset scenarios; kept support/scope invariants and Q4/Q5 | Product answers |
| 7: identity dependency | Lossless core port/adapter/API identity required in S2 before S3; paging/auth stays S6 | Future identity tests |
| 8: server validation | Every slice requires asserting disposable Compose smoke and cleanup, otherwise incomplete | Future smoke |

## What continues / what waits

Feedback applies to `docs/odcs-ai-catalog-plan`, base `eb397d8`, artifact r8;
the independent scope review covers r3 only. Q7's model and Q1's first-release
compatibility are accepted; Q7 mapping/response design, remaining Q1 intake
policy, Q2-Q6 and Q8 UX remain open. Q0 execution is authorized, subject to
independent documentation RV and per-step RV; Q8's outcome is
accepted in the
[canonical ledger](odcs-ai-catalog.md#open-questions-canonical-ledger). The proposed
PR stack is revised and unpublished. S4's comparison/preview scenario moved out
of S7's gate to avoid making primary browse depend on optional output; its server
side-effect test is explicitly in S4. Checkout isolation is an agent choice.
The following parent coordination receipt predates r3; r3 synchronization is a
parent handoff, with no platform edits made in this task. Parent reported sync in
`/home/developer/projects/platform/worktrees/aster-odcs-plan`, branch
`docs/aster-odcs-plan`, base `d792329`: existing 2026-09-14 proposal updated with
an appended 2026-09-27 decision, proposal-index Aster row and docs README updated.
Dirty main remains untouched; integration reconciliation is required.
Platform `just test` is blocked by missing ignored repository references;
`just docs-check` reports 14 pre-existing broken-link occurrences (13 missing
targets); diff check passed. These are parent-reported results, not Aster reruns.

Parent reports `cr` loaded and `tuicr` installed, but no CMUX/TMUX/ZELLIJ/HERDR
environment. Human manual handoff is `tuicr --file docs/plans` from the Aster
task checkout. No session launched and no approval received. Aster CI remains
blocked as recorded above.

## Risks and recovery

No runtime state needs rollback. Keep the immutable first-draft receipt while
revising the candidate. Do not convert the incomplete CI attempt into a green
claim or weaken cache configuration as part of this documentation task. Future
rollback disables unsafe enrichment rather than reinstating global retrieval.
