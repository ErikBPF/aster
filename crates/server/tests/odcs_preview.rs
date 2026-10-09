#[allow(dead_code)]
#[path = "support/odcs_resolution.rs"]
mod support;

#[tokio::test]
async fn full_document_preview_preserves_all_content() {
    support::full_document_preview_preserves_all_content().await;
}
