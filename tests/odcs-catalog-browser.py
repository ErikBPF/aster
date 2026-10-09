"""S7 real browser and real router; disposable verified team fixture, no providers."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from playwright.sync_api import sync_playwright, expect

with tempfile.TemporaryDirectory(prefix="aster-s7-browser-") as work:
    receipt = Path(work) / "fixture.json"
    env = dict(os.environ, ASTER_S7_BROWSER_FIXTURE=str(receipt))
    # Compile before the readiness clock starts: a cold, isolated snapshot can
    # legitimately need more than 60 seconds. Run Cargo's exact artifact below.
    built = subprocess.run(["cargo", "test", "-p", "aster-server", "--test", "odcs_catalog_view",
                            "--no-run", "--message-format=json"], env=env, capture_output=True, text=True)
    assert built.returncode == 0, built.stderr
    binaries = [item["executable"] for line in built.stdout.splitlines()
                if (item := json.loads(line)).get("reason") == "compiler-artifact"
                and item["target"]["name"] == "odcs_catalog_view" and item.get("executable")]
    assert len(binaries) == 1, binaries
    with (Path(work) / "server.log").open("w") as log:
        server = subprocess.Popen([binaries[0], "catalog_view_matches_authorized_projection", "--exact", "--nocapture"],
                                  env=env, stdout=log, stderr=log, start_new_session=True)
        try:
            for _ in range(600):
                if receipt.exists():
                    break
                assert server.poll() is None, (Path(work) / "server.log").read_text()
                time.sleep(.1)
            else:
                raise AssertionError("fixture did not start: " + (Path(work) / "server.log").read_text())
            fixture = json.loads(receipt.read_text())
            with sync_playwright() as pw:
                browser = pw.chromium.launch(executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE"))
                context = browser.new_context(viewport={"width": 390, "height": 844})
                context.add_cookies([{"name": "aster_session", "value": fixture["cookie"], "url": fixture["base"]}])
                page = context.new_page()
                requests, responses, errors = [], [], []
                page.on("request", lambda req: requests.append((req.method, req.url)))
                page.on("requestfinished", lambda req: responses.append(req.response().text()) if "/aster.v1.Aster/" in req.url else None)
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.goto(fixture["base"] + "/catalog")
                expect(page.locator("#contract-context")).to_be_visible()
                page.get_by_label("Declared contract", exact=True).select_option(index=1)
                expect(page.locator("#contract-document")).to_contain_text("TEAM_ONLY_MEANING")
                page.get_by_label("Declared object", exact=True).select_option("/schema/0")
                expect(page.locator("#contract-preparation")).to_contain_text("NET_AFTER_REFUNDS")
                expect(page.locator("#contract-preparation")).to_contain_text("denied")
                page.get_by_label("Query draft", exact=True).fill("SELECT net_amount FROM orders")
                page.get_by_role("button", name="Review draft with context", exact=True).focus()
                page.keyboard.press("Enter")
                expect(page.locator("#contract-review")).to_contain_text("SELECT net_amount FROM orders")
                for meaning in ["NET_FIELD_DEFINITION", "measure", "gross - refunds", "ONE_ROW_PER_ORDER",
                                "CUSTOMER_REFERENCE_ONLY", "sales.eu", fixture["sha256"]]:
                    expect(page.locator("#contract-review")).to_contain_text(meaning)
                assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
                assert page.evaluate("window.contractInjected") is None
                page.get_by_label("Declared object", exact=True).select_option("/schema/2")
                expect(page.locator("#contract-preparation")).to_contain_text("GROSS_BEFORE_REFUNDS")
                page.get_by_role("button", name="Review draft with context", exact=True).click()
                expect(page.locator("#contract-review")).to_contain_text("gross_amount")
                assert "NET_AFTER_REFUNDS" not in page.locator("#contract-review").inner_text()
                page.get_by_label("Declared object", exact=True).select_option("/schema/3")
                expect(page.locator("#contract-preparation")).to_contain_text("ambiguous")
                page.get_by_label("Declared object", exact=True).select_option("/schema/1")
                expect(page.locator("#contract-preparation")).to_contain_text("missing")
                expect(page.locator("#contract-review")).to_be_empty()
                page.get_by_role("button", name="Review draft with context", exact=True).click()
                expect(page.locator("#contract-review")).to_contain_text("refunds")
                assert "NET_AFTER_REFUNDS" not in page.locator("#contract-review").inner_text()
                page.get_by_label("Query draft", exact=True).fill("SELECT 2 -- edited")
                expect(page.locator("#contract-review")).to_be_empty()
                page.get_by_role("button", name="Review draft with context", exact=True).click()
                expect(page.locator("#contract-review")).to_contain_text("SELECT 2 -- edited")
                allowed_responses = list(responses)
                Path(fixture["bundle"], "grants.json").write_text(json.dumps({"formatVersion": 1, "version": "revoked", "grants": []}))
                responses.clear()
                page.get_by_role("button", name="Review draft with context", exact=True).click()
                expect(page.locator("#contract-document")).to_be_empty()
                expect(page.locator("#contract-preparation")).to_be_empty()
                expect(page.locator("#contract-review")).to_be_empty()
                page.reload()
                expect(page.get_by_label("Declared contract", exact=True).locator("option")).to_have_count(1)
                assert "TEAM_ONLY_MEANING" not in page.locator("body").inner_text()
                for response in responses:
                    assert "TEAM_ONLY_MEANING" not in response
                for response in allowed_responses + responses:
                    assert "DENIED_QUERY_MEANING" not in response
                assert not any("/api/query" in url or "ExecuteQuery" in url for _, url in requests)
                assert not any("/api/ai" in url or "/SendMessage" in url or "/chat/completions" in url for _, url in requests), "S7 must not call helper endpoints"
                assert not any(method in ("PUT", "DELETE", "PATCH") for method, _ in requests)
                assert not errors, errors
                browser.close()
        finally:
            import signal
            os.killpg(server.pid, signal.SIGTERM)
            server.wait(timeout=10)
print("odcs-catalog-browser OK: 1 browser flow; zero query execution requests")
