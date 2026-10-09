#!/usr/bin/env python3
"""Helm overlay for the existing isolated Authentik benchmark demo."""
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile

here = pathlib.Path(__file__).resolve().parent
root = here.parent.parent / "contracts/benchmark-catalog"
registration = json.loads(pathlib.Path(os.environ["ASTER_DEMO_REGISTRATION"]).read_text())
manifest = json.loads((root / "manifest.json").read_text())
files = ["manifest.json", "bindings.json", "grants.json"] + [d["path"] for d in manifest["documents"]]
data = {name.replace("/", "--"): (root / name).read_text() for name in files}
data["schema-owners.json"] = json.dumps({"formatVersion": 1, "version": "benchmark-r24", "schemas": [
    {"catalog": catalog, "namespaceSegments": ["tiny"], "team": "aster-editors"} for catalog in ["tpch", "tpcds"]]})
data["teams.json"] = json.dumps({"aster-editors": {
    "member_claim": registration["groups"]["aster-editors"],
    "maintainer_claim": registration["groups"]["aster-maintainers"],
    "allowed_repositories": ["ErikBPF/aster"]}})
files += ["schema-owners.json", "teams.json"]
data["nginx.conf"] = (here / "nginx.conf").read_text()
env = {
    "ASTER_NOTEBOOK_MODE": "local",
    "ASTER_CONTRACT_BUNDLE": "/etc/aster/bundle",
    "ASTER_CONTRACT_MANIFEST_SHA256": hashlib.sha256((root / "manifest.json").read_bytes()).hexdigest(),
    "ASTER_CONTRACT_BUNDLE_VERSION": manifest["version"],
    "ASTER_TEAM_POLICY_FILE": "/etc/aster/bundle/teams.json",
    "ASTER_AUTHENTIK_API_ORIGIN": "https://localhost:8443",
    "ASTER_AUTHENTIK_CA_FILE": "/etc/aster/tls/ca.crt",
}
mounts = [{"name": "odcs-bundle", "mountPath": "/etc/aster/bundle/" + name,
           "subPath": name.replace("/", "--"), "readOnly": True} for name in files]
mounts.append({"name": "identity-ca", "mountPath": "/etc/aster/tls", "readOnly": True})
proxy = {
    "name": "authentik-proxy",
    "image": "docker.io/library/nginx:1.29.1-alpine@sha256:60e48a050b6408d0c5dd59b98b6e36bf0937a0bbe99304e3e9c0e63b7563443a",
    "command": ["nginx", "-c", "/etc/aster/nginx.conf", "-g", "daemon off;"],
    "ports": [{"name": "oidc", "containerPort": 8081}],
    "resources": {"requests": {"cpu": "25m", "memory": "32Mi"}, "limits": {"memory": "128Mi"}},
    "volumeMounts": [{"name": "odcs-bundle", "mountPath": "/etc/aster/nginx.conf", "subPath": "nginx.conf", "readOnly": True},
                     {"name": "identity-tls", "mountPath": "/etc/aster/tls", "readOnly": True}],
    "readinessProbe": {"httpGet": {"port": 8081, "path": "/-/health/ready/"}, "initialDelaySeconds": 3},
}
patch = [{"op": "add", "path": "/spec/strategy", "value": {"type": "Recreate"}}]
for value in [{"name": key, "value": value} for key, value in env.items()] + [
    {"name": "ASTER_AUTHENTIK_READ_TOKEN", "valueFrom": {"secretKeyRef": {"name": "aster-authentik-demo-reader", "key": "authentik_read_token"}}}
]:
    patch.append({"op": "add", "path": "/spec/template/spec/containers/0/env/-", "value": value})
for value in mounts:
    patch.append({"op": "add", "path": "/spec/template/spec/containers/0/volumeMounts/-", "value": value})
for value in [
    {"name": "odcs-bundle", "configMap": {"name": "aster-odcs-demo-bundle"}},
    {"name": "identity-tls", "secret": {"secretName": "aster-authentik-demo-tls"}},
    {"name": "identity-ca", "secret": {"secretName": "aster-authentik-demo-tls", "items": [{"key": "ca.crt", "path": "ca.crt"}]}},
]:
    patch.append({"op": "add", "path": "/spec/template/spec/volumes/-", "value": value})
patch.append({"op": "add", "path": "/spec/template/spec/containers/-", "value": proxy})
secret_patch = {"apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "aster-server"},
                "spec": {"template": {"spec": {"containers": [{"name": "server", "env": [{
                    "name": "ASTER_OIDC_CLIENT_SECRET", "valueFrom": {"secretKeyRef": {
                        "name": "aster-authentik-demo-client", "key": "oidc_client_secret"}}}]}]}}}}
with tempfile.TemporaryDirectory() as temporary:
    directory = pathlib.Path(temporary)
    (directory / "chart.yaml").write_text(sys.stdin.read())
    (directory / "bundle.json").write_text(json.dumps({"apiVersion": "v1", "kind": "ConfigMap", "metadata": {"name": "aster-odcs-demo-bundle"}, "data": data}))
    (directory / "kustomization.yaml").write_text(json.dumps({
        "apiVersion": "kustomize.config.k8s.io/v1beta1", "kind": "Kustomization", "resources": ["chart.yaml", "bundle.json"],
        "patches": [{"target": {"kind": "Deployment", "name": "aster-server"}, "patch": json.dumps(patch)}, {"patch": json.dumps(secret_patch)}]}))
    subprocess.run(["kubectl", "kustomize", str(directory)], check=True)
