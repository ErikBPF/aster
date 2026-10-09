#[allow(dead_code)]
#[path = "support/odcs_resolution.rs"]
mod support;

#[allow(dead_code)]
#[path = "support/ai_context_boundary.rs"]
mod ai_context_boundary;

#[allow(dead_code)]
#[path = "support/odcs_ai_context.rs"]
mod ai_support;

#[tokio::test]
async fn unselected_inventory_context_is_not_disclosed() {
    ai_support::selected_flow("inventory-unselected").await;
    ai_support::selected_flow("inventory-history").await;
}

#[tokio::test]
async fn physical_inventory_admission() {
    support::physical_inventory_admission().await;
}

#[tokio::test]
async fn mock_table_grants_do_not_disclose_siblings() {
    support::mock_table_grants_do_not_disclose_siblings().await;
}

#[tokio::test]
async fn inventory_unknown_and_unauthorized_are_indistinguishable() {
    support::inventory_unknown_and_unauthorized_are_indistinguishable().await;
}

#[tokio::test]
async fn inventory_uses_one_current_identity_snapshot() {
    support::inventory_uses_one_current_identity_snapshot().await;
}

#[tokio::test]
async fn inventory_nested_semantic_annotations_are_detected() {
    support::inventory_nested_semantic_annotations_are_detected().await;
}
