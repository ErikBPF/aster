# Conversation UI reference

Human seed, verbatim: “open operwebui againt and take some prints on how it is done on a conversation. Functionality is ok, but ui is too clutered”.

## Observed outcome

Reopened the existing local Open WebUI instance at localhost:8082. [Eight captures](index.html) show its composer, integrations menu, searchable skill picker, enabled skill, count indicator, attachments and advanced controls, alongside the deployed Aster notebook panel. Images are unaltered Chromium captures at 1440 × 1000. The SQL Review skill is synthetic; the screenshots show a new conversation without an inference response.

Open WebUI keeps the full skill list inside a popover. Selection adds only an icon/count to the toolbar. Attachment actions use a separate plus menu. Advanced model controls open separately. In Aster, the attachment selector, MCP fieldset, skill fieldset and SQL textarea all precede the message box; with one available skill the input reaches the bottom of the same viewport.

## Recommended adaptation

Keep the message box and Send anchored at the bottom, with history scrolling above. Use a single toolbar: Attach, Skills with enabled count, Tools with enabled count. Use short visible labels rather than relying entirely on unfamiliar icons. Open searchable toggle popovers, bounded to the viewport. Put exact version selection, version history and refresh inside the skill popover; preserve explicit upgrades. Keep attachment details collapsed, represented by removable attachment chips after selection. Keep pending changing-action confirmations visible in the message history, with exact arguments expandable before approval. Apply the same controls to notebook and cell chats.

Keep personal defaults in AI settings. Preserve ACL checks, explicit MCP opt-in, pinned versions and confirmation behavior. This is a layout recommendation, not a functionality redesign.

## Evidence and limits

All eight captures came from the running local UIs. The skill toggle changed from off to on and closing its menu produced a count of one. Browser menu interactions completed successfully. The initial screenshot helper was renamed after its filename shadowed Python's inspect module; no application fix was required.

No Aster production code or deployment changed. No model or live MCP connection is configured in this Open WebUI reference, so tool-server selection and generated-message layout were not verified. The existing UI remains open for inspection. Production layout implementation is the next work item; no additional feature scope is proposed.
