#[allow(dead_code)]
#[path = "support/ai_context_boundary.rs"]
mod ai_context_boundary;
#[path = "support/odcs_ai_context.rs"]
mod support;
use support::selected_flow;

#[tokio::test]
async fn proposed_exchange_respects_unique_dependency_limit() {
    selected_flow("selection-limit").await;
}

#[tokio::test]
async fn failed_turn_cannot_resurrect_upstream_cached_context() {
    selected_flow("failed-cache").await;
}

#[tokio::test]
async fn catalog_observation_revocation_refuses_history() {
    selected_flow("observation-revoke").await;
    selected_flow("legacy-observation-revoke").await;
}

#[tokio::test]
async fn all_ai_entrypoints_use_authorized_context() {
    for mode in ["allowed", "personal", "shared", "nested"] {
        selected_flow(mode).await;
    }
}
#[tokio::test]
async fn contract_revocation_stops_next_turn_disclosure() {
    selected_flow("revoke").await;
}
#[tokio::test]
async fn multibyte_and_malformed_context_respect_budgets() {
    for mode in [
        "field-budget",
        "reference-budget",
        "properties-budget",
        "reply-budget",
    ] {
        selected_flow(mode).await;
    }
}
#[tokio::test]
async fn contract_context_with_denied_observation_makes_zero_catalog_calls() {
    selected_flow("denied").await;
}
#[tokio::test]
async fn contract_semantics_are_primary_without_live_observation() {
    for mode in ["timeout", "malformed-observation", "observed"] {
        selected_flow(mode).await;
    }
}
#[tokio::test]
async fn cell_conversation_uses_selected_meaning() {
    selected_flow("cell").await;
}
#[tokio::test]
async fn legacy_untracked_history_refuses_replay() {
    selected_flow("legacy-history").await;
    selected_flow("legacy-no-bundle").await;
}
#[tokio::test]
async fn total_serialized_history_budget_refuses_without_mutation() {
    selected_flow("history-budget").await;
}
#[tokio::test]
async fn query_assistance_uses_semantic_context_for_drafts() {
    selected_flow("drafts").await;
}
