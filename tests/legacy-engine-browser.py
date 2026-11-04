"""Browser proof that a legacy mixed-engine notebook keeps its cell pins on save."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request
from playwright.sync_api import sync_playwright

def unused_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]

with tempfile.TemporaryDirectory(prefix="aster-legacy-engine-") as work:
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
        ASTER_ENGINES="mock-a;mock;local,mock-b;mock;local",
        ASTER_CATALOGS="mock;mock;local",
        ASTER_CATALOG_BINDINGS="mock;mock-a;mock;unprotected",
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
                    executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE")
                )
                page = browser.new_page()
                page.goto(base + "/dev-login", wait_until="networkidle")
                created = page.request.put(
                    base + "/api/notebooks/legacy",
                    headers={"If-None-Match": "*"},
                    data={
                        "id": "legacy",
                        "title": "Legacy",
                        "cells": [
                            {"id": "c1", "sql": "SELECT 1", "engine": "mock-a"},
                            {"id": "c2", "sql": "SELECT 2", "engine": "mock-b"},
                        ],
                    },
                )
                assert created.ok, created.text()
                page.goto(base + "/notebooks/legacy", wait_until="networkidle")
                page.wait_for_timeout(1200)
                assert page.locator("#notebook-engine").input_value() == "__legacy"
                page.locator(".cell[data-id=c1] .editor").fill("SELECT 10")
                page.wait_for_timeout(2600)
                saved = page.request.get(base + "/api/notebooks/legacy")
                assert saved.ok, saved.text()
                notebook = saved.json()
                assert notebook["engine"] is None, notebook
                assert notebook["cells"][0]["engine"] == "mock-a", notebook
                assert notebook["cells"][1]["engine"] == "mock-b", notebook
                text = Path(work + "/notebooks/legacy.aster").read_text()
                assert "# engine:" not in text, text
                assert "engine=mock-a" in text and "engine=mock-b" in text, text
                browser.close()
        finally:
            server.terminate()
            server.wait(timeout=10)
print("legacy-engine-browser OK: mixed cell engines survive autosave")
