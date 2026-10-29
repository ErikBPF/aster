"""Bind local-stack-context.feature using real recipes and recording fake clients."""

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
GUARDED = ("chart-install", "chart-uninstall", "stack-up", "stack-down", "build-minikube", "stack-status")
MUTATIONS = GUARDED[:4]
CASES = []


class LocalStackContext(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="aster-context-test-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.bin = self.directory / "bin"
        self.bin.mkdir(mode=0o700)
        self.state = self.directory / "context"
        self.trace = self.directory / "trace.jsonl"
        self.just = shutil.which("just")
        bash = shutil.which("bash")
        if not self.just or not bash:
            raise RuntimeError("local stack context checks require declared just and bash")
        self.just = str(Path(self.just).resolve())
        bash = str(Path(bash).resolve())
        (self.directory / "justfile").write_bytes((ROOT / "justfile").read_bytes())
        for name in ("sh", "bash"):
            (self.bin / name).symlink_to(bash)
        wrapper = self.bin / "just"
        wrapper.write_text(f'#!{bash}\nexec "{self.just}" --no-dotenv --justfile "$ASTER_TEST_JUSTFILE" "$@"\n')
        wrapper.chmod(0o700)
        # No native client appears in PATH, and each stub never forwards execution.
        recorder = f'''#!{sys.executable} -S
import json, os, sys
from pathlib import Path
name, args = Path(sys.argv[0]).name, sys.argv[1:]
state = Path(os.environ["ASTER_TEST_STATE"])
with Path(os.environ["ASTER_TEST_TRACE"]).open("a") as out:
    out.write(json.dumps({{"tool": name, "args": args, "ambient": state.read_text()}}) + "\\n")
mode = os.environ["ASTER_TEST_MODE"]
if name == "kubectl" and args == ["config", "current-context"]:
    if mode == "query-error":
        raise SystemExit(31)
    print(state.read_text())
    if mode == "flip":
        state.write_text("fixture-other")
elif name == "kubectl" and args[:2] == ["config", "use-context"]:
    state.write_text(args[2])
elif name in {{"devspace", "helm"}}:
    if mode == "client-error":
        raise SystemExit(37)
    if mode == "missing-release" and name == "helm":
        raise SystemExit(0 if "--ignore-not-found" in args else 44)
'''
        for name in ("kubectl", "helm", "devspace", "minikube", "docker"):
            executable = self.bin / name
            executable.write_text(recorder)
            executable.chmod(0o700)

    def run_recipe(self, recipe, current="aster", selected=None, mode="stable", profile=None):
        self.state.write_text(current)
        self.trace.write_text("")
        env = {
            "PATH": str(self.bin), "CONTAINER_ENGINE": "docker",
            "ASTER_MINIKUBE_PROFILE": "fixture-profile",
            "ASTER_TEST_STATE": str(self.state), "ASTER_TEST_TRACE": str(self.trace),
            "ASTER_TEST_MODE": mode, "ASTER_TEST_JUSTFILE": str(self.directory / "justfile"),
        }
        if selected is not None:
            env["ASTER_KUBE_CONTEXT"] = selected
        command = [self.just, "--no-dotenv", "--justfile", env["ASTER_TEST_JUSTFILE"], recipe]
        if profile is not None:
            command.append(profile)
        result = subprocess.run(command, cwd=self.directory, env=env, capture_output=True, timeout=10)
        entries = [json.loads(line) for line in self.trace.read_text().splitlines()]
        CASES.append((recipe, mode, result.returncode))
        operations = [entry for entry in entries if entry["args"][:1] != ["config"]]
        return result, operations

    def assert_bound(self, operations, context, profile="local"):
        clients = [entry for entry in operations if entry["tool"] in ("devspace", "helm", "kubectl")]
        self.assertTrue(clients, "positive control must dispatch a cluster client")
        for entry in clients:
            args = entry["args"]
            option = "--context" if entry["tool"] == "kubectl" else "--kube-context"
            self.assertIn(option, args, f'{entry["tool"]} must receive an explicit context')
            self.assertEqual(args[args.index(option) + 1], context)
            if entry["tool"] == "devspace":
                self.assertEqual(args[args.index("--profile") + 1], profile)

    def test_wrong_or_unavailable_context_stops_before_operations(self):
        for recipe in GUARDED:
            for current, mode in (("fixture-other", "stable"), ("", "stable"), ("aster", "query-error")):
                with self.subTest(recipe=recipe, current=current, mode=mode):
                    result, operations = self.run_recipe(recipe, current=current, mode=mode)
                    self.assertNotEqual(result.returncode, 0, "unsafe or unavailable context must fail")
                    self.assertEqual(operations, [], "context refusal must precede all operations")

    def test_empty_selected_context_is_refused(self):
        for recipe in GUARDED:
            with self.subTest(recipe=recipe):
                result, operations = self.run_recipe(recipe, current="", selected="")
                self.assertNotEqual(result.returncode, 0, "empty context must never fall back to ambient state")
                self.assertEqual(operations, [])

    def test_matching_default_context_dispatches_explicit_target(self):
        for recipe in (*MUTATIONS, "stack-status"):
            with self.subTest(recipe=recipe):
                result, operations = self.run_recipe(recipe)
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                self.assert_bound(operations, "aster")

    def test_custom_context_and_profile_remain_supported(self):
        context = "fixture alternate context"
        for recipe in (*MUTATIONS, "stack-status"):
            with self.subTest(recipe=recipe):
                profile = "apps" if recipe.startswith("stack-") and recipe != "stack-status" else None
                result, operations = self.run_recipe(recipe, current=context, selected=context, profile=profile)
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                self.assert_bound(operations, context, profile or "local")

    def test_context_change_after_guard_does_not_change_dispatched_target(self):
        for recipe in (*MUTATIONS, "stack-status"):
            with self.subTest(recipe=recipe):
                result, operations = self.run_recipe(recipe, mode="flip")
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                self.assertTrue(all(entry["ambient"] == "fixture-other" for entry in operations))
                self.assert_bound(operations, "aster")

    def test_custom_context_is_one_inert_shell_argument(self):
        context = "fixture's context $(printf unsafe > canary)"
        for recipe in (*MUTATIONS, "stack-status"):
            with self.subTest(recipe=recipe):
                result, operations = self.run_recipe(recipe, current=context, selected=context)
                self.assertFalse((self.directory / "canary").exists(), "context must not execute shell substitutions")
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                self.assert_bound(operations, context)

    def test_client_failure_is_reported(self):
        for recipe in MUTATIONS:
            with self.subTest(recipe=recipe):
                result, operations = self.run_recipe(recipe, mode="client-error")
                self.assertTrue(operations, "failure control must reach the fake client")
                self.assertNotEqual(result.returncode, 0, "native client failure must propagate")

    def test_absent_release_is_successful_teardown(self):
        result, operations = self.run_recipe("chart-uninstall", mode="missing-release")
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertTrue(operations)

    def test_bootstrap_binds_deployment_and_status(self):
        result, operations = self.run_recipe("stack-bootstrap", current="fixture-other", selected="fixture-selected", profile="apps")
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assert_bound(operations, "fixture-selected", "apps")


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(LocalStackContext))
    if not result.wasSuccessful():
        raise SystemExit(1)
    print(f"LOCAL_STACK_CONTEXT_OK: {len(CASES)} recipe cases")
