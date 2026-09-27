#!/usr/bin/env python3
"""Disposable Trino 483 authentication and Iceberg policy fixture (V3b1b)."""

import base64
import hashlib
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request


COMPOSE = Path(__file__).with_name("trino-policy.compose.yaml")
ROOT = Path(__file__).resolve().parents[1]


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return str(sock.getsockname()[1])


def redacted(output, values):
    for key in ("V3B1B_POLARIS_SECRET", "V3B1B_S3_KEY", "V3B1B_S3_SECRET",
                "V3B1B_JWT_KEY", "V3B1B_JWT"):
        if key in values:
            output = output.replace(values[key], "<redacted>")
    return output


def compose(project, values, *args):
    result = subprocess.run(
        ["docker", "compose", "-f", str(COMPOSE), "-p", project, *args],
        env={**os.environ, **values}, capture_output=True, text=True,
    )
    if result.returncode:
        lines = [line for line in redacted(result.stdout + result.stderr, values).splitlines()
                 if any(marker in line for marker in ("Error", "Exception", "failed", "denied", "V3B1B"))]
        raise RuntimeError(f"compose {args[:2]} exited {result.returncode}: " + " | ".join(lines[-8:]))
    return result.stdout + result.stderr


def write(path, content):
    path.write_text(content)
    path.chmod(0o600)


