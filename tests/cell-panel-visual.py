"""Disposable real-browser proof that the cell conversation is a narrow column.

Opens the cell panel on a real page and measures it against the cell it belongs
to: a takeover of the cell would fail, and so would a panel that collapsed to
nothing. Screenshots land in /tmp for the visual double check. No provider calls.
"""
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request
from playwright.sync_api import sync_playwright, expect

RATIO_LIMIT = 0.30
RATIO_FLOOR = 0.15
SHOTS = (
    ("/tmp/aster-cell-panel-wide.png", {"width": 1440, "height": 1000}),
    ("/tmp/aster-cell-panel-narrow.png", {"width": 900, "height": 900}),
)


def unused_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


with tempfile.TemporaryDirectory(prefix="aster-cell-panel-") as work:
    port = unused_port()
    base = f"http://127.0.0.1:{port}"
    env = os.environ.copy()
    for key in ("DATABASE_URL", "ASTER_STATE_URL", "ASTER_OIDC_ISSUER",
                "ASTER_CONVERSATION_DATABASE_URL", "ASTER_EXCHANGE_DATABASE_URL"):
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
    )
    binary = os.environ.get("ASTER_SERVER_BIN", "target/debug/aster-server")
    with open(work + "/server.log", "w") as log:
        server = subprocess.Popen([str(Path(binary).resolve())], env=env, stdout=log, stderr=log)
        try:
            for _ in range(100):
                try:
                    urllib.request.urlopen(base + "/healthz", timeout=1).close()
                    break
                except OSError:
                    time.sleep(.1)
            else:
                raise AssertionError("disposable server did not start")
            with sync_playwright() as pw:
                browser = pw.chromium.launch(
                    headless=True,
                    executable_path=os.environ.get("ASTER_BROWSER_EXECUTABLE"),
                )
                page = browser.new_page(viewport={"width": 1440, "height": 1000})
                errors = []
                page.on("pageerror", lambda error: errors.append(str(error)))
                page.goto(base + "/dev-login")
                assert page.request.put(
                    base + "/api/notebooks/visual-check",
                    headers={"If-None-Match": "*"},
                    data={
                        "id": "visual-check",
                        "title": "Visual check",
                        "cells": [
                            {"id": "q1", "sql": "select orderkey from orders", "engine": "mock"},
                            {"id": "q2", "sql": "select 1", "engine": "mock"},
                        ],
                    },
                ).ok
                page.goto(base + "/notebooks/visual-check")
                cell = page.locator(".cell").first
                panel = cell.locator(".cell-chat")
                expect(panel).to_be_hidden()
                cell.locator('[data-action="ai"]').click()
                expect(panel).to_be_visible()
                expect(cell.locator(".cell-chat-history")).to_be_visible()

                measured = page.evaluate(
                    """() => {
                        const cell = document.querySelector('.cell');
                        const chat = cell.querySelector('.cell-chat');
                        return {cell: cell.getBoundingClientRect().width,
                                chat: chat.getBoundingClientRect().width};
                    }"""
                )
                ratio = measured["chat"] / measured["cell"]
                assert ratio <= RATIO_LIMIT, f"cell chat took {ratio:.1%} of the cell: {measured}"
                assert ratio >= RATIO_FLOOR, f"cell chat collapsed to {ratio:.1%}: {measured}"

                for path, viewport in SHOTS:
                    page.set_viewport_size(viewport)
                    expect(panel).to_be_visible()
                    page.screenshot(path=path, full_page=True)

                cell.locator(".cell-chat-close").click()
                expect(panel).to_be_hidden()
                assert not errors, errors
                browser.close()
            print(f"cell-panel-visual OK (chat {ratio:.1%} of the cell, limit {RATIO_LIMIT:.0%})")
        finally:
            server.terminate()
            server.wait(timeout=15)
