"""Browser check: team Save is local and Sync is a separate scoped action."""

import json
import os
from pathlib import Path

from playwright.sync_api import expect, sync_playwright


html = """
<span id="status"></span>
<button type="button" data-action="save">Save</button>
<button type="button" data-action="sync">Sync</button>
<select id="helper"></select>
<button type="button" id="chat-toggle" aria-expanded="false">Chat</button>
<aside id="chat-panel" hidden><button id="chat-close">Close</button>
  <button id="chat-expand" aria-pressed="false">Expand</button>
  <div id="chat-history"></div><p id="chat-status"></p>
  <form id="chat-form"><textarea id="chat-prompt"></textarea>
    <textarea id="chat-context"></textarea><button id="chat-send">Send</button></form>
</aside>
<div id="cells"><section class="cell" data-id="c1">
  <div class="cell-head"><select class="engine"><option value="">default</option></select></div>
  <textarea class="editor">SELECT 1</textarea>
</section></div>
<script type="application/json" id="nb" data-content-revision="old" data-team="alpha" data-workspace="session">
{"id":"base","title":"Base","cells":[{"id":"c1","sql":"SELECT 1","engine":null}]}
</script>
"""

with sync_playwright() as pw:
    browser = pw.chromium.launch(
        headless=True, executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE")
    )
    page = browser.new_page()
    page.set_content(html)
    page.evaluate("""() => {
      window.calls = [];
      window.fetch = async (url, options = {}) => {
        window.calls.push({url, method: options.method || 'GET',
          headers: options.headers || {}, body: options.body || null});
        const data = url === '/api/catalogs' || url.startsWith('/api/engines') ? [] :
          url === '/api/llm' ? [{id:'go', model:'go', scope:'personal', ref:'personal/go'}] :
          url === '/api/state' ? {} :
          url.endsWith('/helper') ? options.method === 'PUT'
            ? {helper:JSON.parse(options.body).helper} : {helper:null} :
          url.endsWith('/GetConversation') ? {revision:'0', messages:[]} :
          url.endsWith('/SendMessage') ? {revision:'1', messages:[]} :
          url.endsWith('/sync') ? {status:'synced', remote_revision:'remote-sha'} :
          {revision:'local-sha', content_revision:'new-content'};
        if (window.holdSave && url === '/api/teams/alpha/notebooks/base' && options.method === 'PUT') {
          window.holdSave = false;
          return new Promise(resolve => {
            window.releaseSave = () => resolve(new Response(JSON.stringify(data), {
              status: 200, headers: {'content-type':'application/json'}
            }));
          });
        }
        if (window.holdSync && url.endsWith('/sync')) {
          window.holdSync = false;
          return new Promise(resolve => {
            window.releaseSync = () => resolve(new Response(JSON.stringify(data), {
              status: 200, headers: {'content-type':'application/json'}
            }));
          });
        }
        return new Response(JSON.stringify(data), {
          status: 200, headers: {'content-type':'application/json'}
        });
      };
    }""")
    page.add_script_tag(content=Path("crates/server/assets/app.js").read_text())
    page.get_by_role("button", name="Save").click()
    expect(page.locator("#status")).to_contain_text("saved")
    page.get_by_role("button", name="Sync").click()
    calls = page.evaluate("window.calls")
    writes = [call for call in calls if call["method"] in ("PUT", "POST")]
    assert [call["url"] for call in writes] == [
        "/api/teams/alpha/notebooks/base",
        "/api/teams/alpha/notebooks/base/sync",
    ], writes
    assert writes[0]["headers"]["x-aster-workspace"] == "session"
    assert writes[1]["headers"]["x-aster-workspace"] == "session"
    expect(page.locator("#helper option[value='personal/go']")).to_have_count(1)
    page.locator("#helper").select_option("personal/go")
    expect(page.locator("#status")).to_have_text("helper saved")
    page.locator("#chat-toggle").click()
    page.locator("#chat-prompt").fill("Explain")
    page.locator("#chat-send").click()
    expect(page.locator("#chat-status")).to_contain_text("Saved.")
    calls = page.evaluate("window.calls")
    assert any(call["url"] == "/api/teams/alpha/notebooks/base/helper"
               and call["method"] == "GET" for call in calls), calls
    assert any(call["url"] == "/api/teams/alpha/notebooks/base/helper"
               and call["method"] == "PUT" for call in calls), calls
    assert not any(call["url"] == "/api/state" for call in calls), calls
    for method in ("GetConversation", "SendMessage"):
        call = next(call for call in calls if call["url"].endswith("/" + method))
        assert {key: json.loads(call["body"])[key] for key in ("team", "workspace")} == {
            "team": "alpha", "workspace": "session"
        }, call
    page.locator(".editor").fill("SELECT 2")
    expect(page.locator('[data-action="sync"]')).to_be_disabled()
    page.evaluate("window.holdSave = true")
    page.get_by_role("button", name="Save").click()
    page.wait_for_function("!!window.releaseSave")
    page.locator(".editor").fill("SELECT 3")
    page.evaluate("window.releaseSave()")
    expect(page.locator("#status")).to_have_text("unsaved changes")
    expect(page.locator('[data-action="sync"]')).to_be_disabled()
    page.get_by_role("button", name="Save").click()
    expect(page.locator("#status")).to_contain_text("saved locally; Sync pending")
    page.evaluate("window.holdSync = true")
    page.get_by_role("button", name="Sync").click()
    page.wait_for_function("!!window.releaseSync")
    page.locator(".editor").fill("SELECT 4")
    page.get_by_role("button", name="Save").click()
    expect(page.locator("#status")).to_contain_text("saved locally; Sync pending")
    page.evaluate("window.releaseSync()")
    expect(page.locator("#status")).to_contain_text("saved locally; Sync pending")
    browser.close()
print("team-browser OK")
