"""Exact owning runtime inventory gate; no query-authorization claim."""
import pathlib
import sys

rust, bdd = (pathlib.Path(path).read_text() for path in sys.argv[1:])
for name in ["physical_inventory_admission", "unselected_inventory_context_is_not_disclosed",
             "inventory_unknown_and_unauthorized_are_indistinguishable", "inventory_uses_one_current_identity_snapshot",
             "inventory_nested_semantic_annotations_are_detected", "mock_table_grants_do_not_disclose_siblings"]:
    assert f"test {name} ... ok" in rust
assert "test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;" in rust
assert "6 scenarios (6 passed)" in bdd
assert "6 steps (6 passed)" in bdd
assert "skipped" not in bdd.lower()
for name in ["Physical entries require current owner or contract admission", "Unselected AI cannot bypass physical inventory admission",
             "Unauthorized schema probes do not reveal configured existence", "Inventory decisions share one fresh identity snapshot",
             "Nested annotations contribute to the declared semantic facet", "A mock materialization grant does not disclose its sibling table"]:
    assert name in bdd
print("odcs-s8-report OK: 6 tests; 6 scenarios; 6 steps; zero skips")
