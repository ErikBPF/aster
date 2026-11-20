"""Browser contract for durable draft controls; API responses are fixture data."""
import json
import os
import re
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

workspace = "a" * 32
checkout = "b" * 32
html = f'''
<div class="nb-bar"><label>Compute<select id="notebook-engine"><option value="fixture">fixture</option></select></label><div id="notebook-data"></div></div><p id="status" role="status"></p><p id="workspace-branch"></p><ul id="workspace-open-tabs"></ul>
<button data-action="save">Save draft</button>
<button data-action="commit">Commit</button><button data-action="push">Push</button>
<button data-action="pull">Pull</button><button data-action="open-pr">Open PR</button>
<button data-action="takeover">Take over</button><button data-action="reconcile">Check remote operation</button>
<label>Commit message<input id="workspace-commit-message" value="Analysis"></label>
<div id="cells"><section class="cell" data-id="c1">
<div class="cell-head"><code class="cell-id">c1</code><div class="cell-actions"><button data-action="delete">Delete</button><button data-action="up">Up</button><button data-action="down">Down</button></div></div>
<div class="cell-gutter"></div><div class="insert-bar"></div><textarea class="editor">SELECT 1</textarea></section></div>
<script type="application/json" id="nb" data-content-revision="draft:old"
 data-durable-workspace="{workspace}" data-checkout="{checkout}" data-checkout-version="1"
 data-branch="aster/alice/checkout/analysis">
{{"id":"one","title":"One","cells":[{{"id":"c1","sql":"SELECT 1","engine":null,"metadata":{{"custom":"keep"}}}}]}}
</script>'''
with sync_playwright() as pw:
    browser = pw.chromium.launch(headless=True, executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE"))
    page = browser.new_page()
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.route("https://aster.test/**", lambda route: route.fulfill(body=html, content_type="text/html"))
    page.goto(f"https://aster.test/workspaces/{workspace}/{checkout}/notebooks/one")
    page.evaluate("""workspace => {
      sessionStorage.setItem('aster-writer:' + workspace, JSON.stringify({token:'writer', epoch:'1'}));
      window.calls = [];
      window.fetch = async (url, options = {}) => {
        const body = options.body ? JSON.parse(options.body) : {};
        window.calls.push({url, body, method:options.method || 'GET'});
        let data = [];
        if (url === "/api/engines") data = [{id:"fixture"}];
        if (url === "/api/catalogs") data = [{id:"fixture"}];
        if (url.endsWith('/OpenWorkspace')) data = {id:workspace, epoch:'1'};
        else if (url.endsWith('/GetWorkspaceState')) data = {layoutJson:'{\"tabs\":[],\"active\":null}', layoutVersion:'0'};
        else if (url.endsWith('/SaveWorkspaceLayout')) data = {version:'1'};
        else if (url.endsWith('/SaveWorkspaceDraft')) {
          data = {version:'2', branch:'aster/alice/checkout/analysis', dirty:true, draftRevision:'draft:new'};
          if (window.holdSave) return new Promise(resolve => { window.releaseSave = () => resolve(new Response(JSON.stringify(data))); });
        } else if (url.endsWith('/CommitWorkspaceNotebooks')) data = {version:'3', branch:'aster/alice/checkout/analysis', dirty:false, head:'commit'};
        else if (url.endsWith('/PushWorkspaceCheckout')) data = {version:'4', branch:'aster/alice/checkout/analysis', dirty:false, head:'commit'};
        else if (url.endsWith('/OpenWorkspacePullRequest')) data = {url:'https://github.com/acme/analytics/pull/1'};
        else if (url.endsWith('/GetWorkspaceDraft')) data = {revision:'draft:new', checkoutVersion:'2', branch:'aster/alice/checkout/analysis'};
        else if (url.startsWith('/api/notebooks/')) data = {content_revision:'legacy', revision:'implicit-commit'};
        return new Response(JSON.stringify(data), {status:200, headers:{'content-type':'application/json'}});
      };
    }""", workspace)
    page.add_script_tag(content=Path("crates/server/assets/app.js").read_text())
    page.evaluate("document.getElementById('notebook-engine').value = 'fixture'")
    page.get_by_role("button", name="Save draft", exact=True).click()
    page.wait_for_timeout(250)
    calls = page.evaluate("window.calls")
    saves = [call for call in calls if call["url"].endswith('/SaveWorkspaceDraft')]
    assert len(saves) == 1, f"Save must use the durable draft API, calls={calls}"
    assert json.loads(saves[0]["body"]["notebookJson"]).get("engine") == "fixture", "durable drafts must retain notebook compute"
    expect(page.locator("#workspace-branch")).to_have_text(re.compile(r"^analysis(?: ·|$)"))
    assert saves[0]["body"]["workspace"] == workspace
    assert saves[0]["body"]["checkout"] == checkout
    assert saves[0]["body"]["expectedDraftRevision"] == "draft:old"
    assert json.loads(saves[0]["body"]["notebookJson"])["cells"][0]["metadata"]["custom"] == "keep"
    assert not any(call["url"].startswith('/api/notebooks') or call["url"] == '/api/state' for call in calls)
    page.get_by_role("button", name="Commit", exact=True).click()
    expect(page.locator("#status")).to_contain_text("committed")
    expect(page.locator('#workspace-branch')).to_contain_text('commit')
    page.get_by_role("button", name="Push", exact=True).click()
    expect(page.locator("#status")).to_contain_text("pushed")
    page.get_by_role('button', name='Check remote operation').click()
    page.wait_for_timeout(250)
    assert any(call['url'].endswith('/ReconcileWorkspaceRemote') for call in page.evaluate('window.calls')), 'read-only recovery control must call reconciliation'
    page.evaluate("window.holdSave = true")
    page.locator('.editor').fill('SELECT 2')
    page.wait_for_function("!!window.releaseSave")
    expect(page.locator('#status')).not_to_have_text(re.compile(r'^saved'))
    page.evaluate("window.releaseSave()")
    expect(page.locator('#status')).to_contain_text('saved')
    page.evaluate("durable.readOnly = true; applyDurableAuthority(); addCell(document.querySelector('.cell')); selectCell(document.querySelector('.cell')); editSelectedCells('copy')")
    expect(page.locator('.cell')).to_have_count(1)
    expect(page.locator('#notebook-engine')).to_be_disabled()
    page.evaluate("durable.readOnly = false; applyDurableAuthority()")
    page.evaluate("run = async () => { durable.busy = true; }")
    page.locator('.editor').press('Shift+Enter')
    page.wait_for_timeout(200)
    assert not errors, errors
    page.evaluate("durable.busy = false; applyDurableAuthority()")
    page.get_by_role('button', name='Close tab one', exact=True).click()
    expect(page).to_have_url('https://aster.test/workspaces/' + workspace)
    index = browser.new_page()
    index.route("https://aster.test/**", lambda route: route.fulfill(body='<p id="status"></p><form id="workspace-create"><label>Workspace name <input name="name" required></label><button>Create workspace</button></form>', content_type="text/html"))
    index.goto("https://aster.test/workspaces")
    index.evaluate("""() => {
      document.getElementById('workspace-create').addEventListener('submit', event => event.preventDefault());
      window.fetch = async () => new Response(JSON.stringify({id:'cccccccccccccccccccccccccccccccc', writerToken:'writer-token', epoch:'1'}));
    }""")
    index.add_script_tag(content=Path("crates/server/assets/app.js").read_text())
    index.get_by_label('Workspace name').fill('Research')
    index.get_by_role('button', name='Create workspace').click()
    expect(index).to_have_url('https://aster.test/workspaces/cccccccccccccccccccccccccccccccc')
    assert json.loads(index.evaluate("sessionStorage.getItem('aster-writer:cccccccccccccccccccccccccccccccc')"))["token"] == "writer-token"
    browser.close()
print('workspace-browser OK: durable draft controls and workspace creation; mocked API')
