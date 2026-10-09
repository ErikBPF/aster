#[path = "support/catalog_credentials.rs"]
mod support;

#[tokio::test]
async fn partial_polaris_environment_refuses_startup() {
    support::partial_polaris_environment_refuses_startup().await;
}

#[tokio::test]
async fn secret_selection_reaches_only_the_configured_catalog() {
    support::secret_selection_reaches_only_the_configured_catalog().await;
}
