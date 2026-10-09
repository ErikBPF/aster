"""Wrong-context recipes refuse before Kubernetes/Helm; positive control uses stubs."""
import os
import pathlib
import shutil
import subprocess
import tempfile

runner = shutil.which("just")
root = pathlib.Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory() as directory:
    temporary = pathlib.Path(directory)
    trace = temporary / "mutations"
    for command in ["kubectl", "helm", "kubeconform", "cargo", "python3"]:
        executable = temporary / command
        body = 'printf "%s\\n" "$0" >> "$ASTER_MUTATION_TRACE"\n' if command in ["kubectl", "helm"] else ""
        executable.write_text("#!/bin/sh\n" + body + "exit 0\n")
        executable.chmod(0o755)
    env = {**os.environ, "PATH": str(temporary) + os.pathsep + os.environ["PATH"],
           "ASTER_MUTATION_TRACE": str(trace), "ASTER_DEMO_REGISTRATION": "/unused-in-stub-test"}
    for recipe in [["benchmark-backend-up"], ["benchmark-deploy", "sha256:" + "0" * 64]]:
        for context, allowed in [("wrong-cluster", False), ("aster-demo", True)]:
            trace.unlink(missing_ok=True)
            result = subprocess.run([runner, *recipe], cwd=root, env={**env, "ASTER_KUBE_CONTEXT": context},
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            assert (result.returncode == 0) == allowed, f"{recipe[0]} must refuse wrong context"
            assert trace.exists() == allowed, f"{recipe[0]} mutation boundary or positive control failed"
print("BENCHMARK_CONTEXT_OK")
