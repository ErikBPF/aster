#[path = "support/odcs_resolution.rs"]
#[allow(dead_code)]
mod support;

#[tokio::test]
async fn catalog_view_matches_authorized_projection() {
    support::ui_checks().await;
}
