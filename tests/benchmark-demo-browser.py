"""Real isolated-demo login and notebook lifecycle. Never print credentials."""
import base64
import importlib.util
import json
import os
import pathlib
import socket
import subprocess
import time
import traceback
import urllib.request

from playwright.sync_api import expect, sync_playwright

root = pathlib.Path(__file__).resolve().parent.parent
evidence = pathlib.Path(os.environ["ASTER_DEMO_EVIDENCE"])
evidence.mkdir(parents=True, exist_ok=True)
k = ["kubectl", "--context", os.environ["ASTER_KUBE_CONTEXT"]]
assert os.environ["ASTER_KUBE_CONTEXT"] == "aster-demo", "isolated demo only"
registration = json.loads(pathlib.Path(os.environ["ASTER_DEMO_REGISTRATION"]).read_text())
manifest = json.loads((root / "contracts/benchmark-catalog/manifest.json").read_text())
spec = importlib.util.spec_from_file_location("trino", root / "tests/benchmark-trino.py")
trino = importlib.util.module_from_spec(spec)
spec.loader.exec_module(trino)
children = []
step = "setup"
receipt = {"checks": [], "scope": "synthetic benchmarks; not SQL target authorization"}


def secret(namespace, name):
    value = json.loads(subprocess.check_output(k + ["-n", namespace, "get", "secret", name, "-o", "json"]))
    return {key: base64.b64decode(value).decode() for key, value in value["data"].items()}


def forward(namespace, target, ports):
    port = int(ports.split(":")[0])
    with socket.socket() as available:
        available.bind(("127.0.0.1", port))
    children.append(subprocess.Popen(k + ["-n", namespace, "port-forward", target, ports],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))


def rpc(page, method, body):
    return page.evaluate("""async ({method,body}) => {
      const r=await fetch('/aster.v1.Aster/'+method,{method:'POST',headers:{'content-type':'application/json','connect-protocol-version':'1'},body:JSON.stringify(body)});
      return {status:r.status,body:await r.json()};
    }""", {"method": method, "body": body})


def login(browser, username, credentials):
    context = browser.new_context(viewport={"width": 1440, "height": 1000})
    page = context.new_page()
    page.goto("http://localhost:8080/login", wait_until="networkidle")
    page.locator('input[name="uidField"]').fill(username)
    page.locator('button[type="submit"]').click()
    page.get_by_role("button", name="Continue", exact=True).wait_for()
    page.locator('input[name="password"]').fill(credentials[username])
    page.get_by_role("button", name="Continue", exact=True).click()
    page.wait_for_url("http://localhost:8080/", timeout=30000)
    expect(page.get_by_role("heading", name="Notebooks", exact=True)).to_be_visible()
    return context, page


def ak(path, token, data=None):
    request = urllib.request.Request("http://localhost:19001/api/v3/" + path,
                                     data=None if data is None else json.dumps(data).encode(),
                                     method="GET" if data is None else "PATCH",
                                     headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=15) as response:
        return json.load(response)


