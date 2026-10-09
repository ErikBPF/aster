"""Disposable real-browser proof for notebook chat; no live provider calls."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from playwright.sync_api import sync_playwright, expect

received = []
class Helper(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        received.append(body)
        reply = "Remembered your question.\n<script>window.chatInjected=true</script>\n```sql\nSELECT 42 AS answer\n```"
        payload = json.dumps({"choices": [{"message": {"content": reply}}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

def unused_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]

mock = ThreadingHTTPServer(("127.0.0.1", 0), Helper)
threading.Thread(target=mock.serve_forever, daemon=True).start()
with tempfile.TemporaryDirectory(prefix="aster-chat-browser-") as work:
    port = unused_port()
    base = f"http://127.0.0.1:{port}"
    env = os.environ.copy()
    for key in ("DATABASE_URL", "ASTER_STATE_URL", "ASTER_OIDC_ISSUER", "ASTER_CONVERSATION_DATABASE_URL"):
        env.pop(key, None)
    env.update(ASTER_BIND=f"127.0.0.1:{port}", ASTER_METRICS_BIND=f"127.0.0.1:{unused_port()}",
               ASTER_METADATA_STORE="memory", ASTER_STATE_STORE="memory", ASTER_CONVERSATION_STORE="memory",
               ASTER_IDP_KIND="none", ASTER_DEV_LOGIN="1", ASTER_NOTEBOOK_DIR=work+"/notebooks",
               ASTER_ENGINES="mock;mock;local", ASTER_CATALOGS="mock;mock;local",
               ASTER_CATALOG_BINDINGS="mock;mock;mock;unprotected")
    binary = os.environ.get("ASTER_SERVER_BIN", "target/debug/aster-server")
    with open(work+"/server.log", "w") as log:
        server = subprocess.Popen([str(Path(binary).resolve())], env=env, stdout=log, stderr=log)
        try:
            for _ in range(100):
                try:
                    urllib.request.urlopen(base+"/healthz", timeout=1).close()
                    break
                except OSError:
                    time.sleep(.1)
            else:
                raise AssertionError("disposable server did not start")
            with sync_playwright() as pw:
                browser = pw.chromium.launch(headless=True, executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE"))
                page = browser.new_page(viewport={"width":1440,"height":1000})
                errors = []
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.goto(base+"/dev-login")
                assert page.request.put(base+"/api/notebooks/chat-check", headers={"If-None-Match":"*"}, data={
                    "id":"chat-check","title":"Chat check","cells":[{"id":"c1","sql":"select 1","engine":"mock"}]
                }).ok
                assert page.request.put(base+"/api/llm/test", data={
                    "base_url":f"http://127.0.0.1:{mock.server_port}/v1","model":"test","api_key":"fake"
                }).ok
                # This UI regression supplies contract responses; S7's router browser
                # separately exercises real team admission and revocation.
                def contract_context(route):
                    method = route.request.url.rsplit("/", 1)[-1]
                    values = {
                        "ListContracts": {"contracts": [{"id": "fixture", "version": "1", "target": "mock", "path": "fixture.json", "sha256": "fixture"}]},
                        "GetContract": {"declared": {"schema": [{"name": "orders"}]}, "provenance": {"path": "fixture.json"}},
                        "PrepareContractQuery": {"declared": {"name": "orders"}, "binding": {"status": "missing"}, "observation": {"status": "unavailable"}},
                    }
                    route.fulfill(json={"contextJson": json.dumps(values[method])})
                for method in ("ListContracts", "GetContract", "PrepareContractQuery"):
                    page.route("**/aster.v1.Aster/" + method, contract_context)
                page.goto(base+"/notebooks/chat-check")
                expect(page.locator("#contract-context")).to_be_attached()
                page.get_by_text("Contract context for manual query review", exact=True).click()
                page.get_by_label("Declared contract", exact=True).select_option(index=1)
                page.get_by_label("Declared object", exact=True).select_option("/schema/0")
                editor = page.locator(".editor")
                editor.fill("SEL")
                page.keyboard.press("Control+Space")
                expect(page.locator(".complete-item").first).to_be_visible()
                page.keyboard.press("Enter")
                expect(editor).to_be_focused()
                expect(editor).to_have_value("SELECT")
                # No refocus: completion must synchronize before review is clicked.
                expect(page.get_by_label("Query draft", exact=True)).to_have_value("SELECT")
                page.get_by_role("button", name="Review draft with context", exact=True).click()
                expect(page.locator("#contract-review")).to_contain_text("SELECT")
                assert received == [], "manual completion/review must not call the helper"
                page.locator(".editor").fill("SELECT 7 AS manual_draft")
                expect(page.locator("#contract-review")).to_be_empty()
                expect(page.get_by_label("Query draft", exact=True)).to_have_value("SELECT 7 AS manual_draft")
                page.locator(".editor").fill("select 1")
                expect(page.locator("#helper")).to_have_value("")
                assert page.request.get(base+"/api/notebooks/chat-check/helper").json()=={"helper":None}
                page.locator("#helper").select_option("personal/test")
                expect(page.locator("#helper")).to_have_value("personal/test")
                page.locator("#chat-toggle").focus()
                page.keyboard.press("Enter")
                expect(page.locator("#chat-panel")).to_be_visible()
                page.locator("#chat-expand").click()
                expect(page.locator("#chat-expand")).to_have_attribute("aria-pressed","true")
                selections = []
                def selected_turn(route):
                    body = route.request.post_data_json
                    selections.append(body.pop("contractSelection", None))
                    # The contract panel above is a UI fixture. Real admitted
                    # selected requests are captured by odcs_ai_context on HTTPS.
                    route.continue_(post_data=json.dumps(body))
                page.route("**/aster.v1.Aster/SendMessage", selected_turn)
                for prompt in ("Remember revenue", "Explain the earlier answer"):
                    page.locator("#chat-prompt").fill(prompt)
                    page.locator("#chat-send").click()
                    expect(page.locator("#chat-status")).to_have_text("Saved. Replies do not execute queries.")
                assert len(received)==2
                assert selections == [{"path":"fixture.json", "sha256":"fixture", "object":"/schema/0"}] * 2, selections
                assert any(m["content"]=="Remember revenue" for m in received[1]["messages"])
                assert any(m["role"]=="assistant" for m in received[1]["messages"])
                expect(page.locator(".editor")).to_have_value("select 1")
                assert page.evaluate("window.chatInjected") is None
                page.reload()
                page.locator("#chat-toggle").click()
                expect(page.locator(".chat-message")).to_have_count(4)
                # A slow earlier read must never roll back a completed send.
                page.locator("#chat-close").click()
                held = []
                def delay_first(route):
                    if not held:
                        response = route.fetch()
                        held.append((route, response))
                    else:
                        route.continue_()
                page.route("**/aster.v1.Aster/GetConversation", delay_first)
                page.locator("#chat-toggle").click()
                for _ in range(50):
                    if held:
                        break
                    page.wait_for_timeout(20)
                assert held
                page.locator("#chat-close").click()
                page.locator("#chat-toggle").click()
                expect(page.locator("#chat-send")).to_be_enabled()
                page.locator("#chat-prompt").fill("A third question")
                page.locator("#chat-send").click()
                expect(page.locator(".chat-message")).to_have_count(6)
                held[0][0].fulfill(response=held[0][1])
                page.wait_for_timeout(150)
                expect(page.locator(".chat-message")).to_have_count(6)
                page.unroute("**/aster.v1.Aster/GetConversation", delay_first)
                page.get_by_role("button",name="Insert SQL as new cell").first.click()
                expect(page.locator(".cell")).to_have_count(2)
                expect(page.locator(".editor").last).to_have_value("SELECT 42 AS answer")
                expect(page.locator(".out-body")).to_have_count(0)
                page.locator("#chat-prompt").fill("unsent draft")
                page.locator("#chat-close").click()
                expect(page.locator("#chat-panel")).to_be_hidden()
                page.locator("#chat-toggle").click()
                expect(page.locator("#chat-prompt")).to_have_value("unsent draft")
                # Tab accepts only the segment after the last dot, so the
                # qualifier survives: `...aster_demo.ord` -> `...aster_demo.orders`.
                editor = page.locator(".editor").first
                editor.click()
                editor.fill("select * from mock.aster_demo.ord")
                expect(page.locator(".complete-item").first).to_be_visible()
                page.keyboard.press("Tab")
                expect(editor).to_have_value("select * from mock.aster_demo.orders")
                # The cell AI action opens a scoped inline panel that measures
                # at most 30 percent of the cell, and closes again.
                first = page.locator(".cell").first
                expect(first.locator(".cell-chat")).to_be_hidden()
                first.locator('[data-action="ai"]').click()
                expect(first.locator(".cell-chat")).to_be_visible()
                expect(first.locator(".cell-chat-history")).to_be_visible()
                page.get_by_text("Contract context for manual query review", exact=True).click()
                page.get_by_label("Declared contract", exact=True).select_option(index=1)
                page.get_by_label("Declared object", exact=True).select_option("/schema/0")
                expect(page.locator("#contract-preparation")).to_contain_text("orders")
                first.locator(".cell-chat-prompt").fill("Explain this cell")
                first.locator(".cell-chat-send").click()
                expect(first.locator(".cell-chat-status")).to_have_text("Saved. Replies do not execute queries.")
                expect(first.locator(".cell-chat-history .chat-message")).to_have_count(2)
                assert selections[-1] == {"path":"fixture.json", "sha256":"fixture", "object":"/schema/0"}, selections
                first.locator(".cell-chat-prompt").fill("unsent cell draft")
                ratio = page.evaluate("""() => {
                    const cell = document.querySelector('.cell');
                    const chat = cell.querySelector('.cell-chat');
                    return chat.getBoundingClientRect().width / cell.getBoundingClientRect().width;
                }""")
                assert ratio <= 0.30, ratio
                # RV P2: revoke send admission while the cell remains open and
                # loaded. An earlier allowed read must not restore its cache.
                cell_body = {"notebook":"chat-check", "cell":"c1"}
                cell_stored = page.request.post(base+"/aster.v1.Aster/GetConversation",
                    headers={"Connect-Protocol-Version":"1"}, data=cell_body).json()
                expect(first.get_by_role("button", name="Replace this cell", exact=True)).to_have_count(1)
                pending_cell_read = []
                def hold_cell_read(route):
                    if route.request.post_data_json.get("cell") == "c1" and not pending_cell_read:
                        pending_cell_read.append((route, route.fetch()))
                    else:
                        route.continue_()
                page.route("**/aster.v1.Aster/GetConversation", hold_cell_read)
                page.evaluate("() => { void loadCellChat(document.querySelector('.cell')); }")
                for _ in range(50):
                    if pending_cell_read: break
                    page.wait_for_timeout(20)
                assert pending_cell_read
                assert page.evaluate("cellChatState('c1').loaded")
                helper_before = len(received)
                editor_before = first.locator(".editor").input_value()
                page.context.add_cookies([{"name":"aster_roles", "value":"viewer", "url":base}])
                with page.expect_response(lambda response: response.url.endswith("/aster.v1.Aster/SendMessage")) as denied_send:
                    first.locator(".cell-chat-send").click()
                assert denied_send.value.status == 403
                expect(first.locator(".cell-chat-status")).to_contain_text("Your draft is unchanged")
                expect(first.locator(".cell-chat-history")).to_be_empty()
                expect(first.get_by_role("button", name="Replace this cell", exact=True)).to_have_count(0)
                assert page.evaluate("!cellChatState('c1').loaded && cellChatState('c1').messages.length === 0")
                expect(first.locator(".cell-chat-prompt")).to_have_value("unsent cell draft")
                pending_cell_read[0][0].fulfill(response=pending_cell_read[0][1])
                page.wait_for_timeout(150)
                expect(first.locator(".cell-chat-history")).to_be_empty()
                assert page.evaluate("!cellChatState('c1').loaded && cellChatState('c1').messages.length === 0")
                expect(first.locator(".cell-chat-prompt")).to_have_value("unsent cell draft")
                expect(first.locator(".editor")).to_have_value(editor_before)
                assert len(received) == helper_before, "revoked send must not call helper"
                assert page.request.post(base+"/aster.v1.Aster/GetConversation",
                    headers={"Connect-Protocol-Version":"1"}, data=cell_body).json() == cell_stored
                page.unroute("**/aster.v1.Aster/GetConversation", hold_cell_read)
                page.context.add_cookies([{"name":"aster_roles", "value":"editor", "url":base}])
                page.evaluate("loadCellChat(document.querySelector('.cell'))")
                expect(first.locator(".cell-chat-history .chat-message")).to_have_count(2)
                first.locator(".cell-chat-close").click()
                expect(first.locator(".cell-chat")).to_be_hidden()
                # Failed fresh admission must clear displayed cached replies,
                # retaining both composers and the server's stored history.
                before_denial = []
                def deny_history(route):
                    if not before_denial:
                        before_denial.append((route, route.fetch()))
                    else:
                        route.fulfill(status=403, json={"code":"permission_denied", "message":"Context revoked; start a fresh conversation"})
                page.route("**/aster.v1.Aster/GetConversation", deny_history)
                page.locator("#chat-close").click()
                page.locator("#chat-toggle").click()
                for _ in range(50):
                    if before_denial: break
                    page.wait_for_timeout(20)
                assert before_denial
                page.locator("#chat-close").click()
                page.locator("#chat-toggle").click()
                expect(page.locator("#chat-status")).to_contain_text("fresh conversation")
                expect(page.locator("#chat-history")).to_be_empty()
                before_denial[0][0].fulfill(response=before_denial[0][1])
                page.wait_for_timeout(150)
                expect(page.locator("#chat-history")).to_be_empty()
                expect(page.locator("#chat-status")).to_contain_text("fresh conversation")
                expect(page.locator("#chat-prompt")).to_have_value("unsent draft")
                first.locator('[data-action="ai"]').click()
                expect(first.locator(".cell-chat-status")).to_contain_text("fresh conversation")
                expect(first.locator(".cell-chat-history")).to_be_empty()
                expect(first.locator(".cell-chat-prompt")).to_have_value("unsent cell draft")
                stored = page.request.post(base+"/aster.v1.Aster/GetConversation", headers={"Connect-Protocol-Version":"1"}, data={"notebook":"chat-check"}).json()
                assert len(stored["messages"]) == 6, stored
                page.set_viewport_size({"width":390,"height":844})
                expect(page.locator("#chat-send")).to_be_visible()
                page.screenshot(path="/tmp/aster-conversation-sidebar.png",full_page=True)
                assert not errors,errors
                browser.close()
            print("conversation-browser OK")
        finally:
            server.terminate()
            server.wait(timeout=15)
            mock.shutdown()
            mock.server_close()
