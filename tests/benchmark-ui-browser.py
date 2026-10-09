"""Frontend protocol check in Chromium; fixture API, real unmodified app.js/CSS.

Backend persistence, authorization and live Trino are separate integration gates.
"""
import json
import os
from pathlib import Path
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit, parse_qs

from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
notebook = {"id": "bench", "title": "Benchmark", "cells": [
    {"id": "c1", "sql": "SELECT count(*) FROM nation", "engine": None,
     "metadata": {"note": "preserve unrelated metadata"}}]}
queries, discoveries = [], []
catalog_ids = ["aster-tpch", "aster-tpcds"]
namespace_names = ["tiny"]
revoked = False
literal = '<img src=x onerror="window.contractInjected=true">'
declared = {
    "id": "benchmark", "version": "1", "description": {"purpose": literal},
    "schema": [{"name": "nation", "businessName": "Nation", "properties": [
        {"name": "nationkey", "logicalType": "integer", "required": False,
         "quality": [{"type": "sql", "mustBe": 0}],
         "customProperties": [{"property": "nested", "value": {"enabled": False, "zero": 0}}]}]}],
    "team": {"name": "Synthetic benchmark"}, "slaProperties": [{"property": "freshness", "value": 0}],
    "customProperties": [{"property": "deep", "value": {"items": [False, 0, {"text": literal}]}}],
}
original = 'id: benchmark\n# original spacing retained\ndescription: "literal"\n'