try:
    credentials = secret("aster", "aster-authentik-demo-login")
    bootstrap = secret("aster-authentik", "authentik-demo-bootstrap")["AUTHENTIK_BOOTSTRAP_TOKEN"]
    forward("aster", "svc/aster-server", "8080:80")
    forward("aster", "deployment/aster-server", "8081:8081")
    forward("aster", "svc/benchmark-trino", "18090:8080")
    forward("aster-authentik", "svc/authentik-demo-server", "19001:80")
    for endpoint in ["http://localhost:8080/healthz", "http://localhost:8081/-/health/ready/", "http://localhost:18090/v1/info", "http://localhost:19001/-/health/ready/"]:
        for attempt in range(30):
            try:
                with urllib.request.urlopen(endpoint, timeout=2) as response:
                    assert response.status == 200
                break
            except OSError:
                time.sleep(1)
        else:
            raise AssertionError("demo endpoint readiness")
    with sync_playwright() as pw:
        browser = pw.chromium.launch(executable_path=os.environ["ASTER_BROWSER_EXECUTABLE"], headless=True, args=["--no-sandbox"])
        step = "fresh login must land on usable notebooks"
        owner, page = login(browser, "alice", credentials)
        errors = []
        page.on("pageerror", lambda error: errors.append(type(error).__name__))
        notebook = os.environ.get("ASTER_DEMO_NOTEBOOK")
        if notebook:
            page.goto("http://localhost:8080/notebooks/" + notebook)
            expect(page.locator(".cell").first.get_by_label("Catalog", exact=True)).to_have_value("tpcds")
            expect(page.locator(".cell .editor").first).to_have_value("SELECT count(*) AS rows FROM call_center")
            receipt["restart_persistence"] = True
        else:
            step = "create notebook through browser"
            notebook = "benchmark-tour-" + str(int(time.time()))
            page.locator("#new-id").fill(notebook)
            page.locator("#create").get_by_role("button", name="Create", exact=True).click()
            page.wait_for_url("http://localhost:8080/notebooks/" + notebook)
        receipt["notebook"] = notebook
        for catalog, table in [("tpch", "nation"), ("tpcds", "call_center")]:
            step = "save reload and real query: " + catalog
            cell = page.locator(".cell").first
            cell.get_by_label("Catalog", exact=True).select_option(catalog)
            schema = cell.get_by_label("Schema", exact=True)
            if schema.evaluate("e => e.tagName") == "SELECT":
                schema.select_option("tiny")
            else:
                schema.fill("tiny")
            cell.locator("select.engine").select_option("trino-local")
            sql = f"SELECT count(*) AS rows FROM {table}"
            cell.locator(".editor").fill(sql)
            with page.expect_response(lambda r: r.request.method == "PUT" and "/api/notebooks/" in r.url) as saved:
                page.locator('[data-action="save"]').click()
            assert saved.value.status == 200
            page.reload(wait_until="networkidle")
            cell = page.locator(".cell").first
            expect(cell.locator(".editor")).to_have_value(sql)
            expect(cell.get_by_label("Catalog", exact=True)).to_have_value(catalog)
            expect(cell.get_by_label("Schema", exact=True)).to_have_value("tiny")
            expected = trino.query("http://localhost:18090", f"SELECT count(*) FROM {catalog}.tiny.{table}")
            assert expected and expected[0][0] > 0
            with page.expect_response(lambda r: r.url.endswith("/api/query")) as result:
                cell.locator('[data-action="run"]').click()
            response = result.value
            assert response.status == 200
            sent = response.request.post_data_json
            assert sent["catalog_context"] == catalog and sent["schema"] == "tiny"
            assert response.json()["rows"] == expected
            expect(cell.locator(".out-content")).to_contain_text(str(expected[0][0]))
            receipt["checks"].append({"catalog": catalog, "saved_and_reloaded": True, "real_rows": expected[0][0]})
        page.screenshot(path=str(evidence / "benchmark-notebook.png"), full_page=True)
        step = "real inventories and structured contracts"
        for catalog, expected_count in [("tpch", 8), ("tpcds", 25)]:
            native = trino.query("http://localhost:18090", f"SELECT table_name FROM {catalog}.information_schema.tables WHERE table_schema='tiny' ORDER BY table_name")
            inventory = rpc(page, "ListCatalogInventory", {"catalog": catalog, "namespaceSegments": ["tiny"]})
            assert inventory["status"] == 200
            inventory = json.loads(inventory["body"]["contextJson"])
            names = sorted(item["physicalName"] for item in inventory["entries"])
            assert names == [row[0] for row in native] and len(names) == expected_count
            for name in names:
                quoted = '"' + name.replace('"', '""') + '"'
                assert trino.query("http://localhost:18090", f"SELECT * FROM {catalog}.tiny.{quoted} LIMIT 1")
            receipt["checks"].append({"catalog": catalog, "live_tables_probed": len(names)})
        page.goto("http://localhost:8080/catalog", wait_until="networkidle")
        expect(page.get_by_label("Declared contract", exact=True).locator("option")).to_have_count(34)
        page.get_by_label("Declared contract", exact=True).select_option(index=1)
        expect(page.locator("#contract-details")).to_contain_text("Synthetic")
        for document in manifest["documents"]:
            detail = rpc(page, "GetContract", {"path": document["path"], "sha256": document["sha256"]})
            assert detail["status"] == 200
            detail = json.loads(detail["body"]["contextJson"])
            assert detail["originalSource"] == (root / "contracts/benchmark-catalog" / document["path"]).read_text()
            assert detail["provenance"]["sha256"] == document["sha256"]
        page.screenshot(path=str(evidence / "benchmark-contracts.png"), full_page=True)
        step = "viewer cannot read or modify owner notebook or run queries"
        owned = page.request.get("http://localhost:8080/api/notebooks/" + notebook)
        assert owned.status == 200
        saved_document = owned.json()
        viewer, other = login(browser, "viewer", credentials)
        assert other.request.get("http://localhost:8080/api/notebooks/" + notebook).status in [403, 404]
        assert other.request.get("http://localhost:8080/notebooks/" + notebook).status in [403, 404]
        assert other.request.put("http://localhost:8080/api/notebooks/" + notebook,
                                 headers={"if-match": owned.headers["etag"]},
                                 data={**saved_document, "title": "forbidden overwrite"}).status in [403, 404]
        assert page.request.get("http://localhost:8080/api/notebooks/" + notebook).json() == saved_document
        assert other.request.post("http://localhost:8080/api/query", data={"sql": "SELECT 1", "engine": "trino-local", "catalog_context": "tpch", "schema": "tiny"}).status == 403
        receipt["viewer_denied"] = True
        step = "fresh contract revocation and UI clearing"
        alice_path = f'core/users/{registration["users"]["alice"]["pk"]}/'
        groups = ak(alice_path, bootstrap)["groups"]
        ak(alice_path, bootstrap, {"groups": []})
        try:
            first = manifest["documents"][0]
            assert rpc(page, "GetContract", {"path": first["path"], "sha256": first["sha256"]})["status"] == 403
            page.get_by_role("button", name="Refresh contracts", exact=True).click()
            expect(page.locator("#contract-details")).to_be_empty()
            expect(page.locator("#contract-document")).to_be_empty()
        finally:
            ak(alice_path, bootstrap, {"groups": groups})
        receipt["revocation_restored"] = True
        assert not errors, "browser JavaScript errors"
        browser.close()
    (evidence / "benchmark-browser-result.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("BENCHMARK_DEMO_E2E_OK")
except Exception as error:
    print("BENCHMARK_DEMO_E2E_FAILED:", step, type(error).__name__)
    print("check lines:", [frame.lineno for frame in traceback.extract_tb(error.__traceback__) if frame.filename == __file__])
    raise SystemExit(1) from None
finally:
    for child in children:
        child.terminate()
        child.wait(timeout=10)
