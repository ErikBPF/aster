#[path = "support/ai_context_boundary.rs"]
mod boundary;

#[tokio::test]
async fn denied_metadata_never_reaches_helper() {
    boundary::denied_metadata().await;
}

#[tokio::test]
async fn mixed_unbound_catalogs_are_never_retrieved() {
    boundary::mixed_metadata().await;
}

#[tokio::test]
async fn workspace_index_uses_admitted_snapshot() {
    boundary::workspace_index().await;
}
