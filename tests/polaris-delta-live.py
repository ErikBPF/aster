#!/usr/bin/env python3
"""Disposable Polaris 1.7 Generic Delta registration and Spark row-read gate."""

import base64
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request


COMPOSE = Path(__file__).with_name("polaris-delta.compose.yaml")
PACKAGES = ",".join(
    (
        "org.apache.polaris:polaris-spark-3.5_2.12:1.7.0",
        "org.apache.iceberg:iceberg-aws-bundle:1.10.0",
        "io.delta:delta-spark_2.12:3.3.1",
        "org.apache.hadoop:hadoop-aws:3.3.4",
    )
)


def free_port() -> str:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return str(sock.getsockname()[1])


def redacted(output: str, values: dict[str, str]) -> str:
    for name in ("V9C_POLARIS_SECRET", "V9C_S3_KEY", "V9C_S3_SECRET", "V9C_POLARIS_TOKEN"):
        if name in values:
            output = output.replace(values[name], "<redacted>")
    return output


def compose(project: str, values: dict[str, str], *args: str, expected: int = 0) -> str:
    command = ["docker", "compose", "-f", str(COMPOSE), "-p", project, *args]
    result = subprocess.run(command, env={**os.environ, **values}, capture_output=True, text=True)
    output = result.stdout + result.stderr
    if result.returncode != expected:
        useful = [line for line in redacted(output, values).splitlines()
                  if any(term in line for term in ("Error", "Exception", "Caused by", "V9C_", "failed", "denied"))]
        raise RuntimeError(f"compose {args[:2]} exited {result.returncode}: " + " | ".join(useful[-8:]))
    return output


def spark(project: str, values: dict[str, str], mode: str, *, missing: bool = False) -> str:
    args = (
        "run", "--rm", "spark", "--master", "local[2]", "--packages", PACKAGES,
        "--conf", "spark.jars.ivy=/tmp/.ivy2", "--driver-memory", "2g",
        "/fixture/polaris-delta-spark.py", mode,
    )
    if missing:
        result = subprocess.run(
            ["docker", "compose", "-f", str(COMPOSE), "-p", project, *args],
            env={**os.environ, **values}, capture_output=True, text=True,
        )
        output = result.stdout + result.stderr
        if result.returncode == 0 or "TABLE_OR_VIEW_NOT_FOUND" not in output or \
                "AssertionError: Polaris Generic Table did not resolve Delta rows" not in output:
            raise RuntimeError("pre-registration row-read did not fail for the missing table: " +
                               redacted(output[-800:], values))
        print("V9C_ROW_READ_RED")
        return output
    output = compose(project, values, *args)
    marker = "V9C_DELTA_LOG_SEEDED" if mode == "seed" else \
        f"V9C_DELTA_ROWS_OK version={1 if mode == 'append' else 0}"
    if marker not in output:
        raise RuntimeError(f"Spark {mode} exited successfully without {marker}")
    print(marker)
    return output


def api(base: str, path: str, *, token: str | None = None,
        method: str = "GET", body: dict | None = None) -> dict:
    headers = {"Polaris-Realm": "POLARIS", "Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(
        base + path, data=json.dumps(body).encode() if body is not None else None,
        method=method, headers=headers,
    )
    with urllib.request.urlopen(request, timeout=5) as response:
        if response.status not in (200, 201, 204):
            raise RuntimeError(f"{method} {path} returned HTTP {response.status}")
        payload = response.read()
        return json.loads(payload) if payload else {}


def token(base: str, values: dict[str, str]) -> str:
    credential = base64.b64encode(
        f"root:{values['V9C_POLARIS_SECRET']}".encode()
    ).decode()
    request = urllib.request.Request(
        base + "/api/catalog/v1/oauth/tokens",
        data=b"grant_type=client_credentials&scope=PRINCIPAL_ROLE%3AALL",
        headers={"Authorization": f"Basic {credential}", "Polaris-Realm": "POLARIS",
                 "Content-Type": "application/x-www-form-urlencoded"},
    )
    with urllib.request.urlopen(request, timeout=5) as response:
        return json.load(response)["access_token"]


