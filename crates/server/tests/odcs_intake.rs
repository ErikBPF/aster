#[path = "support/odcs_intake.rs"]
mod support;

#[test]
fn compiled_bundle_requires_no_author_tree() {
    support::compiled_bundle_requires_no_author_tree();
}

#[test]
fn offline_schema_validation_is_distinct_from_support() {
    support::offline_schema_validation_is_distinct_from_support();
}

#[test]
fn schema_valid_v31_is_rejected_without_conversion() {
    support::schema_valid_v31_is_rejected_without_conversion();
}

#[test]
fn compiled_bundle_selection_and_pins_are_explicit() {
    support::compiled_bundle_selection_and_pins_are_explicit();
}

#[test]
fn intake_preservation_does_not_expand_legacy_disclosure() {
    support::intake_preservation_does_not_expand_legacy_disclosure();
}

#[tokio::test]
async fn namespace_segments_survive_adapter_boundaries() {
    support::namespace_segments_survive_adapter_boundaries().await;
}