def generate_trino_config(work, values):
    config = work / "trino"
    catalog = config / "catalog"
    catalog.mkdir(parents=True)
    key = secrets.token_bytes(48)
    values["V3B1B_JWT_KEY"] = base64.b64encode(key).decode()
    # Trino 483 FileSigningKeyLocator decodes this file as standard base64.
    write(config / "jwt.key", values["V3B1B_JWT_KEY"])
    def openssl(*args):
        result = subprocess.run(["openssl", *args], capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError("fixture TLS certificate generation failed")

    openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
            "-keyout", str(work / "ca.key"), "-out", str(work / "ca.crt"),
            "-subj", "/CN=Aster disposable fixture CA",
            "-addext", "basicConstraints=critical,CA:TRUE",
            "-addext", "keyUsage=keyCertSign,cRLSign")
    openssl("req", "-new", "-newkey", "rsa:2048", "-nodes",
            "-keyout", str(work / "tls.key"), "-out", str(work / "tls.csr"),
            "-subj", "/CN=localhost")
    write(work / "tls.ext", "\n".join((
        "subjectAltName=DNS:localhost,DNS:trino,IP:127.0.0.1",
        "basicConstraints=critical,CA:FALSE",
        "keyUsage=digitalSignature,keyEncipherment",
        "extendedKeyUsage=serverAuth",
    )) + "\n")
    openssl("x509", "-req", "-in", str(work / "tls.csr"),
            "-CA", str(work / "ca.crt"), "-CAkey", str(work / "ca.key"),
            "-CAcreateserial", "-out", str(work / "tls.crt"), "-days", "1",
            "-sha256", "-extfile", str(work / "tls.ext"))
    openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
            "-keyout", str(work / "other-ca.key"), "-out", str(work / "other-ca.crt"),
            "-subj", "/CN=Aster unrelated fixture CA",
            "-addext", "basicConstraints=critical,CA:TRUE",
            "-addext", "keyUsage=keyCertSign,cRLSign")
    write(config / "tls.pem", (work / "tls.key").read_text() + (work / "tls.crt").read_text())
    write(config / "config.properties", "\n".join((
        "coordinator=true",
        "node-scheduler.include-coordinator=true",
        "http-server.http.port=8080",
        "http-server.https.enabled=true",
        "http-server.https.port=8443",
        "http-server.https.keystore.path=/etc/trino/tls.pem",
        "http-server.authentication.type=JWT",
        "http-server.authentication.jwt.key-file=/etc/trino/jwt.key",
        "http-server.authentication.jwt.required-issuer=aster-v3b1b",
        "http-server.authentication.jwt.required-audience=trino-v3b1b",
        "internal-communication.shared-secret=" + secrets.token_urlsafe(48),
        "discovery.uri=https://localhost:8443",
        "catalog.management=static",
        "query.max-memory=1GB",
        "query.max-memory-per-node=512MB",
    )) + "\n")
    write(config / "node.properties", "node.environment=v3b1b\nnode.id=trino-1\nnode.data-dir=/data/trino\n")
    write(config / "jvm.config", "\n".join((
        "-server", "-Xmx2G", "-XX:+UseG1GC", "-XX:G1HeapRegionSize=32M",
        "-XX:+ExplicitGCInvokesConcurrent", "-XX:+HeapDumpOnOutOfMemoryError",
        "-XX:+ExitOnOutOfMemoryError", "-Djdk.attach.allowAttachSelf=true",
    )) + "\n")
    write(config / "access-control.properties",
          "access-control.name=file\nsecurity.config-file=/etc/trino/rules.json\n")
    rules = {
        "impersonation": [{"original_user": "^aster_service$",
                           "new_user": "fixture_admin|alice|bob", "allow": True}],
        "catalogs": [
            {"user": "^fixture_admin$", "catalog": "^(polaris|tpch)$", "allow": "all"},
            {"user": "^(alice|bob)$", "catalog": "^polaris$", "allow": "read-only"},
        ],
        "schemas": [{"user": "^fixture_admin$", "catalog": "^polaris$",
                     "schema": "^sales$", "owner": True}],
        "tables": [
            {"user": "^fixture_admin$", "catalog": "^polaris$", "schema": "^sales$",
             "privileges": ["SELECT", "INSERT", "DELETE", "UPDATE", "OWNERSHIP"]},
            {"user": "^alice$", "catalog": "^polaris$", "schema": "^sales$",
             "table": "^orders$", "privileges": ["SELECT"]},
            {"user": "^bob$", "catalog": "^polaris$", "schema": "^sales$",
             "table": "^orders$", "privileges": []},
            {"user": "^bob$", "catalog": "^polaris$", "schema": "^sales$",
             "table": "^storage_probe$", "privileges": ["SELECT"]},
        ],
    }
    write(config / "rules.json", json.dumps(rules))
    write(catalog / "tpch.properties", "connector.name=tpch\n")
    write(catalog / "polaris.properties", "\n".join((
        "connector.name=iceberg",
        "iceberg.catalog.type=rest",
        "iceberg.rest-catalog.uri=http://polaris:8181/api/catalog",
        "iceberg.rest-catalog.security=OAUTH2",
        "iceberg.rest-catalog.oauth2.credential=root:${ENV:CLIENT_SECRET}",
        "iceberg.rest-catalog.oauth2.scope=PRINCIPAL_ROLE:ALL",
        "iceberg.rest-catalog.warehouse=v3b1b_catalog",
        "iceberg.rest-catalog.http-headers=Polaris-Realm: POLARIS",
        "fs.s3.enabled=true",
        "s3.endpoint=http://rustfs:9000",
        "s3.path-style-access=true",
        "s3.region=us-west-2",
        "s3.security-mapping.enabled=true",
        "s3.security-mapping.config-file=/etc/trino/s3-mapping.json",
    )) + "\n")
    mapping = {"mappings": [{"user": "^(fixture_admin|alice)$",
                             "prefix": "s3://" + values["V3B1B_BUCKET"] + "/",
                             "accessKey": values["V3B1B_S3_KEY"],
                             "secretKey": values["V3B1B_S3_SECRET"]}]}
    write(config / "s3-mapping.json", json.dumps(mapping))
    values["V3B1B_TRINO_CONFIG_DIR"] = str(config)
    return work / "ca.crt"


def jwt(values, *, subject="aster_service", audience="trino-v3b1b", expires=900):
    def enc(value):
        return base64.urlsafe_b64encode(json.dumps(value, separators=(",", ":")).encode()).rstrip(b"=")
    header = enc({"alg": "HS256", "typ": "JWT"})
    payload = enc({"sub": subject, "iss": "aster-v3b1b", "aud": audience,
                   "iat": int(time.time()), "exp": int(time.time()) + expires})
    signed = header + b"." + payload
    signature = base64.urlsafe_b64encode(hmac.new(
        base64.b64decode(values["V3B1B_JWT_KEY"]), signed, hashlib.sha256).digest()).rstrip(b"=")
    return (signed + b"." + signature).decode()


