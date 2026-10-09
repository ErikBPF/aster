"""Success-only S5 gate: exact test and BDD counts, no ignored/skipped work."""
import pathlib
import sys

rust, bdd = (pathlib.Path(path).read_text() for path in sys.argv[1:])
assert "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" in rust
for name in (
    "all_ai_entrypoints_use_authorized_context",
    "proposed_exchange_respects_unique_dependency_limit",
    "catalog_observation_revocation_refuses_history",
    "failed_turn_cannot_resurrect_upstream_cached_context",
    "contract_revocation_stops_next_turn_disclosure",
    "multibyte_and_malformed_context_respect_budgets",
    "contract_context_with_denied_observation_makes_zero_catalog_calls",
    "contract_semantics_are_primary_without_live_observation",
    "cell_conversation_uses_selected_meaning",
    "legacy_untracked_history_refuses_replay",
    "total_serialized_history_budget_refuses_without_mutation",
    "query_assistance_uses_semantic_context_for_drafts",
):
    assert f"test {name} ... ok" in rust, name
assert "3 scenarios (3 passed)" in bdd
assert "3 steps (3 passed)" in bdd
assert "skipped" not in bdd.lower()
print("odcs-s5-report OK: 12 tests; 3 scenarios; 3 steps; zero skips")
