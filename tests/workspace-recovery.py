#!/usr/bin/env python3
"""Exercise recovery against runner-owned PostgreSQL and RustFS, without live credentials."""
import importlib.util
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("rustfs_fixture", ROOT / "tests/spark-rustfs-iam.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


def recovery(*, network, work, bucket, keys, passwords, hidden):
    published = fixture.run(["podman", "port", network + "-rustfs", "9000/tcp"]).stdout.strip()
    environment = {
        **os.environ,
        "ASTER_TEST_WORKSPACE_GIT": "1",
        "CARGO_TARGET_DIR": os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")),
        "ASTER_TEST_WORKSPACE_S3_ENDPOINT": "http://" + published,
        "ASTER_TEST_WORKSPACE_S3_BUCKET": bucket,
        "ASTER_TEST_WORKSPACE_S3_KEY": keys["root"],
        "ASTER_TEST_WORKSPACE_S3_SECRET": passwords["root"],
    }
    result = fixture.run(["bash", str(ROOT / "tests/workspace-postgres.sh")],
                         env=environment, secrets_to_redact=hidden, ok=False)
    output = result.stdout + result.stderr
    for secret in hidden:
        output = output.replace(secret, "<redacted>")
    print(output)
    if result.returncode or "workspace-postgres OK" not in output:
        raise AssertionError("workspace PostgreSQL/recovery gate did not complete")


if __name__ == "__main__":
    os.chdir(ROOT)
    fixture.main(after_iam=recovery)
    print("workspace-recovery-validation OK")
