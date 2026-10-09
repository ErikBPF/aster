#[path = "support/odcs_resolution.rs"]
mod support;

#[tokio::test]
async fn whole_contract_team_access_is_default_deny() {
    support::whole_contract_team_access_is_default_deny().await;
}

#[tokio::test]
async fn query_preparation_exposes_selected_meaning_and_binding_gaps() {
    support::checks(true).await;
}

#[tokio::test]
async fn qualified_collision_and_multiobject_resolution() {
    support::bound_checks().await;
}

#[tokio::test]
async fn drift_and_provenance_do_not_replace_observation() {
    support::observation_checks().await;
}

#[tokio::test]
async fn contract_reads_do_not_require_live_schema() {
    support::unavailable_checks().await;
}

#[tokio::test]
async fn contract_reads_with_denied_observation_make_zero_catalog_calls() {
    support::bound_checks().await;
}

#[tokio::test]
async fn contract_revocation_stops_next_request_disclosure() {
    support::whole_contract_team_access_is_default_deny().await;
}