def request(url, *, token=None, user=None, body=None, context=None, method=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    if user:
        headers["X-Trino-User"] = user
    if body is not None:
        headers["Content-Type"] = "text/plain; charset=utf-8"
    req = urllib.request.Request(url, data=body, method=method, headers=headers)
    with urllib.request.urlopen(req, context=context, timeout=12) as response:
        return response.status, response.read()


def polaris_token(base, values):
    encoded = base64.b64encode(("root:" + values["V3B1B_POLARIS_SECRET"]).encode()).decode()
    req = urllib.request.Request(
        base + "/api/catalog/v1/oauth/tokens",
        data=b"grant_type=client_credentials&scope=PRINCIPAL_ROLE%3AALL",
        headers={"Authorization": "Basic " + encoded, "Polaris-Realm": "POLARIS",
                 "Content-Type": "application/x-www-form-urlencoded"},
    )
    with urllib.request.urlopen(req, timeout=5) as response:
        return json.load(response)["access_token"]


def polaris_api(base, token, path, *, body=None, method="GET"):
    headers = {"Authorization": "Bearer " + token, "Polaris-Realm": "POLARIS",
               "Content-Type": "application/json"}
    req = urllib.request.Request(base + path, data=json.dumps(body).encode() if body else None,
                                 method=method, headers=headers)
    with urllib.request.urlopen(req, timeout=5) as response:
        payload = response.read()
        return json.loads(payload) if payload else {}


def trino_query(base, context, token, user, sql):
    headers = {"Authorization": "Bearer " + token, "X-Trino-User": user,
               "X-Trino-Catalog": "polaris", "X-Trino-Schema": "sales"}
    def fetch(url, data=None):
        req = urllib.request.Request(url, data=data, headers=headers)
        with urllib.request.urlopen(req, context=context, timeout=30) as response:
            return json.load(response)
    payload = fetch(base + "/v1/statement", sql.encode())
    rows = []
    while True:
        if "error" in payload:
            raise RuntimeError("Trino query denied: " + payload["error"].get("message", "unknown"))
        rows.extend(payload.get("data", []))
        next_uri = payload.get("nextUri")
        if not next_uri:
            return rows
        parts = urllib.parse.urlsplit(next_uri)
        payload = fetch(base + parts.path + ("?" + parts.query if parts.query else ""))


def ready(url):
    try:
        request(url)
        return True
    except (urllib.error.URLError, TimeoutError):
        return False


def expect_query_denied(base, context, token, user, sql, marker):
    try:
        trino_query(base, context, token, user, sql)
    except RuntimeError as error:
        if marker.lower() not in str(error).lower():
            raise AssertionError(f"Trino denial did not name {marker}") from error
    else:
        raise AssertionError(f"Trino unexpectedly allowed {user} query")


def expect_http_status(url, expected, *, context=None, token=None, user=None, body=None):
    try:
        request(url, context=context, token=token, user=user, body=body)
    except urllib.error.HTTPError as error:
        if error.code != expected:
            raise AssertionError(f"HTTP {error.code}, expected {expected}") from error
        error.close()
    else:
        raise AssertionError(f"HTTP {expected} denial was absent")


def aster_adapter(work, values, cert, token, *, red):
    token_file = work / "service.token"
    write(token_file, token)
    expired_file = work / "expired.token"
    write(expired_file, jwt(values, expires=-5))
    env = {**os.environ,
           "RUSTC_WRAPPER": "",
           "CARGO_TARGET_DIR": str(ROOT.parent.parent / "target"),
           "V3B1B_TRINO_ENDPOINT": "https://127.0.0.1:" + values["V3B1B_TRINO_PORT"],
           "V3B1B_TRINO_TOKEN_FILE": str(token_file),
           "V3B1B_TRINO_EXPIRED_TOKEN_FILE": str(expired_file),
           "V3B1B_TRINO_CA_FILE": str(cert),
           "V3B1B_TRINO_WRONG_CA_FILE": str(work / "other-ca.crt")}
    cargo = ROOT / ".devenv/profile/bin/cargo"
    attacker_hits = []
    redirect_hits = []
    class Attacker(BaseHTTPRequestHandler):
        def do_POST(self):
            attacker_hits.append(self.headers.get("Authorization"))
            self.send_response(200)
            self.end_headers()
        def do_GET(self):
            self.do_POST()
        def log_message(self, *_):
            pass
    attacker = ThreadingHTTPServer(("127.0.0.1", 0), Attacker)
    class Redirect(BaseHTTPRequestHandler):
        def do_POST(self):
            redirect_hits.append(1)
            self.send_response(307)
            self.send_header("Location", f"http://127.0.0.1:{attacker.server_port}/stolen")
            self.end_headers()
        def log_message(self, *_):
            pass
    redirect = ThreadingHTTPServer(("127.0.0.1", 0), Redirect)
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(str(work / "tls.crt"), str(work / "tls.key"))
    redirect.socket = tls.wrap_socket(redirect.socket, server_side=True)
    env["V3B1B_REDIRECT_ENDPOINT"] = f"https://127.0.0.1:{redirect.server_port}"
    threads = [threading.Thread(target=server.serve_forever, daemon=True)
               for server in (attacker, redirect)]
    for thread in threads:
        thread.start()
    try:
        result = subprocess.run(
            [str(cargo), "test", "-p", "aster-engines", "--test", "trino_data_policy",
             "verified_subject_reaches_trino_table_policy", "--", "--ignored", "--exact", "--nocapture"],
            cwd=ROOT, env=env, capture_output=True, text=True,
        )
    finally:
        for server in (redirect, attacker):
            server.shutdown()
            server.server_close()
        for thread in threads:
            thread.join(timeout=2)
    output = redacted(result.stdout + result.stderr, {**values, "V3B1B_JWT": token})
    if red:
        if result.returncode == 0 or "verified Alice reads independently seeded Iceberg rows" not in output \
                or "Trino authenticated delegation is not configured" not in output:
            raise RuntimeError("adapter RED was not the expected assertion: " + output[-1200:])
        print("V3B1B_ASTER_ADAPTER_RED", flush=True)
    elif result.returncode or "1 passed" not in output:
        raise RuntimeError("adapter GREEN did not select and pass its test: " + output[-1200:])
    else:
        if len(redirect_hits) != 1 or attacker_hits:
            raise AssertionError("protected Trino client followed a cross-origin redirect")
        print("V3B1B_ASTER_ADAPTER_GREEN", flush=True)
    if red:
        return
    result = subprocess.run(
        [str(cargo), "test", "-p", "aster-server", "--test", "trino_policy_live",
         "protected_rest_and_connect_use_trino_table_policy", "--", "--ignored", "--exact", "--nocapture"],
        cwd=ROOT, env=env, capture_output=True, text=True,
    )
    output = redacted(result.stdout + result.stderr, {**values, "V3B1B_JWT": token})
    if result.returncode or "1 passed" not in output:
        raise RuntimeError("route GREEN did not select and pass its test: " + output[-1400:])
    print("V3B1B_ASTER_ROUTE_GREEN", flush=True)


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "setup"
    if mode not in ("setup", "red", "gate"):
        raise SystemExit("usage: trino-data-policy.py [setup|red|gate]")
    suffix = secrets.token_hex(4)
    project = "asterv3b1b" + suffix
    values = {
        "V3B1B_POLARIS_SECRET": secrets.token_hex(24),
        "V3B1B_S3_KEY": secrets.token_hex(10),
        "V3B1B_S3_SECRET": secrets.token_hex(24),
        "V3B1B_BUCKET": "aster-v3b1b-" + suffix,
        "V3B1B_S3_PORT": free_port(),
        "V3B1B_POLARIS_PORT": free_port(),
        "V3B1B_TRINO_PORT": free_port(),
    }
    with tempfile.TemporaryDirectory(prefix="aster-v3b1b-") as temp:
        work = Path(temp)
        cert = generate_trino_config(work, values)
        polaris = "http://127.0.0.1:" + values["V3B1B_POLARIS_PORT"]
        trino = "https://127.0.0.1:" + values["V3B1B_TRINO_PORT"]
        context = ssl.create_default_context(cafile=str(cert))
        try:
            compose(project, values, "up", "-d", "rustfs", "polaris")
            for label, check in (
                ("RustFS", lambda: ready("http://127.0.0.1:" + values["V3B1B_S3_PORT"] + "/health")),
                ("Polaris", lambda: bool(polaris_token(polaris, values))),
            ):
                for _ in range(45):
                    try:
                        if check():
                            break
                    except (urllib.error.URLError, OSError):
                        pass
                    time.sleep(1)
                else:
                    raise RuntimeError(label + " did not become ready")
            access = polaris_token(polaris, values)
            compose(project, values, "run", "--rm", "aws", "--endpoint-url", "http://rustfs:9000",
                    "s3", "mb", "s3://" + values["V3B1B_BUCKET"])
            polaris_api(polaris, access, "/api/management/v1/catalogs", method="POST", body={
                "catalog": {"name": "v3b1b_catalog", "type": "INTERNAL", "readOnly": False,
                            "properties": {"default-base-location": "s3://" + values["V3B1B_BUCKET"]},
                            "storageConfigInfo": {"storageType": "S3", "endpoint": "http://rustfs:9000",
                                                  "endpointInternal": "http://rustfs:9000",
                                                  "pathStyleAccess": True, "stsUnavailable": True,
                                                  "region": "us-west-2",
                                                  "allowedLocations": ["s3://" + values["V3B1B_BUCKET"]]}}})
            polaris_api(polaris, access,
                        "/api/management/v1/catalogs/v3b1b_catalog/catalog-roles/catalog_admin/grants",
                        method="PUT", body={"type": "catalog", "privilege": "CATALOG_MANAGE_CONTENT"})
            polaris_api(polaris, access, "/api/catalog/v1/v3b1b_catalog/namespaces",
                        method="POST", body={"namespace": ["sales"]})
            print("V3B1B_CATALOG_READY", flush=True)
            compose(project, values, "up", "-d", "trino")
            service_token = jwt(values)
            values["V3B1B_JWT"] = service_token
            for _ in range(90):
                try:
                    trino_query(trino, context, service_token, "fixture_admin", "SELECT 1")
                    break
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(2)
                except RuntimeError as error:
                    if str(error) != "Trino query denied: Trino server is still initializing":
                        raise
                    time.sleep(2)
            else:
                raise RuntimeError("Trino authenticated startup did not become ready")
            print("V3B1B_TRINO_AUTH_READY", flush=True)
            trino_query(trino, context, service_token, "fixture_admin",
                        "CREATE TABLE polaris.sales.orders (id integer, label varchar) WITH (format='PARQUET')")
            trino_query(trino, context, service_token, "fixture_admin",
                        "INSERT INTO polaris.sales.orders VALUES (1, 'alpha'), (2, 'beta')")
            trino_query(trino, context, service_token, "fixture_admin",
                        "CREATE TABLE polaris.sales.storage_probe AS SELECT * FROM polaris.sales.orders")
            rows = trino_query(trino, context, service_token, "fixture_admin",
                               "SELECT id, label FROM polaris.sales.orders ORDER BY id")
            if rows != [[1, "alpha"], [2, "beta"]]:
                raise AssertionError("authenticated admin Iceberg seed rows did not match")
            print("V3B1B_ICEBERG_SEEDED", flush=True)
            alice = trino_query(trino, context, service_token, "alice",
                                "SELECT id, label FROM polaris.sales.orders ORDER BY id")
            if alice != rows:
                raise AssertionError("Trino did not return exact Iceberg rows to Alice")
            expect_query_denied(trino, context, service_token, "bob",
                                "SELECT id, label FROM polaris.sales.orders", "Access Denied")
            expect_query_denied(trino, context, service_token, "bob",
                                "SELECT id FROM polaris.sales.storage_probe", "S3")
            expect_query_denied(trino, context, service_token, "alice",
                                "SELECT * FROM tpch.tiny.orders LIMIT 1", "Access Denied")
            expect_http_status(trino + "/v1/statement", 401, context=context,
                               user="alice", body=b"SELECT 1")
            expect_query_denied(trino, context, service_token, "intruder",
                                "SELECT 1", "impersonat")
            expect_http_status(trino + "/v1/statement", 401, context=context,
                               token=jwt(values, audience="wrong-audience"), user="alice", body=b"SELECT 1")
            objects = compose(project, values, "run", "--rm", "aws", "--endpoint-url",
                              "http://rustfs:9000", "s3api", "list-objects-v2",
                              "--bucket", values["V3B1B_BUCKET"], "--query", "Contents[].Key",
                              "--output", "text").split()
            data_key = next((key for key in objects if key.endswith(".parquet")), None)
            if not data_key:
                raise AssertionError("Iceberg data object was not present in RustFS")
            compose(project, values, "run", "--rm", "aws", "--endpoint-url", "http://rustfs:9000",
                    "s3api", "head-object", "--bucket", values["V3B1B_BUCKET"], "--key", data_key)
            expect_http_status("http://127.0.0.1:" + values["V3B1B_S3_PORT"] + "/" +
                               values["V3B1B_BUCKET"] + "/" + data_key, 403)
            print("V3B1B_BACKEND_POLICY_OK", flush=True)
            if mode in ("red", "gate"):
                aster_adapter(work, values, cert, service_token, red=mode == "red")
            if mode == "gate":
                print("trino-data-policy-validation OK", flush=True)
        finally:
            primary = sys.exc_info()[0]
            try:
                compose(project, values, "down", "-v")
            except RuntimeError as cleanup_error:
                if primary is None:
                    raise
                print("cleanup also failed: " + str(cleanup_error), file=sys.stderr)


if __name__ == "__main__":
    main()
