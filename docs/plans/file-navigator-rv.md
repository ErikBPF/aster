# Skill file navigator refinement

## Outcome and why

Human seed: “tree view and file positioning is clunky. Lets try improving. Also adding a number in the end of names is a ugly alternative”.

The file navigator now reflects the package's actual folder hierarchy. Native
collapsible folders group filename-only entries, with indentation and guide
lines. Full paths remain available through accessible names, hover titles and
the selected file's editor preview. Files with matching basenames in different
folders remain distinct. No stored filenames or paths are automatically renamed.

File name, Folder and File content labels no longer contain numeric suffixes.
Internal HTML identifiers remain unique without exposing those identifiers as
labels. File name and Folder share a compact row on desktop. The content area
uses a bounded initial height and can still be resized. Add file sits beside
Files in the navigator, rather than below the entire content editor. On narrow
screens the navigator stacks above the editor and scrolls within a bounded area.

Saving keeps the selected file open, including the first save of a new skill.
Switching versions retains that file when its path exists in the selected
version, otherwise showing instructions. Folder collapse state is retained;
selecting a file expands its ancestors. Native folder disclosure controls and
file buttons preserve keyboard operation, and selecting a file restores focus
to its new navigation element after rendering.

## Review and evidence

The browser regression first failed because nested folders were absent. After
implementation, all 25 browser flows passed. New checks cover nested folders,
matching basenames, unnumbered labels, full-path accessible names, focus,
selection after save, retained collapse state, navigator placement of Add file,
and narrow-screen overflow. Existing version, sharing, import/export, deletion
and MCP checks remain in the suite. Tests now target the visible reference
editor instead of numbered labels.

A fresh independent source review found no verified correctness or accessibility
blocker. Desktop and mobile previews were inspected before building. Cleanup
removed duplicate placeholder/style declarations. First draft:
`/tmp/aster-file-navigator-draft-1.tar.gz`. RED/GREEN receipts:
`/tmp/aster-file-navigator-red.log`, `/tmp/aster-file-navigator-green.log` and
`/tmp/aster-file-navigator-final-browser.log`. Preview screenshots are under
`/tmp/aster-file-navigator-shots/`. Unbound Gherkin remains a behavior record.

## Delivery, limits and recovery

This is a UI change: storage, skill permissions, immutable version semantics,
text-only asset limits and deletion behavior remain as previously accepted.
There is no drag-and-drop filesystem operation; file positioning is edited
through Folder, with the resulting package path visible.

Builds and full CI run on Apollo using the declared devenv. Deployment targets
the authorized local Podman stack at localhost:8081. The current image and
Compose override will be preserved before replacement, and Donsetch must be
removed before its server namespace is replaced, then recreated. Deployed
verification will create and remove only a uniquely named temporary skill.
Final runtime/CI, deployed screenshots and image identity follow after observation.
No commits, pushes, cluster changes or real external tool mutations are authorized
by this refinement.

## Final observed delivery

Apollo `just ai-registry-browser` passed all 25 flows, including the selected
file remaining visible within the mobile navigator and selection after a new
skill's first save. An added first-save assertion was narrowed to its correct
fixture; import assertions remain intact. The initial real-browser attempt
stopped during disposable database creation, before exercising the UI. Readiness
now checks PostgreSQL TCP rather than accepting its initialization-only local
socket. The subsequent `just ai-registry-live-browser` passed save, versions,
sharing, MCP, deletion and restart; `just ci` finished with `CI GREEN`.

Five changed source/test paths matched Apollo and `git diff --check` passed.
Server and controller now run image
`d332251e14d7ea1d8ff2811771287cdba05ada797f790585ddc9630844673fb4`.
The local Compose update succeeded, `/healthz` returned `ok`, and Donsetch's
approved synthetic read passed after its sidecar was recreated.

`FILE_NAVIGATOR_LOCAL_OK` verifies nested folders, filename-only entries,
unnumbered labels, selection after save, mobile overflow and highlight visibility.
Desktop, mobile and dark-theme captures are in the
[current gallery](openwebui-improvements/index.html). These isolate the real
file panel, hiding only the sticky application header during screenshot capture
so it cannot cover the panel. Temporary verification skills were removed;
existing user skills and paths were not changed.

Final receipts: `/tmp/aster-file-navigator-final-browser.log`,
`/tmp/aster-file-navigator-final-validation.log`,
`/tmp/aster-file-navigator-local.log`, `/tmp/aster-file-navigator-deploy.log`,
and `/tmp/aster-file-navigator-source-check.log`. Rollback retains image
`localhost/aster-server:pre-file-navigator` and override
`compose.before-file-navigator.yaml` in the local deployment directory.
All required gates are met. No commits, pushes or cluster deployment occurred.
