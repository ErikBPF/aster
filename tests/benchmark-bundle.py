"""Verify per-table benchmark contracts against the captured live metadata."""
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
manifest = json.loads((root / "manifest.json").read_text())
assert {d["target"] for d in manifest["documents"]} == {"tpch", "tpcds"}, "both real benchmark catalogs required"
observed = json.loads((root / "observed-metadata.json").read_text())
bindings_bytes = (root / "bindings.json").read_bytes()
assert hashlib.sha256(bindings_bytes).hexdigest() == manifest["bindingsSha256"]
bindings = json.loads(bindings_bytes)["bindings"]
expected = {(catalog, row[0]) for catalog, rows in observed.items() for row in rows}
assert len({table for catalog, table in expected if catalog == "tpch"}) == 8
assert len({table for catalog, table in expected if catalog == "tpcds"}) == 25
assert ("tpcds", "dbgen_version") in expected
assert len(manifest["documents"]) == len(bindings) == len(expected) == 33
actual = set()
for entry, binding in zip(manifest["documents"], bindings, strict=True):
    data = (root / entry["path"]).read_bytes()
    assert hashlib.sha256(data).hexdigest() == entry["sha256"]
    document = json.loads(data)  # JSON is a YAML 1.2 subset.
    assert document["apiVersion"] == "v3.2.0" and document["status"] == "draft"
    assert document["id"] == entry["id"] and document["version"] == entry["version"]
    assert len(document["schema"]) == 1
    table = document["schema"][0]
    catalog = entry["target"]
    actual.add((catalog, table["physicalName"]))
    assert binding == {"path": entry["path"], "object": "/schema/0", "catalog": catalog,
                       "namespaceSegments": ["tiny"], "physicalName": table["physicalName"]}
    expected_columns = [(row[1], row[2]) for row in observed[catalog] if row[0] == table["physicalName"]]
    assert [(p["physicalName"], p["physicalType"]) for p in table["properties"]] == expected_columns
    assert all(p["description"] for p in table["properties"])
    assert "synthetic" in document["description"]["limitations"].lower()
assert actual == expected
print("BENCHMARK_BUNDLE_OK: 33 per-table contracts match captured real metadata")
