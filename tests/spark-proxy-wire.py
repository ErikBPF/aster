#!/usr/bin/env python3
"""Disposable Spark Connect wire fixture; never enables protected Spark in Aster."""

import json
import os
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CARGO = ROOT / ".devenv/profile/bin/cargo"
PROTOC = ROOT / ".devenv/profile/bin/protoc"
FIXTURE = ROOT / "tests/spark-proxy-wire/Cargo.toml"


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "gate"
    if mode not in ("red", "gate"):
        raise SystemExit("usage: spark-proxy-wire.py [red|gate]")
    metadata = json.loads(subprocess.check_output(
        [str(CARGO), "metadata", "--format-version", "1", "--locked"], cwd=ROOT
    ))
    matches = [package for package in metadata["packages"]
               if package["name"] == "apache-spark-connect-proto"
               and package["version"] == "4.2.0"]
    if len(matches) != 1:
        raise RuntimeError("pinned Spark Connect proto package unavailable")
    proto = Path(matches[0]["manifest_path"]).parent / "proto"
    if not (proto / "spark/connect/base.proto").is_file():
        raise RuntimeError("pinned Spark Connect proto sources unavailable")
    env = {**os.environ,
           "ASTER_SPARK_PROTO_ROOT": str(proto),
           "PROTOC": str(PROTOC),
           "PKG_CONFIG_PATH": str(ROOT / ".devenv/profile/lib/pkgconfig"),
           "RUSTC_WRAPPER": "",
           "CARGO_TARGET_DIR": str(ROOT.parent.parent / "target")}
    result = subprocess.run(
        [str(CARGO), "test", "--manifest-path", str(FIXTURE),
         "real_client_reaches_authenticated_fake_proxy", "--", "--nocapture"],
        cwd=ROOT, env=env, text=True, capture_output=True,
    )
    output = result.stdout + result.stderr
    if mode == "red":
        if (result.returncode == 0 or "V3B2A_WIRE_CONTROL_READY" not in output
                or "protected Spark must hand verified Alice" not in output):
            raise RuntimeError("Spark proxy RED was setup, not the expected assertion:\n" + output[-1600:])
        print("V3B2A_PROXY_RED", flush=True)
        return
    if result.returncode or not re.search(r"\b1 passed\b", output):
        raise RuntimeError("Spark proxy wire fixture failed or selected zero tests:\n" + output[-1600:])
    print("spark-proxy-handoff-validation OK", flush=True)


if __name__ == "__main__":
    main()
