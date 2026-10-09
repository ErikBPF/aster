"""Run the actual Cargo test artifact in an isolated network namespace."""
import json
import re
import subprocess
import sys

target, test = sys.argv[1:] or ("odcs_intake", "offline_schema_validation_is_distinct_from_support")
build = subprocess.run(["cargo", "test", "--locked", "-p", "aster-server", "--test", target,
                        "--no-run", "--message-format=json"], check=True, text=True, stdout=subprocess.PIPE)
executables = {item["executable"] for line in build.stdout.splitlines()
               if (item := json.loads(line)).get("reason") == "compiler-artifact"
               and item.get("target", {}).get("name") == target
               and item.get("profile", {}).get("test") and item.get("executable")}
assert len(executables) == 1, executables
executable = executables.pop()
print(f"Offline Cargo artifact: {executable}", flush=True)
result = subprocess.run(["unshare", "-Urn", executable, test, "--exact"],
                        check=True, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
print(result.stdout, end="")
assert re.search(r"test result: ok\. 1 passed; 0 failed; 0 ignored;", result.stdout)
print("odcs-offline-validation OK")