class Fixture(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def reply(self, value, status=200, content_type="application/json"):
        payload = (json.dumps(value) if content_type == "application/json" else value).encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        path = urlsplit(self.path).path
        if path in ("/app.js", "/app.css"):
            self.reply((ROOT / "crates/server/assets" / path[1:]).read_text(), content_type=
                       "text/javascript" if path.endswith("js") else "text/css")
        elif path == "/api/catalogs":
            self.reply([{"id": name, "kind": "trino", "health": "healthy"}
                        for name in catalog_ids])
        elif path == "/api/engines":
            context = parse_qs(urlsplit(self.path).query).get("catalog_context", [None])[0]
            discoveries.append(context)
            self.reply([{"id": "trino", "kind": "trino", "health": "healthy"}] if context else [],
                       200 if context else 400)
        elif path.endswith("/namespaces"):
            self.reply([{"name": name, "segments": [name]} for name in namespace_names])
        elif path == "/":
            self.reply('''<!doctype html><html><head><meta name="viewport" content="width=device-width">
<link rel="stylesheet" href="/app.css"></head><body><p id="status" role="status"></p>
<button data-action="save">Save</button><div id="cells">
<section class="cell" data-id="c1"><div class="cell-head"><select class="engine"></select>
<button class="btn-run" data-action="run">Run</button></div><textarea class="editor">SELECT count(*) FROM nation</textarea>
<div class="out"><div class="out-content"></div></div></section></div>
<section id="contract-context"><label for="contract-selection">Declared contract</label><select id="contract-selection"></select>
<label for="contract-object">Declared object</label><select id="contract-object"></select><button id="contract-refresh">Refresh</button>
<p id="contract-status"></p><details><summary>Full declared document and provenance</summary>
<pre id="contract-document"></pre></details><div id="contract-preparation"></div>
<textarea id="contract-draft"></textarea><button id="contract-review-button">Review draft with context</button>
<div id="contract-review"></div></section><script id="nb" type="application/json">'''
                       + json.dumps(notebook).replace("<", "\\u003c") + '</script><script src="/app.js"></script></body></html>',
                       content_type="text/html")
        else:
            self.reply({})

    def do_PUT(self):
        global notebook
        notebook = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.reply({"revision": "saved", "content_revision": "saved"})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if self.path == "/api/query":
            queries.append(body)
            self.reply({"columns": [{"name": "count", "type": "bigint"}], "rows": [[25]], "truncated": False})
        elif revoked:
            self.reply({"message": "denied"}, 403)
        else:
            value = {"contracts": [{"id": "benchmark", "version": "1", "target": "tiny",
                                    "path": "nation.yaml", "sha256": "pinned"}]}
            if self.path.endswith("GetContract"):
                value = {"declared": declared, "provenance": {"sha256": "pinned"}, "original": original}
            elif self.path.endswith("PrepareContractQuery"):
                value = {"declared": declared, "provenance": {"sha256": "pinned"},
                         "observation": {"status": "unavailable"}, "comparison": {"status": "not measured"}}
            self.reply({"contextJson": json.dumps(value)})


server = ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE"))
        page = browser.new_page(viewport={"width": 390, "height": 844})
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(f"http://127.0.0.1:{server.server_port}/")
        expect(page.get_by_label("Catalog", exact=True)).to_be_enabled()
        expect(page.get_by_label("Catalog", exact=True)).to_have_value("")
        page.get_by_role("button", name="Run", exact=True).click()
        expect(page.locator(".out")).to_contain_text("Select an available catalog")
        assert not queries, "multiple catalogs require explicit selection"
        page.get_by_label("Declared contract", exact=True).select_option("0")
        structured = page.locator("#contract-details")
        expect(structured).to_be_visible()
        expect(structured).to_contain_text("Declared contract")
        expect(structured).to_contain_text("Quality results: not measured")
        for text in (literal, "nationkey", "integer", "false", "0", "freshness", "Synthetic benchmark", "enabled"):
            expect(structured).to_contain_text(text)
        assert structured.locator("pre").count() == 0, "structured fields must not be JSON dumps"
        assert structured.locator("img").count() == 0
        assert "pricing" not in structured.inner_text()
        page.get_by_text("Full declared document and provenance", exact=True).click()
        expect(page.locator("#contract-document")).to_contain_text("pinned")
        assert original in json.loads(page.locator("#contract-document").inner_text()).values()
        page.get_by_label("Declared object", exact=True).select_option("/schema/0")
        expect(page.locator("#contract-preparation")).to_contain_text("Observed metadata")
        for catalog in ("aster-tpch", "aster-tpcds"):
            page.get_by_label("Catalog", exact=True).select_option(catalog)
            expect(page.get_by_label("Schema", exact=True)).to_have_value("tiny")
            expect(page.locator("select.engine option[value=trino]")).to_have_count(1)
            page.locator("select.engine").select_option("trino")
            page.get_by_role("button", name="Save", exact=True).click()
            expect(page.locator("#status")).to_have_text("saved saved")
            assert notebook["cells"][0]["metadata"]["note"] == "preserve unrelated metadata"
            page.reload()
            expect(page.get_by_label("Catalog", exact=True)).to_have_value(catalog)
            expect(page.get_by_label("Schema", exact=True)).to_have_value("tiny")
            expect(page.locator("select.engine")).to_be_enabled()
            page.get_by_role("button", name="Run", exact=True).click()
            expect(page.locator(".out")).to_contain_text("25")
            assert queries[-1]["catalog_context"] == catalog, queries[-1]
            assert queries[-1]["schema"] == "tiny", queries[-1]
            assert queries[-1]["engine"] == "trino", queries[-1]
            assert queries[-1]["sql"] == "SELECT count(*) FROM nation"
            assert discoveries[-1] == catalog, discoveries
        # Saving SQL while discovery is pending or failed must not erase context.
        held_catalogs = []
        page.route("**/api/catalogs", lambda route: held_catalogs.append(route))
        page.reload()
        expect(page.get_by_label("Catalog", exact=True)).to_be_disabled()
        page.locator(".editor").fill("SELECT 7")
        page.get_by_role("button", name="Save", exact=True).click()
        expect(page.locator("#status")).to_have_text("saved saved")
        saved_cell = notebook["cells"][0]
        assert saved_cell["metadata"]["catalog_context"] == "aster-tpcds", saved_cell
        assert saved_cell["metadata"]["schema"] == "tiny", saved_cell
        assert saved_cell["engine"] == "trino", saved_cell
        assert saved_cell["sql"] == "SELECT 7", saved_cell
        assert len(held_catalogs) == 1
        held_catalogs[0].fulfill(status=503, json={"error": "unavailable"})
        expect(page.locator("#status")).to_contain_text("Catalog choices unavailable")
        page.locator(".editor").fill("SELECT 8")
        page.get_by_role("button", name="Save", exact=True).click()
        expect(page.locator("#status")).to_have_text("saved saved")
        assert notebook["cells"][0]["metadata"]["catalog_context"] == "aster-tpcds"
        assert notebook["cells"][0]["metadata"]["schema"] == "tiny"
        query_count = len(queries)
        page.get_by_role("button", name="Run", exact=True).click()
        expect(page.locator(".out")).to_contain_text("Select an available catalog")
        assert len(queries) == query_count, "unresolved saved context must not execute"
        page.unroute("**/api/catalogs")
        # Existing notebooks with one catalog and no context metadata still run.
        # A schema is optional: fully qualified SQL and mock engines need none.
        catalog_ids = ["sole-catalog"]
        namespace_names = []
        notebook["cells"][0]["metadata"] = {"note": "legacy notebook"}
        page.reload()
        expect(page.get_by_label("Catalog", exact=True)).to_have_value("sole-catalog")
        expect(page.locator("select.engine")).to_be_enabled()
        expect(page.locator("select.engine")).to_have_value("trino")
        expect(page.get_by_label("Schema", exact=True)).to_have_value("")
        page.locator(".editor").fill("SELECT * FROM tpch.tiny.nation")
        page.get_by_role("button", name="Run", exact=True).click()
        expect(page.locator(".out")).to_contain_text("25")
        assert queries[-1]["catalog_context"] == "sole-catalog"
        assert queries[-1]["schema"] is None
        assert discoveries[-1] == "sole-catalog"
        page.get_by_label("Declared contract", exact=True).select_option("0")
        expect(structured).to_contain_text(literal)
        revoked = True
        page.get_by_role("button", name="Refresh", exact=True).click()
        expect(structured).to_be_empty()
        expect(page.locator("#contract-document")).to_be_empty()
        expect(page.locator("#contract-preparation")).to_be_empty()
        assert page.evaluate("window.contractInjected") is None
        assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
        assert not errors, errors
        browser.close()
finally:
    server.shutdown()
print("BENCHMARK_UI_OK: structured declared fields, literal text, revocation, scoped Run after save/reload (fixture API)")
