"""Generate YAML-compatible JSON ODCS contracts from native tiny metadata."""
import collections
import hashlib
import importlib.util
import json
import pathlib
import sys

spec = importlib.util.spec_from_file_location("trino", pathlib.Path(__file__).with_name("benchmark-trino.py"))
trino = importlib.util.module_from_spec(spec)
spec.loader.exec_module(trino)
endpoint, destination = sys.argv[1:]
root = pathlib.Path(destination)
assert not (root / "manifest.json").exists(), "generate into a fresh bundle directory"
(root / "compiled").mkdir(parents=True, exist_ok=True)
observed, entries, bindings = {}, [], []


def write(name, value):
    data = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()
    (root / name).write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def logical(physical):
    if physical in {"tinyint", "smallint", "integer", "bigint"}:
        return "integer"
    if physical.startswith(("decimal", "double", "real")):
        return "number"
    if physical.startswith("timestamp"):
        return "timestamp"
    if physical.startswith("time(") or physical == "time":
        return "time"
    if physical == "date":
        return "date"
    if physical == "boolean":
        return "boolean"
    assert physical.startswith(("varchar", "char")), f"unmapped Trino type: {physical}"
    return "string"


for catalog, expected in [("tpch", 8), ("tpcds", 25)]:
    rows = trino.query(endpoint, f"SELECT table_name, column_name, data_type, ordinal_position, is_nullable FROM {catalog}.information_schema.columns WHERE table_schema = 'tiny' ORDER BY table_name, ordinal_position")
    observed[catalog] = rows
    tables = collections.defaultdict(list)
    for table, column, physical_type, ordinal, nullable in rows:
        assert nullable in {"YES", "NO"} and ordinal == len(tables[table]) + 1
        tables[table].append({"name": column, "physicalName": column,
                              "businessName": column.replace("_", " "),
                              "logicalType": logical(physical_type), "physicalType": physical_type,
                              "required": nullable == "NO",
                              "description": f"Native {catalog}.tiny.{table}.{column} benchmark column; type and nullability captured from Trino information_schema."})
    assert len(tables) == expected, f"incomplete {catalog} inventory"
    for table, properties in tables.items():
        path = f"compiled/{catalog}.{table}.yaml"
        identifier = f"aster-benchmark-{catalog}-{table}"
        document = {
            "apiVersion": "v3.2.0", "kind": "DataContract", "id": identifier,
            "version": "0.1.0", "name": f"{catalog.upper()} — {table.replace('_', ' ')}",
            "status": "draft", "domain": "synthetic-benchmarks", "tags": [catalog, "tiny", "synthetic"],
            "description": {"purpose": f"Describe the native {catalog}.tiny.{table} benchmark table for notebook exploration.",
                            "usage": "Interactive SQL and catalog/contract demonstrations on the isolated Build-host demo.",
                            "limitations": "Synthetic generated benchmark data, not production records. No business ownership, freshness SLA, monetary unit, key uniqueness or measured quality guarantee is asserted. Catalog context routes queries; it does not enforce SQL target authorization."},
            "team": {"name": "Aster benchmark demo", "description": "Demo stewardship label only. Actual access is controlled independently by current Authentik membership and server-managed grants."},
            "schema": [{"name": table, "physicalName": table, "businessName": table.replace("_", " ").title(),
                        "description": f"Native {catalog.upper()} tiny-scale {table.replace('_', ' ')} relation. Schema is observed from the generator; business constraints are not inferred.",
                        "properties": properties}],
            "customProperties": [{"property": "benchmarkCatalog", "value": catalog},
                                 {"property": "benchmarkSchema", "value": "tiny"},
                                 {"property": "synthetic", "value": True},
                                 {"property": "qualityRulesExecuted", "value": False}],
        }
        digest = write(path, document)
        entries.append({"path": path, "sha256": digest, "id": identifier, "version": "0.1.0", "target": catalog})
        bindings.append({"path": path, "object": "/schema/0", "catalog": catalog, "namespaceSegments": ["tiny"], "physicalName": table})
bindings_digest = write("bindings.json", {"formatVersion": 1, "version": "benchmark-r24", "bindings": bindings})
write("observed-metadata.json", observed)
write("manifest.json", {"manifestVersion": 1, "bundle": "aster-benchmark-catalog", "version": "benchmark-r24",
                        "bindingsSha256": bindings_digest, "documents": entries})
write("grants.json", {"formatVersion": 1, "version": "benchmark-r24", "grants": [
    {"path": entry["path"], "sha256": entry["sha256"], "teams": ["aster-editors"]} for entry in entries]})
print(f"Generated {len(entries)} synthetic per-table contracts from live Trino metadata")
