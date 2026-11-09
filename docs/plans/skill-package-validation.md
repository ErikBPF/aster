# Skill import/export validation

## Outcome and scope

Human seed: “validate import and export functions”. Validation covers the skill
package boundary and saved-version download, with existing private-draft,
immutable-version and text-only behavior preserved. No new package format,
archive loader, code execution or permission mechanism is introduced.

Verified checks include Markdown with UTF-8 BOM, Unicode text, nested and root
references, historical version selection, unsaved edits, cancelled imports,
superseded reads, metadata stripping, exact byte-limit boundaries, malformed
packages, duplicate/colliding paths, traversal, encoded/binary declarations,
NUL content, file count and total-size limits. A saved export reimports as an
unsaved private draft; explicit save creates a distinct identity at version one.

## Verified failures and repairs

Assertion-failing browser tests exposed malformed package handling and text
validation defects. JSON `null` produced a JavaScript property error. Whitespace
names and Unicode metadata exceeding the server's byte limits could pass import
validation. File.text silently substituted replacement characters for malformed
UTF-8. Export also stayed enabled while a requested version was loading, allowing
the previous loaded version to be downloaded during that transition.

Import now checks package shape, uses fatal UTF-8 decoding, rejects unpaired
Unicode surrogates, checks names for non-whitespace content, and measures metadata
limits in UTF-8 bytes. It validates before offering to discard an unsaved draft.
Export is disabled during version loading; failed loads restore the displayed
saved revision. Export still deliberately uses saved content rather than pending
editor changes. Existing content/path validation and authority stripping remain.

## Evidence and limits

The expanded browser suite passed 31 flows. The malformed-package matrix covers
17 variants; prior escaped-JSON envelope regression remains in the suite.
RED receipts: `/tmp/aster-package-validation-red.log`,
`/tmp/aster-package-validation-matrix-red.log`, and
`/tmp/aster-package-export-race-red.log`. GREEN receipt:
`/tmp/aster-package-validation-green.log`. Final real-server/PostgreSQL round
trip, restart, CI and deployed validation are recorded below after observation.
Unbound Gherkin is a behavior record, not an executed scenario claim.

Text import is not a byte-preserving archive operation. The browser editor
normalizes CRLF to LF; metadata is trimmed on save. UTF-8 BOM input is accepted.
Export preserves stored version text. JSON packages are limited to 256 KiB and
decoded skill content to 32 KiB with at most 16 references. ZIP, binary assets,
URL imports and ownership/grant restoration remain unsupported by design.

## Delivery and recovery

Changes are limited to UI validation/loading behavior and regression coverage;
there is no data migration. Tests and builds run on Apollo through declared
devenv; deployment remains the authorized localhost:8081 Podman stack. Keep the
current image and override for rollback. Deployed verification will create only
uniquely named temporary skills and remove those fixtures afterward. No existing
user skill needs modification. No commits, pushes or external MCP mutations.

## Observed delivery

Apollo ran `just ai-registry-browser` (31 flows) and
`just ai-registry-live-browser` successfully. The latter used the Rust server and
disposable PostgreSQL, exported a historical version, imported and saved it as a
new private identity, then restarted and verified persisted reference contents.
The combined receipt is `/tmp/aster-package-validation-final.log`.

The rebuilt image `localhost/aster-server:package-validation`, ID
`96c484ab90d871781b0010e3ad283eb06dd9fcd9692697d88b7e16ff51f6077f`, is running
on the local server and controller. Health and the Donsetch approved read passed.
Local/Apollo hashes match for the application JavaScript, both browser suites,
and behavior record. `git diff --check` passed.

The deployed browser check passed historical export, Unicode/nested references,
new private identity, authority stripping, and invalid imports preserving edits.
Its temporary original and imported skills were deleted in cleanup. Receipt:
`/tmp/aster-package-validation-local.log`; runner:
`/tmp/aster-package-validation-local.py`. Two harness setup issues were repaired:
the first attempt started before server readiness; the second checked the editor
status instead of the import status. Neither required a product change.

Rollback is retained as `localhost/aster-server:pre-package-validation` and
`<private-runtime-directory>/compose.before-package-validation.yaml`. Restore
the prior override and recreate server/controller/Donsetch together; Donsetch
must first stop because it shares the server network namespace. Database services
and existing user skills were not recreated or altered.

Final Apollo `just ci` completed successfully: formatting, Clippy, workspace
tests, features, repository/provider checks and local context checks all passed
(`CI GREEN` in the combined receipt). No required validation remains pending.
