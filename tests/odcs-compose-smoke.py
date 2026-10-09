"""Disposable Build-host Compose controls; binaries come only from declared devenv."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid

assert sys.argv[1:] in (["s1"], ["s2"], ["s3"], ["s7"], ["s5"], ["s6"], ["s8"]), "unknown slice"
slice_name = sys.argv[1]
assert socket.gethostname() == "build-host", "builds require Build-host"
root = Path(__file__).resolve().parent.parent
engine = os.environ.get("CONTAINER_ENGINE", "podman")
project = "aster-odcs-" + slice_name + "-" + uuid.uuid4().hex[:12]
env = os.environ.copy()
env.update({"ASTER_ENGINES": "mock-local;mock;local", "ASTER_GRANTS": "alice:mock-local",
            "POSTGRES_USER": "aster", "POSTGRES_PASSWORD": "aster", "POSTGRES_DB": "aster",
            "VALKEY_PASSWORD": "aster-dev", "KC_ADMIN_USER": "admin", "KC_ADMIN_PASSWORD": "aster-dev-admin",
            "OTEL_EXPORTER_OTLP_ENDPOINT": ""})
env["COMPOSE_PROJECT_NAME"] = project
env["CONTAINER_ENGINE"] = engine
env["ASTER_DEV_LOGIN"] = "1"
env["ASTER_CATALOG_BINDINGS"] = "mock-local;mock-local;mock-local;unprotected"
env["ASTER_CATALOGS"] = "mock-local;mock;local"
ports = []
for name in ["ASTER_PORT", "ASTER_METRICS_PORT", "ASTER_DB_PORT", "ASTER_STATE_PORT", "ASTER_IDP_PORT"]:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    env[name] = f"127.0.0.1:{port}"
    ports.append(port)


def run(*args, capture=False):
    return subprocess.run(args, cwd=root, env=env, check=True, text=True,
                          stdout=subprocess.PIPE if capture else None)


def request(path, body=None, cookie="aster_subject=alice; aster_roles=editor", method=None):
    req = urllib.request.Request(f"http://127.0.0.1:{ports[0]}{path}",
                                 data=None if body is None else json.dumps(body).encode(), method=method,
                                 headers={"Cookie": cookie, "Connect-Protocol-Version": "1",
                                          "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=5) as response:
            return response.status, response.read().decode()
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode()


def healthy():
    for _ in range(90):
        try:
            if request("/healthz")[0] == 200:
                return
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(1)
    raise AssertionError("isolated server did not become healthy")


build = run("cargo", "build", "--locked", "--target-dir", str(root / "target"), "--message-format=json",
            "-p", "aster-server", "-p", "aster-controller", capture=True)
artifacts = {item["target"]["name"]: Path(item["executable"]).resolve()
             for line in build.stdout.splitlines()
             if (item := json.loads(line)).get("reason") == "compiler-artifact"
             and item.get("executable")}
for binary in ["aster-server", "aster-controller"]:
    expected = (root / "target/debug" / binary).resolve()
    assert artifacts.get(binary) == expected, (binary, artifacts.get(binary), expected)
    print(f"Cargo artifact {binary}: {expected}", flush=True)
# No Docker Rust toolchain: package the exact devenv-built executables. The
# read-only store mount supplies their pinned dynamic loader and Git closure.
with tempfile.TemporaryDirectory(prefix="odcs-s1-compose-", dir=root / "target") as tmp:
    tmp = Path(tmp)
    digests = {}
    for binary in ["aster-server", "aster-controller"]:
        shutil.copy2(root / "target/debug" / binary, tmp / binary)
        digests[binary] = hashlib.file_digest(artifacts[binary].open("rb"), "sha256").hexdigest()
        assert hashlib.file_digest((tmp / binary).open("rb"), "sha256").hexdigest() == digests[binary]
    dockerfile = tmp / "Dockerfile"
    dockerfile.write_text(
        "FROM docker.io/library/debian:bookworm-slim\n"
        "COPY aster-server /usr/local/bin/aster-server\n"
        "COPY aster-controller /usr/local/bin/aster-controller\n"
        f"ENV PATH={Path(shutil.which('git')).parent}:/usr/local/bin:/usr/bin:/bin\n"
        'ENTRYPOINT ["/usr/local/bin/aster-server"]\n'
    )
    override = tmp / "compose.json"
    override.write_text(json.dumps({"services": {
        name: {"image": f"localhost/{project}:test",
               "build": {"context": str(tmp), "dockerfile": str(dockerfile)},
               "volumes": ["/nix/store:/nix/store:ro"]}
        for name in ["server", "controller"]
    }}))
    env["COMPOSE_FILE"] = f"{root / 'docker-compose.yml'}:{override}"
    if slice_name in ["s2", "s3", "s7", "s5", "s8"]:
        bundle = tmp / "bundle"
        (bundle / "compiled").mkdir(parents=True)
        document = json.dumps({"apiVersion": "v3.2.0", "kind": "DataContract", "id": "INTERNAL_ONLY_CONTRACT",
                               "version": "1.4.0", "description": {"purpose": "INTERNAL_ONLY_MEANING"},
                               "schema": [{"name": "orders", "description": "NET_AFTER_REFUNDS"}]}).encode()
        (bundle / "compiled/sales.json").write_bytes(document)
        manifest = json.dumps({"manifestVersion": 1, "bundle": "smoke", "version": "reviewed",
                               "documents": [{"path": "compiled/sales.json", "sha256": hashlib.sha256(document).hexdigest(),
                                              "id": "INTERNAL_ONLY_CONTRACT", "version": "1.4.0", "target": "warehouse"}]}).encode()
        (bundle / "manifest.json").write_bytes(manifest)
        config = json.loads(override.read_text())
        config["services"]["server"]["volumes"].append(f"{bundle}:/odcs:ro")
        config["services"]["server"]["environment"] = {
            "ASTER_CONTRACT_BUNDLE": "/odcs", "ASTER_CONTRACT_BUNDLE_VERSION": "reviewed",
            "ASTER_CONTRACT_MANIFEST_SHA256": hashlib.sha256(manifest).hexdigest()}
        override.write_text(json.dumps(config))
        if slice_name in ["s3", "s7", "s5", "s8"]:
            group = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
            user = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"
            (bundle / "grants.json").write_text(json.dumps({"formatVersion": 1, "version": "1", "grants": [
                {"path": "compiled/sales.json", "sha256": hashlib.sha256(document).hexdigest(), "teams": ["alpha"]}]}))
            bindings = json.dumps({"formatVersion": 1, "version": "1", "bindings": [{
                "path": "compiled/sales.json", "object": "/schema/0", "catalog": "mock-local",
                "namespaceSegments": ["aster_demo"], "physicalName": "orders"}]}).encode()
            (bundle / "bindings.json").write_bytes(bindings)
            parsed = json.loads(manifest)
            parsed["bindingsSha256"] = hashlib.sha256(bindings).hexdigest()
            manifest = json.dumps(parsed).encode()
            (bundle / "manifest.json").write_bytes(manifest)
            (bundle / "teams.json").write_text(json.dumps({"alpha": {"member_group_uuid": group,
                "maintainer_group_uuid": "cccccccc-cccc-4ccc-8ccc-cccccccccccc", "installation_id": 1,
                "allowed_repositories": {"fixture/notebooks": 1}}}))
            (bundle / "member").touch()
            if slice_name == "s8":
                (bundle / "schema-owners.json").write_text(json.dumps({"formatVersion": 1, "version": "owners-1", "schemas": [
                    {"catalog": "mock-local", "namespaceSegments": ["aster_demo"], "team": "alpha"}]}))
            run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                "-subj", "/CN=authority", "-addext", "subjectAltName=DNS:authority", "-addext", "basicConstraints=critical,CA:FALSE",
                "-keyout", str(bundle / "key.pem"), "-out", str(bundle / "cert.pem"))
            shutil.copy2(root / "tests/odcs-s3-authority.py", bundle / "authority.py")
            config["services"]["authority"] = {"image": f"localhost/{project}:test",
                "entrypoint": [sys.executable, "/fixture/authority.py"],
                "volumes": ["/nix/store:/nix/store:ro", f"{bundle}:/fixture:ro"]}
            config["services"]["server"]["environment"].update({
                "ASTER_CONTRACT_MANIFEST_SHA256": hashlib.sha256(manifest).hexdigest(),
                "ASTER_TEAM_GIT_ENABLED": "1", "ASTER_TEAM_GIT_POLICY_FILE": "/odcs/teams.json",
                "ASTER_GITHUB_APP_ID": "1", "ASTER_GITHUB_APP_KEY_NAME": "DISPOSABLE_GITHUB_KEY",
                "DISPOSABLE_GITHUB_KEY": "unused-no-github-calls", "ASTER_IDP_KIND": "oidc",
                "ASTER_OIDC_ISSUER": "https://authority:8443", "ASTER_IDP_USER_UUID_CLAIM": "user_uuid",
                "ASTER_AUTHENTIK_API_ORIGIN": "https://authority:8443", "ASTER_AUTHENTIK_READ_TOKEN": "disposable",
                "ASTER_AUTHENTIK_CA_FILE": "/odcs/cert.pem"})
            override.write_text(json.dumps(config))
    if slice_name == "s6":
        fixture = tmp / "catalog-fixture.py"
        fixture.write_text('''import json
from http.server import BaseHTTPRequestHandler, HTTPServer
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        routes = {
            "/api/catalog/v1/lake/namespaces": ("p", {"namespaces": [["first"]], "next-page-token": "next"}),
            "/api/catalog/v1/lake/namespaces?pageToken=next": ("p", {"namespaces": [["second"]]}),
            "/api/v1/databaseSchemas?limit=200": ("o", {"data": [{"fullyQualifiedName": "service.db.fixture"}]}),
            "/api/v1/databases?limit=1": ("o", {"data": []}),
            "/cubejs-api/v1/meta": ("c", {"cubes": [{"name": "FixtureCube", "dimensions": [], "measures": []}]}),
        }
        target, payload = routes.get(self.path, ("unknown", {}))
        allowed = self.headers.get("Authorization") == "Bearer disposable-" + target
        body = json.dumps(payload if allowed else {"error": "unauthorized"}).encode()
        self.send_response(200 if allowed else 401)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
HTTPServer(("0.0.0.0", 8081), Handler).serve_forever()
''')
        config = json.loads(override.read_text())
        config["services"]["catalog-fixture"] = {
            "image": f"localhost/{project}:test", "entrypoint": [sys.executable, "/fixture.py"],
            "volumes": ["/nix/store:/nix/store:ro", f"{fixture}:/fixture.py:ro"]}
        config["services"]["server"]["environment"] = {
            "ASTER_SECRET_STORE": "env", "S6_P": "disposable-p", "S6_O": "disposable-o", "S6_C": "disposable-c"}
        config["services"]["controller"]["environment"] = {
            "S6_P": "disposable-p", "S6_O": "disposable-o", "S6_C": "disposable-c", "ASTER_RECONCILE_SECONDS": "1"}
        override.write_text(json.dumps(config))
        env["ASTER_CATALOGS"] += ",p;polaris;http://catalog-fixture:8081;lake;secret://S6_P,o;openmetadata;http://catalog-fixture:8081;;secret://S6_O,c;cube;http://catalog-fixture:8081;;secret://S6_C"
        env["ASTER_CATALOG_BINDINGS"] += ",p;mock-local;p;unprotected,o;mock-local;o;unprotected,c;mock-local;c;unprotected"
    started = False
    try:
        started = True
        run("just", "compose-up")
        healthy()
        packaged = run(engine, "compose", "exec", "-T", "server", "sha256sum",
                       "/usr/local/bin/aster-server", "/usr/local/bin/aster-controller", capture=True)
        assert {Path(path).name: digest for digest, path in
                (line.split() for line in packaged.stdout.splitlines())} == digests, packaged.stdout
        print("Packaged binaries match Cargo outputs: " + json.dumps(digests, sort_keys=True), flush=True)
        status, body = request("/api/catalogs/mock-local/namespaces")
        if slice_name in ["s2", "s3", "s7", "s5", "s8"]:
            assert status == 403 and "aster_demo" not in body, (status, body)
        else:
            assert status == 200 and "aster_demo" in body, (status, body)
        status, body = request("/api/catalogs/mock-local/namespaces/aster_demo/tables")
        if slice_name in ["s2", "s3", "s7", "s5", "s8"]:
            assert status == 403 and "orders" not in body, (status, body)
        else:
            assert status == 200 and "orders" in body, (status, body)
        if slice_name == "s6":
            for catalog, sentinel in [("p", "second"), ("o", "service.db.fixture"), ("c", "cubes")]:
                status, body = request(f"/api/catalogs/{catalog}/namespaces")
                assert status == 200 and sentinel in body, (catalog, status, body)
                assert "disposable-" not in body and "secret://" not in body
            for _ in range(30):
                observed = run(engine, "compose", "exec", "-T", "postgres", "psql", "-U", env.get("POSTGRES_USER", "aster"),
                               "-d", env.get("POSTGRES_DB", "aster"), "-Atc",
                               "SELECT id || ':' || health FROM catalogs WHERE id IN ('p','o','c') ORDER BY id", capture=True).stdout
                if observed.strip() == "c:healthy\no:healthy\np:healthy":
                    break
                time.sleep(1)
            else:
                raise AssertionError("controller did not reconcile all authenticated fixture catalogs: " + observed)
            print("S6 Compose registered target credentials and complete pagination passed (isolated fixtures)", flush=True)
        # Recreate only this disposable server with the existing global guard shut.
        env["ASTER_CATALOG_BINDINGS"] = "mock-local;mock-local;mock-local;protected"
        run(engine, "compose", "up", "-d", "--no-deps", "--force-recreate", "server")
        healthy()
        for path, payload in [
            ("/api/catalogs/mock-local/namespaces", None),
            ("/api/catalogs/mock-local/namespaces/aster_demo/tables", None),
            ("/api/ai", {"prompt": "Explain orders", "sql": "SELECT * FROM orders"}),
        ]:
            status, body = request(path, payload)
            assert status == 403, (path, status, body)
            assert all(value not in body for value in ["aster_demo", "order_id", "decimal(12,2)"]), body
        print("S1 actual-server allowed/denied metadata controls passed", flush=True)
        if slice_name in ["s3", "s7", "s5", "s8"]:
            sid = uuid.uuid4().hex
            session = json.dumps({"sid": sid, "subject": "alice", "roles": ["editor" if slice_name == "s5" else "viewer"], "groups": [group],
                "user_uuid": user, "verified": True, "created_at": int(time.time()),
                "last_seen": int(time.time()), "user_agent": None})
            run(engine, "compose", "exec", "-T", "valkey", "valkey-cli", "-a", "aster-dev",
                "SET", f"aster:v1:session:{sid}", session, "EX", "3600")
            selection = {"path": "compiled/sales.json", "sha256": hashlib.sha256(document).hexdigest(), "object": "/schema/0"}
            cookie = f"aster_session={sid}"
            run(engine, "compose", "exec", "-T", "authority", sys.executable, "-c",
                "import urllib.request,ssl; print(urllib.request.urlopen('https://authority:8443/api/v3/core/users/', context=ssl.create_default_context(cafile='/fixture/cert.pem')).read())")
            status, body = request("/aster.v1.Aster/ListContracts", {}, cookie)
            if status != 200:
                run(engine, "compose", "logs", "server", "authority")
            assert status == 200 and "compiled/sales.json" in body, (status, body)
            status, body = request("/aster.v1.Aster/PrepareContractQuery", selection, cookie)
            assert status == 200, (status, body)
            context = json.loads(json.loads(body)["contextJson"])
            assert context["binding"]["physicalName"] == "orders", context
            assert context["observation"]["status"] == "denied", context
            assert "NET_AFTER_REFUNDS" in body, body
            if slice_name == "s8":
                env["ASTER_CATALOG_BINDINGS"] = "mock-local;mock-local;mock-local;unprotected"
                run(engine, "compose", "up", "-d", "--no-deps", "--force-recreate", "server")
                healthy()
                inventory_request = {"catalog": "mock-local", "namespaceSegments": ["aster_demo"]}
                status, body = request("/aster.v1.Aster/ListCatalogInventory", inventory_request, cookie)
                assert status == 200, (status, body)
                inventory = json.loads(json.loads(body)["contextJson"])
                assert inventory["physical"]["status"] == "present", inventory
                assert any(entry["physicalName"] == "orders" and entry["contract"]["status"] == "admitted" for entry in inventory["entries"]), inventory
                status, body = request("/catalog", cookie=cookie)
                assert status == 200 and "Catalog inventory" in body and "orders" in body, (status, body)
                (bundle / "member").unlink()
                status, body = request("/aster.v1.Aster/ListCatalogInventory", inventory_request, cookie)
                assert status == 200 and json.loads(json.loads(body)["contextJson"])["entries"] == [], (status, body)
                status, body = request("/aster.v1.Aster/ListTables", inventory_request, cookie)
                assert status == 200 and "orders" not in body, (status, body)
                (bundle / "member").touch()
                print("S8 actual-server inventory, UI and legacy RPC revocation controls passed", flush=True)
            if slice_name == "s5":
                status, body = request("/api/llm/s5", {"base_url":"https://helper.invalid", "model":"fixture", "api_key":"disposable"}, cookie, "PUT")
                assert status == 200, (status, body)
            if slice_name == "s7":
                from playwright.sync_api import sync_playwright, expect
                with sync_playwright() as pw:
                    browser = pw.chromium.launch(executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE"))
                    context = browser.new_context()
                    base = f"http://127.0.0.1:{ports[0]}"
                    context.add_cookies([{"name": "aster_session", "value": sid, "url": base}])
                    page = context.new_page()
                    calls = []
                    page.on("request", lambda req: calls.append(req.url))
                    page.goto(base + "/catalog")
                    page.get_by_label("Declared contract", exact=True).select_option(index=1)
                    page.get_by_label("Declared object", exact=True).select_option("/schema/0")
                    expect(page.locator("#contract-preparation")).to_contain_text("NET_AFTER_REFUNDS")
                    expect(page.locator("#contract-preparation")).to_contain_text("denied")
                    page.get_by_label("Query draft", exact=True).fill("SELECT 1 -- manual draft")
                    page.get_by_role("button", name="Review draft with context", exact=True).click()
                    expect(page.locator("#contract-review")).to_contain_text("NET_AFTER_REFUNDS")
                    expect(page.locator("#contract-review")).to_contain_text("SELECT 1 -- manual draft")
                    (bundle / "member").unlink()
                    page.get_by_role("button", name="Review draft with context", exact=True).click()
                    expect(page.locator("#contract-review")).to_be_empty()
                    expect(page.locator("#contract-document")).to_be_empty()
                    assert not any("/api/query" in url or "ExecuteQuery" in url for url in calls)
                    browser.close()
                (bundle / "member").touch()
                print("S7 Compose browser manual review and revocation passed; zero execution requests", flush=True)
            (bundle / "member").unlink()
            status, body = request("/aster.v1.Aster/GetContract", selection, cookie)
            assert status == 403 and "INTERNAL_ONLY_MEANING" not in body, (status, body)
            if slice_name == "s5":
                status, body = request("/api/ai", {"helper":"s5", "prompt":"Draft the selected meaning", "contractSelection":selection}, cookie)
                assert status == 403 and "INTERNAL_ONLY_MEANING" not in body, (status, body)
                print("S5 Compose selected assist refuses revoked authorization before unreachable helper", flush=True)
            print("S3 actual-server contract discovery, qualified binding and fresh revocation passed", flush=True)
        if slice_name == "s2":
            env["ASTER_CATALOG_BINDINGS"] = "mock-local;mock-local;mock-local;unprotected"
            run(engine, "compose", "up", "-d", "--no-deps", "--force-recreate", "server")
            healthy()
            for path in ["/api/contracts", "/contracts"]:
                status, body = request(path)
                assert status in [200, 403], (status, body)
                assert "INTERNAL_ONLY" not in body, body
            (bundle / "compiled/sales.json").write_bytes(document + b"\n")
            run(engine, "compose", "up", "-d", "--no-deps", "--force-recreate", "server")
            for _ in range(30):
                logs = run(engine, "compose", "logs", "server", capture=True).stdout
                if "compiled bundle digest mismatch" in logs:
                    break
                time.sleep(1)
            else:
                raise AssertionError("configured digest mismatch did not refuse startup")
            try:
                assert request("/healthz")[0] != 200, "invalid bundle became healthy"
            except (OSError, urllib.error.URLError):
                pass
            print("S2 compiled-only allowed control, internal disclosure and invalid-startup controls passed", flush=True)
        if slice_name == "s6":
            config["services"]["server"]["environment"]["ASTER_SECRET_STORE"] = "memory"
            override.write_text(json.dumps(config))
            run(engine, "compose", "up", "-d", "--no-deps", "--force-recreate", "server")
            for _ in range(30):
                logs = run(engine, "compose", "logs", "server", capture=True).stdout
                if "configured catalog secret is unavailable" in logs:
                    break
                time.sleep(1)
            else:
                raise AssertionError("missing selected catalog secret did not refuse startup")
            assert all(f"disposable-{id}" not in logs for id in ["p", "o", "c"])
            try:
                assert request("/healthz")[0] != 200, "missing secret became healthy"
            except (OSError, urllib.error.URLError):
                pass
            print("S6 Compose selected-store failure refused startup without secret disclosure", flush=True)
    finally:
        if started:
            run("just", "compose-down")
            run(engine, "compose", "down", "--volumes", "--remove-orphans")
            running = run(engine, "ps", "--filter", f"label=com.docker.compose.project={project}", "--format", "{{.ID}}", capture=True)
            assert not running.stdout.strip(), "isolated containers survived cleanup"
print(f"odcs-compose-smoke {slice_name} OK")
