#!/usr/bin/env python3
"""Disposable V3b2b storage control for Polaris Generic Delta rows."""

import importlib.util
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time


HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
SPARK = "docker.io/apache/spark@sha256:bf9d035a7c32a8ca46aa58d6348182ffd7d2dff6409206ecfbb3915ff1fef211"
POLARIS = "docker.io/apache/polaris@sha256:3495f67f38cca33892a045f7dd3f46eb52387f0fd52d4145538a772fd8aedad7"
PACKAGES = "io.delta:delta-spark_4.1_2.13:4.1.0,org.apache.hadoop:hadoop-aws:3.4.2"
IVY = "asterv3b2-ivy-20260923"


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


iam = module("spark_rustfs_iam", "spark-rustfs-iam.py")
generic = module("polaris_delta_live", "polaris-delta-live.py")


def after_iam(*, network, work, bucket, keys, passwords, hidden):
    mode = sys.argv[1] if len(sys.argv) > 1 else "gate"
    if mode not in ("red", "gate"):
        raise ValueError("expected red or gate")
    polaris_name = network + "-polaris"
    alice_name = network + "-alice-spark"
    bob_name = network + "-bob-spark"
    polaris_secret = secrets.token_hex(24)
    hidden += (polaris_secret,)
    created = []

    def run(args, *, env=None):
        return iam.run(args, env=env, secrets_to_redact=hidden)

    def wait_for(check, label, attempts=90):
        for _ in range(attempts):
            try:
                if check():
                    return
            except (OSError, ValueError):
                pass
            time.sleep(2)
        raise RuntimeError(label + " did not become ready")

    try:
        if iam.run(["podman", "volume", "exists", IVY], ok=False).returncode:
            run(["podman", "volume", "create", IVY])
            created.append(("volume", IVY))
        environment = {**os.environ,
                       "POLARIS_BOOTSTRAP_CREDENTIALS": "POLARIS,root," + polaris_secret,
                       "AWS_ACCESS_KEY_ID": keys["root"],
                       "AWS_SECRET_ACCESS_KEY": passwords["root"]}
        run(["podman", "run", "-d", "--name", polaris_name, "--network", network,
             "--network-alias", "polaris", "-p", "127.0.0.1::8181",
             "-e", "POLARIS_BOOTSTRAP_CREDENTIALS", "-e", "AWS_ACCESS_KEY_ID",
             "-e", "AWS_SECRET_ACCESS_KEY", "-e", "AWS_REGION=us-west-2",
             "-e", "POLARIS_REALM_CONTEXT_REALMS=POLARIS",
             "-e", "QUARKUS_OTEL_SDK_DISABLED=true", POLARIS], env=environment)
        created.append(("container", polaris_name))
        port = run(["podman", "port", polaris_name, "8181/tcp"]).stdout.strip()
        polaris = "http://" + port
        values = {"V9C_POLARIS_SECRET": polaris_secret}
        wait_for(lambda: bool(generic.token(polaris, values)), "Polaris")
        access = generic.token(polaris, values)
        generic.api(polaris, "/api/management/v1/catalogs", token=access,
                    method="POST", body={"catalog": {
                        "name": "v3b2b_catalog", "type": "INTERNAL", "readOnly": False,
                        "properties": {"default-base-location": f"s3://{bucket}"},
                        "storageConfigInfo": {
                            "storageType": "S3", "endpoint": "http://rustfs:9000",
                            "endpointInternal": "http://rustfs:9000", "pathStyleAccess": True,
                            "stsUnavailable": True, "region": "us-west-2",
                            "allowedLocations": [f"s3://{bucket}"]}}})
        generic.api(polaris,
                    "/api/management/v1/catalogs/v3b2b_catalog/catalog-roles/catalog_admin/grants",
                    token=access, method="PUT",
                    body={"type": "catalog", "privilege": "CATALOG_MANAGE_CONTENT"})
        generic.api(polaris, "/api/catalog/v1/v3b2b_catalog/namespaces",
                    token=access, method="POST", body={"namespace": ["sales"]})

        seed_env = {**os.environ, "AWS_ACCESS_KEY_ID": keys["root"],
                    "AWS_SECRET_ACCESS_KEY": passwords["root"],
                    "AWS_DEFAULT_REGION": "us-west-2", "V3B2B_BUCKET": bucket}
        seeded = run(["podman", "run", "--rm", "--network", network,
                      "--user", "0:0", "-e", "AWS_ACCESS_KEY_ID",
                      "-e", "AWS_SECRET_ACCESS_KEY", "-e", "AWS_DEFAULT_REGION",
                      "-e", "V3B2B_BUCKET", "-v", f"{IVY}:/tmp/.ivy2",
                      "-v", f"{HERE / 'spark-delta-seed.py'}:/fixture/seed.py:ro",
                      "--entrypoint", "/opt/spark/bin/spark-submit", SPARK,
                      "--master", "local[2]", "--packages", PACKAGES,
                      "--conf", "spark.jars.ivy=/tmp/.ivy2", "/fixture/seed.py"],
                     env=seed_env)
        if "V3B2B_DELTA_S3_SEEDED" not in seeded.stdout:
            raise RuntimeError("direct Spark Delta seed lacked success marker")
        print("V3B2B_DELTA_S3_SEEDED", flush=True)

        path = "/api/catalog/polaris/v1/v3b2b_catalog/namespaces/sales/generic-tables"
        location = f"s3://{bucket}/sales/orders_delta"
        generic.api(polaris, path, token=access, method="POST",
                    body={"name": "orders_delta", "format": "delta",
                          "base-location": location})
        loaded = generic.api(polaris, path + "/orders_delta", token=access)
        if loaded["table"]["base-location"] != location or loaded["table"]["format"] != "delta":
            raise RuntimeError("Polaris Generic registration did not return the seed location")
        print("V3B2B_GENERIC_LOCATION_READY", flush=True)

        def start_spark(name, identity):
            spark_env = {**os.environ, "AWS_ACCESS_KEY_ID": keys[identity],
                         "AWS_SECRET_ACCESS_KEY": passwords[identity],
                         "AWS_DEFAULT_REGION": "us-west-2"}
            run(["podman", "run", "-d", "--name", name, "--network", network,
                 "-p", "127.0.0.1::15002", "--user", "0:0",
                 "-e", "AWS_ACCESS_KEY_ID", "-e", "AWS_SECRET_ACCESS_KEY",
                 "-e", "AWS_DEFAULT_REGION", "-v", f"{IVY}:/tmp/.ivy2",
                 "--entrypoint", "/opt/spark/sbin/start-connect-server.sh", SPARK,
                 "--wait", "--master", "local[1]", "--packages", PACKAGES,
                 "--conf", "spark.jars.ivy=/tmp/.ivy2",
                 "--conf", "spark.sql.extensions=io.delta.sql.DeltaSparkSessionExtension",
                 "--conf", "spark.sql.catalog.spark_catalog=org.apache.spark.sql.delta.catalog.DeltaCatalog",
                 "--conf", "spark.hadoop.fs.s3a.endpoint=http://rustfs:9000",
                 "--conf", "spark.hadoop.fs.s3a.path.style.access=true",
                 "--conf", "spark.hadoop.fs.s3a.connection.ssl.enabled=false",
                 "--conf", "spark.hadoop.fs.s3a.endpoint.region=us-west-2"], env=spark_env)
            created.append(("container", name))
            def ready():
                output = run(["podman", "logs", "--tail", "8", name])
                return "Spark Connect server started" in output.stdout + output.stderr
            wait_for(ready, name + " Spark Connect", attempts=60)
            return "http://" + run(["podman", "port", name, "15002/tcp"]).stdout.strip()

        alice_endpoint = start_spark(alice_name, "alice")
        cargo = str(ROOT / ".devenv/profile/bin/cargo")
        metadata = json.loads(subprocess.check_output(
            [cargo, "metadata", "--format-version", "1", "--locked"], cwd=ROOT
        ))
        packages = [package for package in metadata["packages"]
                    if package["name"] == "apache-spark-connect-proto"
                    and package["version"] == "4.2.0"]
        if len(packages) != 1:
            raise RuntimeError("pinned Spark Connect 4.2.0 proto package unavailable")
        proto = Path(packages[0]["manifest_path"]).parent / "proto"
        if not (proto / "spark/connect/base.proto").is_file():
            raise RuntimeError("pinned Spark Connect proto sources unavailable")
        cargo_env = {**os.environ,
                     "ASTER_SPARK_PROTO_ROOT": str(proto),
                     "PROTOC": str(ROOT / ".devenv/profile/bin/protoc"),
                     "PKG_CONFIG_PATH": str(ROOT / ".devenv/profile/lib/pkgconfig"),
                     "RUSTC_WRAPPER": "", "CARGO_TARGET_DIR": str(ROOT.parent.parent / "target"),
                     "V3B2B_SPARK_ENDPOINT": alice_endpoint,
                     "V3B2B_POLARIS_ENDPOINT": polaris,
                     "V3B2B_POLARIS_ROOT_SECRET": polaris_secret,
                     "V3B2B_BUCKET": bucket}
        test = run([cargo, "test", "--manifest-path",
                    str(HERE / "spark-proxy-wire/Cargo.toml"),
                    "tests::pinned_spark_reads_catalog_registered_delta_with_alice_key",
                    "--", "--ignored", "--exact", "--nocapture"], env=cargo_env)
        if "1 passed" not in test.stdout or "V3B2B_GENERIC_ALICE_ROWS_READY" not in test.stdout:
            raise RuntimeError("Aster catalog/Connect Alice test selected zero or lacked marker")
        print("V3B2B_GENERIC_ALICE_ROWS_READY", flush=True)
        bob_endpoint = start_spark(bob_name, "alice" if mode == "red" else "bob")
        cargo_env["V3B2B_BOB_SPARK_ENDPOINT"] = bob_endpoint
        bob = iam.run([cargo, "test", "--manifest-path",
                       str(HERE / "spark-proxy-wire/Cargo.toml"),
                       "tests::pinned_spark_bob_cannot_read_alice_catalog_delta",
                       "--", "--ignored", "--exact", "--nocapture"],
                      env=cargo_env, secrets_to_redact=hidden, ok=False)
        output = bob.stdout + bob.stderr
        if mode == "red":
            if bob.returncode == 0 or "Bob read Alice Delta rows under a shared S3 credential" not in output:
                raise RuntimeError("Bob shared-key control did not fail on the expected row-read assertion")
            print("V3B2B_BOB_SHARED_KEY_ASSERTION_RED", flush=True)
        else:
            if bob.returncode or "1 passed" not in output or "V3B2B_BOB_STORAGE_DENIED" not in output:
                raise RuntimeError("Bob scoped-key denial assertion failed")
            print("V3B2B_BOB_STORAGE_DENIED", flush=True)
    finally:
        primary_failure = sys.exc_info()[0] is not None
        cleanup_failed = False
        for kind, name in reversed(created):
            try:
                if kind == "container":
                    iam.run(["podman", "rm", "-f", name], secrets_to_redact=hidden)
                else:
                    iam.run(["podman", "volume", "rm", name], secrets_to_redact=hidden)
            except RuntimeError:
                cleanup_failed = True
        if cleanup_failed:
            if primary_failure:
                print("V3B2B_CLEANUP_INCOMPLETE", file=sys.stderr)
            else:
                raise RuntimeError("V3b2b task-owned resource cleanup incomplete")


if __name__ == "__main__":
    iam.main(after_iam)
