"""Disposable browser proof of notebook-scoped helper selection."""

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

from playwright.sync_api import expect, sync_playwright


requests_seen = []


class FakeHelper(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        requests_seen.append(body)
        payload = json.dumps({"choices": [{"message": {"content": "SELECT 42"}}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def unused_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def put_notebook(page, base, name):
    response = page.request.put(base + "/api/notebooks/" + name, headers={"If-None-Match": "*"}, data={
        "id": name,
        "title": name,
        "cells": [{"id": "c1", "sql": "SELECT 1", "engine": "mock"}],
    })
    assert response.ok, response.text()


def put_helper(page, base, name, endpoint):
    response = page.request.put(base + "/api/llm/" + name, data={
        "base_url": endpoint,
        "model": name,
        "api_key": "fake",
    })
    assert response.ok, response.text()


def selected(page, name):
    expect(page.locator("#helper")).to_have_value(name)


def wait_helper(page, base, notebook, wanted):
    for _ in range(50):
        value = page.request.get(base + f"/api/notebooks/{notebook}/helper").json()["helper"]
        if value == wanted:
            return
        page.wait_for_timeout(50)
    raise AssertionError(f"{notebook} did not save helper {wanted}")


mock = ThreadingHTTPServer(("127.0.0.1", 0), FakeHelper)
threading.Thread(target=mock.serve_forever, daemon=True).start()
with tempfile.TemporaryDirectory(prefix="aster-helper-browser-") as work:
    port = unused_port()
    base = f"http://127.0.0.1:{port}"
    env = os.environ.copy()
    for key in ("DATABASE_URL", "ASTER_STATE_URL", "ASTER_OIDC_ISSUER", "ASTER_CONVERSATION_DATABASE_URL"):
        env.pop(key, None)
    env.update(
        ASTER_BIND=f"127.0.0.1:{port}",
        ASTER_METRICS_BIND=f"127.0.0.1:{unused_port()}",
        ASTER_METADATA_STORE="memory",
        ASTER_STATE_STORE="memory",
        ASTER_CONVERSATION_STORE="memory",
        ASTER_IDP_KIND="none",
        ASTER_DEV_LOGIN="1",
        ASTER_NOTEBOOK_DIR=work + "/notebooks",
        ASTER_ENGINES="mock;mock;local",
        ASTER_CATALOGS="mock;mock;local",
        ASTER_CATALOG_BINDINGS="mock;mock;mock;unprotected",
        ASTER_GRANTS="alice:mock",
    )
    binary = Path(os.environ.get("ASTER_SERVER_BIN", "target/debug/aster-server")).resolve()
    with open(work + "/server.log", "w") as log:
        server = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=log)
        try:
            for _ in range(100):
                try:
                    urllib.request.urlopen(base + "/healthz", timeout=1).close()
                    break
                except OSError:
                    time.sleep(0.1)
            else:
                raise AssertionError("disposable server did not start")

            with sync_playwright() as pw:
                browser = pw.chromium.launch(
                    headless=True, executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE")
                )
                alice = browser.new_page()
                alice.goto(base + "/dev-login?subject=alice&roles=editor")
                for name in ("one", "two", "legacy"):
                    put_notebook(alice, base, name)
                endpoint = f"http://127.0.0.1:{mock.server_port}/v1"
                for name in ("alpha", "beta"):
                    put_helper(alice, base, name, endpoint)
                git_head = subprocess.check_output(
                    ["git", "rev-parse", "HEAD"], cwd=work + "/notebooks", text=True
                ).strip()

                # A query while the first scoped read is pending must not erase
                # a legacy choice before its one-time migration.
                assert alice.request.put(base + "/api/state", data={
                    "notebook": "legacy", "helper": "alpha"
                }).ok
                pending = []
                writes = []

                def hold_legacy(route):
                    pending.append(route)

                def observe_state(route):
                    if route.request.method == "PUT":
                        response = route.fetch()
                        writes.append(True)
                        route.fulfill(response=response)
                    else:
                        route.continue_()

                alice.route("**/api/notebooks/legacy/helper", hold_legacy)
                alice.route("**/api/state", observe_state)
                alice.goto(base + "/notebooks/legacy")
                for _ in range(50):
                    if pending:
                        break
                    alice.wait_for_timeout(20)
                assert pending, "legacy helper GET was not requested"
                alice.locator('.cell [data-action="run"]').click()
                expect(alice.locator(".out-body")).to_have_count(1)
                alice.wait_for_timeout(200)
                wrote_before_load = bool(writes)
                pending[0].continue_()
                assert not wrote_before_load, "page wrote working state before loading helper choice"
                selected(alice, "alpha")
                for _ in range(50):
                    resume = alice.request.get(base + "/api/state").json()
                    if resume.get("cell") == "c1" and resume.get("engine") == "mock":
                        break
                    alice.wait_for_timeout(50)
                assert resume.get("cell") == "c1" and resume.get("engine") == "mock", resume
                alice.unroute("**/api/notebooks/legacy/helper", hold_legacy)
                alice.unroute("**/api/state", observe_state)
                alice.locator("#helper").select_option("personal/beta")
                wait_helper(alice, base, "legacy", "personal/beta")
                resume = alice.request.get(base + "/api/state").json()
                assert resume.get("cell") == "c1" and resume.get("engine") == "mock", resume

                boot_pending = []
                boot_writes = []

                def hold_one(route):
                    boot_pending.append(route)

                def observe_boot_state(route):
                    if route.request.method == "PUT":
                        boot_writes.append(True)
                    route.continue_()

                alice.route("**/api/notebooks/one/helper", hold_one)
                alice.route("**/api/state", observe_boot_state)
                alice.goto(base + "/notebooks/one")
                for _ in range(50):
                    if boot_pending:
                        break
                    alice.wait_for_timeout(20)
                assert boot_pending, "scoped helper read was not requested"
                assert not boot_writes, "notebook resume wrote before scoped helper read"
                assert alice.request.get(base + "/api/state").json().get("notebook") == "legacy"
                boot_pending[0].continue_()
                selected(alice, "")  # No implicit first-helper selection or write.
                for _ in range(50):
                    resume = alice.request.get(base + "/api/state").json()
                    if resume.get("notebook") == "one":
                        break
                    alice.wait_for_timeout(50)
                assert resume.get("notebook") == "one", resume
                alice.unroute("**/api/notebooks/one/helper", hold_one)
                alice.unroute("**/api/state", observe_boot_state)
                assert alice.request.get(base + "/api/notebooks/one/helper").json() == {"helper": None}
                alice.locator("#helper").select_option("personal/alpha")
                wait_helper(alice, base, "one", "personal/alpha")
                for _ in range(50):
                    resume = alice.request.get(base + "/api/state").json()
                    if resume.get("notebook") == "one":
                        break
                    alice.wait_for_timeout(50)
                assert resume.get("notebook") == "one", resume
                alice.goto(base + "/notebooks/two")
                selected(alice, "")
                alice.locator("#helper").focus()
                alice.keyboard.press("End")
                alice.keyboard.press("Enter")
                wait_helper(alice, base, "two", "personal/beta")
                alice.goto(base + "/notebooks/one")
                selected(alice, "personal/alpha")

                # A delayed read cannot cause a page-load write or choose a default.
                held = []

                def delay_selection(route):
                    response = route.fetch()
                    held.append((route, response))

                alice.route("**/api/notebooks/one/helper", delay_selection)
                alice.reload()
                for _ in range(50):
                    if held:
                        break
                    alice.wait_for_timeout(20)
                assert held, "selection GET was not requested"
                expect(alice.locator("#helper")).to_be_disabled()
                assert alice.request.put(
                    base + "/api/notebooks/one/helper", data={"helper": "beta"}
                ).ok
                held[0][0].fulfill(response=held[0][1])
                alice.unroute("**/api/notebooks/one/helper", delay_selection)
                selected(alice, "personal/alpha")
                assert alice.request.get(base + "/api/notebooks/one/helper").json() == {"helper": "beta"}
                alice.reload()
                selected(alice, "beta")
                alice.locator("#helper").select_option("personal/alpha")
                wait_helper(alice, base, "one", "personal/alpha")

                bob = browser.new_page()
                bob.goto(base + "/dev-login?subject=bob&roles=editor")
                put_helper(bob, base, "bob-helper", endpoint)
                bob.goto(base + "/notebooks/one")
                selected(bob, "")
                bob.locator("#helper").select_option("personal/bob-helper")
                wait_helper(bob, base, "one", "personal/bob-helper")
                alice.reload()
                selected(alice, "personal/alpha")

                assert alice.request.delete(base + "/api/llm/alpha").ok
                alice.reload()
                selected(alice, "personal/alpha")
                expect(alice.locator("#helper option:checked")).to_contain_text("unavailable")
                alice.locator("#chat-toggle").click()
                alice.locator("#chat-prompt").fill("Do not send")
                expect(alice.locator("#chat-send")).to_be_disabled()
                assert not requests_seen
                alice.locator("#helper").select_option("personal/beta")
                wait_helper(alice, base, "one", "personal/beta")
                alice.locator("#chat-prompt").fill("Use beta")
                alice.locator("#chat-send").click()
                expect(alice.locator("#chat-status")).to_contain_text("Saved")
                assert requests_seen[-1]["model"] == "beta"
                assert "helper" not in alice.request.get(base + "/api/notebooks/one").json()
                assert subprocess.check_output(
                    ["git", "rev-parse", "HEAD"], cwd=work + "/notebooks", text=True
                ).strip() == git_head

                # A saved shared reference and a personal helper with the same
                # name remain distinct; revocation leaves an unavailable choice.
                shared_visible = [True]

                def scoped_helpers(route):
                    helpers = [{"id": "qwen", "model": "personal-qwen", "scope": "personal", "ref": "personal/qwen"}]
                    if shared_visible[0]:
                        helpers.append({"id": "qwen", "model": "shared-qwen", "scope": "shared", "ref": "shared/qwen"})
                    route.fulfill(status=200, content_type="application/json", body=json.dumps(helpers))

                def saved_shared(route):
                    if route.request.method == "GET":
                        route.fulfill(status=200, content_type="application/json", body='{"helper":"shared/qwen"}')
                    else:
                        route.continue_()

                alice.route("**/api/llm", scoped_helpers)
                alice.route("**/api/notebooks/one/helper", saved_shared)
                alice.goto(base + "/notebooks/one")
                selected(alice, "shared/qwen")
                values = alice.locator("#helper option").evaluate_all("options => options.map(option => option.value)")
                assert "personal/qwen" in values and "shared/qwen" in values, values
                shared_visible[0] = False
                alice.reload()
                selected(alice, "shared/qwen")
                expect(alice.locator("#helper option:checked")).to_contain_text("unavailable")
                alice.locator("#chat-toggle").click()
                expect(alice.locator("#chat-send")).to_be_disabled()
                browser.close()
            print("ai-selection-browser OK")
        finally:
            server.terminate()
            server.wait(timeout=15)
            mock.shutdown()
            mock.server_close()
