#!/usr/bin/env python3
"""Disposable RustFS IAM control for V3b2b; synthetic credentials only."""

import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


RUSTFS = "docker.io/rustfs/rustfs@sha256:fa19210ac4697c79d7ccca1ec9b0eb91aebacc6691991ffb14014bb3c67e6cc3"
RC = "docker.io/rustfs/rc@sha256:9f4e5cd6e43576f2daaa5f6338827ef8296f5eb781175950b0bf62c92e983686"


def run(args, *, env=None, secrets_to_redact=(), ok=True):
    result = subprocess.run(args, env=env, capture_output=True, text=True)
    output = result.stdout + result.stderr
    if ok and result.returncode:
        for secret in secrets_to_redact:
            output = output.replace(secret, "<redacted>")
        raise RuntimeError(f"{args[0]} exited {result.returncode}: {output[-800:]}")
    return result


def policy(bucket, prefix):
    return {"Version": "2012-10-17", "Statement": [
        {"Effect": "Allow", "Action": ["s3:ListBucket", "s3:GetBucketLocation"],
         "Resource": [f"arn:aws:s3:::{bucket}"]},
        {"Effect": "Allow", "Action": ["s3:GetObject"],
         "Resource": [f"arn:aws:s3:::{bucket}/{prefix}/*"]},
    ]}


def main(after_iam=None):
    suffix = secrets.token_hex(4)
    network = "asterv3b2iam" + suffix
    container = network + "-rustfs"
    volume = network + "-data"
    bucket = "aster-v3b2-" + suffix
    keys = {name: secrets.token_hex(12) for name in ("root", "alice", "bob")}
    passwords = {name: secrets.token_hex(24) for name in keys}
    hidden = tuple(keys.values()) + tuple(passwords.values())
    with tempfile.TemporaryDirectory(prefix="aster-v3b2-iam-") as temp:
        work = Path(temp)
        config = work / "config"
        config.mkdir()
        aliases = []
        for name in keys:
            aliases.append(f'''[[aliases]]
name = "{name}"
endpoint = "http://rustfs:9000"
access_key = "{keys[name]}"
secret_key = "{passwords[name]}"
region = "us-west-2"
''')
        (config / "config.toml").write_text(
            'schema_version = 1\n[defaults]\noutput = "human"\ncolor = "never"\nprogress = false\n'
            + "\n".join(aliases)
        )
        (config / "config.toml").chmod(0o600)
        (work / "payload.txt").write_text("alice-marker\n")
        for name, prefix in (("alice", "sales/orders_delta"), ("bob", "bob")):
            (work / f"{name}.json").write_text(json.dumps(policy(bucket, prefix)))
        created = []
        try:
            run(["podman", "network", "create", network], secrets_to_redact=hidden)
            created.append("network")
            run(["podman", "volume", "create", volume], secrets_to_redact=hidden)
            created.append("volume")
            environment = {**os.environ, "RUSTFS_ACCESS_KEY": keys["root"],
                           "RUSTFS_SECRET_KEY": passwords["root"]}
            run(["podman", "run", "-d", "--name", container, "--network", network,
                 "--network-alias", "rustfs", "-p", "127.0.0.1::9000",
                 "-v", f"{volume}:/data", "-e", "RUSTFS_ACCESS_KEY",
                 "-e", "RUSTFS_SECRET_KEY", "-e", "RUSTFS_VOLUMES=/data",
                 "-e", "RUSTFS_ADDRESS=:9000", "-e", "RUSTFS_CONSOLE_ENABLE=false",
                 RUSTFS], env=environment, secrets_to_redact=hidden)
            created.append("container")
            published = run(["podman", "port", container, "9000/tcp"],
                            secrets_to_redact=hidden).stdout.strip()
            endpoint = "http://" + published
            for _ in range(45):
                try:
                    with urllib.request.urlopen(endpoint + "/health", timeout=2) as response:
                        if response.status == 200:
                            break
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(1)
            else:
                raise RuntimeError("disposable RustFS did not become healthy")

            def rc(*args, ok=True):
                return run(["podman", "run", "--rm", "--network", network,
                            "--user", "0:0", "-e", "HOME=/root",
                            "-v", f"{config}:/root/.config/rc:ro",
                            "-v", f"{work}:/fixture:ro", RC, *args],
                           secrets_to_redact=hidden, ok=ok)

            rc("mb", f"root/{bucket}")
            rc("cp", "/fixture/payload.txt",
               f"root/{bucket}/sales/orders_delta/probe.txt")
            for name in ("alice", "bob"):
                rc("admin", "user", "add", "root/", keys[name], passwords[name])
                rc("admin", "policy", "create", "root/", name,
                   f"/fixture/{name}.json")
                rc("admin", "policy", "attach", "root/", name,
                   "--user", keys[name])
            object_path = f"{bucket}/sales/orders_delta/probe.txt"
            alice = rc("cat", f"alice/{object_path}")
            if alice.stdout != "alice-marker\n":
                raise AssertionError("Alice's scoped RustFS key did not read the exact object")
            bob = rc("cat", f"bob/{object_path}", ok=False)
            if bob.returncode == 0 or not any(
                term in (bob.stdout + bob.stderr).lower() for term in ("denied", "forbidden", "403")
            ):
                raise AssertionError("Bob's own scoped key did not receive an S3 denial")
            print("V3B2B_RUSTFS_IAM_READY")
            if after_iam is not None:
                after_iam(network=network, work=work, bucket=bucket,
                          keys=keys, passwords=passwords, hidden=hidden)
        finally:
            primary_failure = sys.exc_info()[0] is not None
            cleanup_failed = False
            for kind, args in (
                ("container", ["podman", "rm", "-f", container]),
                ("volume", ["podman", "volume", "rm", volume]),
                ("network", ["podman", "network", "rm", network]),
            ):
                if kind in created:
                    try:
                        run(args, secrets_to_redact=hidden)
                    except RuntimeError:
                        cleanup_failed = True
            if cleanup_failed:
                if primary_failure:
                    print("V3B2B_CLEANUP_INCOMPLETE", file=sys.stderr)
                else:
                    raise RuntimeError("V3b2b task-owned resource cleanup incomplete")


if __name__ == "__main__":
    main()
