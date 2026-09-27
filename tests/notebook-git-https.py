#!/usr/bin/env python3
"""Disposable TLS Git smart-HTTP fixture for Aster's explicit Sync adapter."""

import base64
import http.server
import os
import pathlib
import shutil
import ssl
import subprocess
import tempfile
import threading
from urllib.parse import urlsplit


PROJECT_ROOT = pathlib.Path(__file__).resolve().parent.parent
TOKEN = "disposable-test-token"


def run(*args, cwd=None, env=None):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True)
    if result.returncode:
        raise RuntimeError((args, result.stderr.decode(errors="replace")))
    return result.stdout.decode().strip()


with tempfile.TemporaryDirectory(prefix="aster-git-https-test-") as temp:
    root = pathlib.Path(temp)
    cert = root / "cert.pem"
    key = root / "key.pem"
    run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
        "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1",
        "-keyout", str(key), "-out", str(cert))
    bare = root / "repo.git"
    work = root / "seed"
    run("git", "init", "--bare", "-q", str(bare))
    run("git", "init", "-q", str(work))
    (work / "a.aster").write_text("-- notebook test\n")
    run("git", "add", "a.aster", cwd=work)
    run("git", "-c", "user.name=aster", "-c", "user.email=aster@localhost", "commit", "-q", "-m", "seed", cwd=work)
    run("git", "push", str(bare), "HEAD:refs/heads/main", cwd=work)
    seed_oid = run("git", "rev-parse", "refs/heads/main", cwd=bare)
    run("git", "config", "http.receivepack", "true", cwd=bare)
    checkout = root / "checkout"
    run("git", "clone", "-q", "--branch", "main", str(bare), str(checkout))

    seen = []
    deny_push = root / "deny-push"

    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_):
            pass

        def do_GET(self):
            self.handle_git()

        def do_POST(self):
            self.handle_git()

        def handle_git(self):
            auth = self.headers.get("Authorization", "")
            valid = auth == "Basic " + base64.b64encode(f"x-access-token:{TOKEN}".encode()).decode()
            seen.append((self.path, valid))
            if not valid:
                self.send_response(401)
                self.send_header("WWW-Authenticate", 'Basic realm="aster-test"')
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
            if deny_push.exists() and "git-receive-pack" in self.path:
                self.send_response(403)
                self.send_header("Content-Length", "0")
                self.send_header("Connection", "close")
                self.end_headers()
                self.close_connection = True
                return
            length = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(length)
            request = urlsplit(self.path)
            env = os.environ.copy()
            env.update({
                "GIT_PROJECT_ROOT": str(root),
                "GIT_HTTP_EXPORT_ALL": "1",
                "PATH_INFO": request.path,
                "QUERY_STRING": request.query,
                "REQUEST_METHOD": self.command,
                "CONTENT_TYPE": self.headers.get("Content-Type", ""),
                "CONTENT_LENGTH": str(length),
                "REMOTE_USER": "aster-test",
            })
            result = subprocess.run(["git", "http-backend"], input=body, env=env, capture_output=True)
            if result.returncode:
                raise RuntimeError(result.stderr.decode(errors="replace"))
            header, response_body = result.stdout.split(b"\r\n\r\n", 1)
            status = 200
            response_headers = []
            for line in header.decode().splitlines():
                name, value = line.split(":", 1)
                if name.lower() == "status":
                    status = int(value.strip().split()[0])
                else:
                    response_headers.append((name, value.strip()))
            self.send_response(status)
            for name, value in response_headers:
                self.send_header(name, value)
            self.send_header("Content-Length", str(len(response_body)))
            self.end_headers()
            self.wfile.write(response_body)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(str(cert), str(key))
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f"https://127.0.0.1:{server.server_port}/repo.git"
    real_git = shutil.which("git")
    assert real_git is not None
    shim_dir = root / "git-shim"
    shim_dir.mkdir()
    argv_log = root / "git-argv-check.log"
    shim = shim_dir / "git"
    shim.write_text(
        "#!/usr/bin/env python3\n"
        "import os, sys\n"
        "token = os.environ.get('ASTER_TEST_HTTPS_TOKEN', '')\n"
        "with open(os.environ['ASTER_TEST_GIT_ARGV_LOG'], 'a', encoding='ascii') as log:\n"
        "    log.write('1\\n' if token and any(token in arg for arg in sys.argv) else '0\\n')\n"
        f"os.execv({real_git!r}, [{real_git!r}, *sys.argv[1:]])\n"
    )
    shim.chmod(0o700)
    env = os.environ.copy()
    env["PATH"] = f"{shim_dir}:{PROJECT_ROOT}/.devenv/profile/bin:{env.get('PATH', '')}"
    env["RUSTC_WRAPPER"] = ""
    common_git = pathlib.Path(run("git", "-C", str(PROJECT_ROOT), "rev-parse",
                                  "--path-format=absolute", "--git-common-dir"))
    env.setdefault("CARGO_TARGET_DIR", str(common_git.parent / "target"))
    env["PROTOC"] = f"{PROJECT_ROOT}/.devenv/profile/bin/protoc"
    env["ASTER_TEST_HTTPS_CHECKOUT"] = str(checkout)
    env["ASTER_TEST_HTTPS_BARE"] = str(bare)
    env["ASTER_TEST_HTTPS_URL"] = url
    env["ASTER_TEST_HTTPS_CA"] = str(cert)
    env["ASTER_TEST_HTTPS_TOKEN"] = TOKEN
    env["ASTER_TEST_HTTPS_SEED_OID"] = seed_oid
    env["ASTER_TEST_HTTPS_DENY_PUSH_FILE"] = str(deny_push)
    env["ASTER_TEST_GIT_ARGV_LOG"] = str(argv_log)
    selected = subprocess.run(
        ["cargo", "test", "-p", "aster-server", "--test", "notebook_git_https", "--", "--ignored", "--list"],
        cwd=PROJECT_ROOT, env=env, text=True, capture_output=True,
    )
    selected_names = (
        "notebook_git_https_sync_pushes_exact_branch_without_persisting_token: test",
        "notebook_git_https_z_clone_pins_moved_default_without_persisting_token: test",
    )
    if selected.returncode or any(name not in selected.stdout for name in selected_names):
        raise RuntimeError("HTTPS Git test selection failed")
    result = subprocess.run(
        ["cargo", "test", "-p", "aster-server", "--test", "notebook_git_https", "--", "--ignored", "--nocapture", "--test-threads=1"],
        cwd=PROJECT_ROOT, env=env, text=True, capture_output=True,
    )
    output = result.stdout + result.stderr
    if TOKEN in output:
        raise RuntimeError("HTTPS Git test exposed its disposable token")
    print(output, end="")
    if result.returncode:
        raise RuntimeError("HTTPS Git test failed")
    assert argv_log.read_text().splitlines()
    assert set(argv_log.read_text().splitlines()) == {"0"}, "Git argv contained token"
    assert any(valid and "git-upload-pack" in path for path, valid in seen)
    assert any(valid and "git-receive-pack" in path for path, valid in seen)
    print("notebook-git-https OK")
    server.shutdown()
    server.server_close()
