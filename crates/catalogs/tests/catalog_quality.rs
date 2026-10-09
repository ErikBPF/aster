#[path = "support/quality.rs"]
mod quality;

#[tokio::test]
async fn polaris_health_shares_oauth_deadline() {
    quality::polaris_health_shares_oauth_deadline().await;
}

#[tokio::test]
async fn polaris_pages_share_bytes_and_elapsed_budget() {
    quality::polaris_pages_share_bytes_and_elapsed_budget().await;
}

#[tokio::test]
async fn iceberg_current_schema_and_pages() {
    quality::iceberg_current_schema_and_pages().await;
}
#[tokio::test]
async fn provider_http_errors_are_not_empty_success() {
    quality::provider_http_errors_are_not_empty_success().await;
}
#[tokio::test]
async fn openmetadata_pages_and_registered_auth() {
    quality::openmetadata_pages_and_registered_auth().await;
}
