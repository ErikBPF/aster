"""Exact success-only S6 counts; live activation remains a separate Q6 handoff."""
import pathlib
import sys

adapters, credentials, bdd = (pathlib.Path(path).read_text() for path in sys.argv[1:])
assert "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" in adapters
for name in (
    "iceberg_current_schema_and_pages",
    "provider_http_errors_are_not_empty_success",
    "openmetadata_pages_and_registered_auth",
    "polaris_health_shares_oauth_deadline",
    "polaris_pages_share_bytes_and_elapsed_budget",
):
    assert f"test {name} ... ok" in adapters, name
assert "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" in credentials
assert "test secret_selection_reaches_only_the_configured_catalog ... ok" in credentials
assert "test partial_polaris_environment_refuses_startup ... ok" in credentials
assert "1 scenario (1 passed)" in bdd
assert "1 step (1 passed)" in bdd
assert "skipped" not in bdd.lower()
assert "Current Iceberg schema and all pages are observed" in bdd
print("odcs-s6-report OK: 7 tests; 1 scenario; 1 step; zero skips")