def ready(url: str) -> bool:
    try:
        with urllib.request.urlopen(url, timeout=2) as response:
            return response.status == 200
    except (urllib.error.URLError, TimeoutError):
        return False


def main() -> None:
    suffix = secrets.token_hex(4)
    project = "asterv9c" + suffix
    values = {
        "V9C_POLARIS_SECRET": secrets.token_hex(24),
        "V9C_S3_KEY": secrets.token_hex(10),
        "V9C_S3_SECRET": secrets.token_hex(24),
        "V9C_BUCKET": "aster-v9c-" + suffix,
        "V9C_S3_PORT": free_port(),
        "V9C_POLARIS_PORT": free_port(),
    }
    base = f"http://127.0.0.1:{values['V9C_POLARIS_PORT']}"
    bucket = values["V9C_BUCKET"]
    try:
        compose(project, values, "up", "-d", "rustfs", "polaris")
        for label, check in (
            ("RustFS", lambda: ready(f"http://127.0.0.1:{values['V9C_S3_PORT']}/health")),
            ("Polaris", lambda: bool(token(base, values))),
        ):
            for _ in range(45):
                try:
                    if check():
                        break
                except (urllib.error.URLError, OSError):
                    pass
                time.sleep(1)
            else:
                raise RuntimeError(f"{label} did not become ready")
        access = token(base, values)
        values["V9C_POLARIS_TOKEN"] = access
        compose(project, values, "run", "--rm", "aws", "--endpoint-url", "http://rustfs:9000",
                "s3", "mb", f"s3://{bucket}")
        api(base, "/api/management/v1/catalogs", token=access, method="POST", body={
            "catalog": {"name": "v9c_catalog", "type": "INTERNAL", "readOnly": False,
                        "properties": {"default-base-location": f"s3://{bucket}"},
                        "storageConfigInfo": {"storageType": "S3", "endpoint": "http://rustfs:9000",
                                              "endpointInternal": "http://rustfs:9000",
                                              "pathStyleAccess": True, "stsUnavailable": True,
                                              "region": "us-west-2",
                                              "allowedLocations": [f"s3://{bucket}"]}}})
        api(base, "/api/management/v1/catalogs/v9c_catalog/catalog-roles/catalog_admin/grants",
            token=access, method="PUT", body={"type": "catalog", "privilege": "CATALOG_MANAGE_CONTENT"})
        api(base, "/api/catalog/v1/v9c_catalog/namespaces", token=access, method="POST",
            body={"namespace": ["sales"]})
        print("V9C_S3_CATALOG_READY")
        spark(project, values, "seed")
        log = "sales/orders_delta/_delta_log/00000000000000000000.json"
        compose(project, values, "run", "--rm", "aws", "--endpoint-url", "http://rustfs:9000",
                "s3api", "head-object", "--bucket", bucket, "--key", log)
        spark(project, values, "read", missing=True)
        path = "/api/catalog/polaris/v1/v9c_catalog/namespaces/sales/generic-tables"
        location = f"s3://{bucket}/sales/orders_delta"
        api(base, path, token=access, method="POST",
            body={"name": "orders_delta", "format": "delta", "base-location": location})
        listed = api(base, path, token=access)
        loaded = api(base, path + "/orders_delta", token=access)
        if {"namespace": ["sales"], "name": "orders_delta"} not in listed["identifiers"] or \
                loaded["table"]["format"] != "delta" or \
                loaded["table"]["base-location"] != location:
            raise RuntimeError("Generic Table list/load did not match the registered Delta fixture")
        print("V9C_GENERIC_REGISTERED")
        spark(project, values, "read")
        spark(project, values, "append")
        compose(project, values, "run", "--rm", "aws", "--endpoint-url", "http://rustfs:9000",
                "s3api", "head-object", "--bucket", bucket, "--key",
                "sales/orders_delta/_delta_log/00000000000000000001.json")
        print("polaris-delta-validation OK")
    finally:
        primary_error = sys.exc_info()[0]
        try:
            compose(project, values, "down", "-v")
        except RuntimeError as cleanup_error:
            if primary_error is None:
                raise
            print(f"cleanup also failed: {cleanup_error}", file=sys.stderr)


if __name__ == "__main__":
    main()
