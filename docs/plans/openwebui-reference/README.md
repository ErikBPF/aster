# Open WebUI UI reference

User seed: “start a local ui and take some prints for inspiration”.

Started a separate local Podman instance at http://localhost:8082, container
`aster-openwebui-reference`, with named volume `aster-openwebui-reference`.
Open WebUI reports version 0.13.0. Image digest:
`sha256:922e4b9ae9d3060c0181bf7aaddb98c5cb0e7d1739c2935c3d80baab2fd3c2f9`.
This runtime image is identified independently of the earlier source research.

[Open the screenshot gallery](index.html). Five unaltered Chromium captures at
1440 × 1000 cover the skill library, populated editor, sharing dialog,
integrations settings, and MCP connection form. The SQL Review skill is synthetic
reference content. No model credentials or live MCP endpoints were configured.
Authentication is disabled only for this loopback-bound reference instance.

Observed patterns to adapt: compact searchable registry rows; a file tree beside
the Markdown editor; access controls separate from content; grouped connection
categories; endpoint, authentication and grants together in the MCP form.
Aster should retain explicit version pins and make private/team/organization
scope visible. Open WebUI's low-contrast labels should not be copied blindly.

Evidence: local `/health` returned 200, browser-created sample skill persisted,
and all five images were captured from actual rendered pages. These are visual
references, not MCP execution or inference tests. Initial capture retried after
a network-idle timeout; the MCP type control toggles directly rather than opening
a menu. Neither issue required an application change.

Aster at localhost:8081 remains running. Stop this reference instance with
`podman stop aster-openwebui-reference`; restart with
`podman start aster-openwebui-reference`. Its named volume retains the sample.
No Aster production code changed. External tool side-effect approval remains
an open decision in the planning record.

## Conversation controls follow-up

[Conversation screenshots and Aster comparison](conversation/index.html), with
[observations and the recommended compact layout](conversation/README.md).
