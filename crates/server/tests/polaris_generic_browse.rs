use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use aster_core::*;
use aster_server::{app, AppState, Metrics};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

struct NoNotebooks;

#[async_trait::async_trait]
impl NotebookStore for NoNotebooks {
    async fn get(&self, _: &str) -> Result<Notebook> {
        unreachable!()
    }
    async fn list(&self, _: &str) -> Result<Vec<String>> {
        unreachable!()
    }
    async fn save(&self, _: &Notebook, _: &str) -> Result<String> {
        unreachable!()
    }
}

struct GenericCatalog {
    id: CatalogId,
    schema_calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Catalog for GenericCatalog {
    fn id(&self) -> &CatalogId {
        &self.id
    }
    fn kind(&self) -> &str {
        "polaris"
    }
    async fn health(&self) -> Health {
        Health::Healthy
    }
    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        Ok(vec![Namespace {
            segments: Vec::new(),
            name: "sales".into(),
        }])
    }
    async fn list_tables(&self, _: &str) -> Result<Vec<TableRef>> {
        Ok(vec![TableRef {
            namespace_segments: Vec::new(),
            namespace: "sales".into(),
            name: "delta_orders".into(),
        }])
    }
    async fn list_table_descriptors(&self, _: &str) -> Result<Vec<TableDescriptor>> {
        Ok(vec![
            TableDescriptor {
                table: TableRef {
                    namespace_segments: Vec::new(),
                    namespace: "sales".into(),
                    name: "delta_orders".into(),
                },
                format: Some("delta".into()),
                base_location: Some("s3://lake/sales/delta_orders".into()),
                schema_available: false,
            },
            TableDescriptor {
                table: TableRef {
                    namespace_segments: Vec::new(),
                    namespace: "sales".into(),
                    name: "delta %20orders".into(),
                },
                format: Some("delta".into()),
                base_location: None,
                schema_available: false,
            },
        ])
    }
    async fn table_schema(&self, _: &TableRef) -> Result<TableSchema> {
        self.schema_calls.fetch_add(1, Ordering::SeqCst);
        Err(CoreError::Invalid("generic schema unavailable".into()))
    }
}

fn test_app() -> (Router, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut catalogs = CatalogRegistry::new();
    catalogs.register(Arc::new(GenericCatalog {
        id: CatalogId::new("polaris"),
        schema_calls: Arc::clone(&calls),
    }));
    let app = app(Arc::new(AppState {
        config: AppConfig {
            bind: String::new(),
            engines: vec![EngineConfig {
                id: "fixture".into(),
                kind: "mock".into(),
                endpoint: "local".into(),
                routing_group: None,
                delegation: None,
            }],
            catalogs: vec![CatalogConfig {
                id: "polaris".into(),
                kind: "polaris".into(),
                endpoint: "local".into(),
                catalog: None,
                token: None,
                credential: None,
            }],
            catalog_bindings: vec![CatalogBindingConfig {
                catalog: "polaris".into(),
                engine: "fixture".into(),
                native_catalog: "polaris".into(),
                policy: BindingPolicy::Unprotected,
            }],
            default_engine: None,
            default_catalog: None,
        },
        engines: EngineRegistry::new(),
        catalogs,
        grants: Arc::new(InMemoryGrants::new()),
        audit: Arc::new(InMemoryAudit::new()),
        notebooks: Arc::new(NoNotebooks),
        notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: None,
        team_git_targets: None,
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: None,
        current_identity: None,
        shared_model_use_enabled: false,
        conversations: Arc::new(InMemoryConversations::default()),
        exchanges: Arc::new(InMemoryExchanges::default()),
        compiled_contracts: None,
        contracts: Arc::new(vec![]),
        http: reqwest::Client::new(),
        sessions: Arc::new(InMemorySessions::new(3600)),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    }));
    (app, calls)
}

async fn call(
    app: &Router,
    path: &str,
    role: &str,
    body: Option<Value>,
) -> (StatusCode, Value, String) {
    let mut builder = Request::builder()
        .uri(path)
        .header("x-aster-subject", "alice")
        .header("x-aster-roles", role);
    if path.starts_with("/aster.v1.Aster/") {
        builder = builder
            .method("POST")
            .header("connect-protocol-version", "1")
            .header("content-type", "application/json");
    }
    let payload = body.map(|value| value.to_string()).unwrap_or_default();
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(payload)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value, text)
}

#[tokio::test]
async fn polaris_generic_rest_rpc_and_browser_show_metadata_without_schema_call() {
    let (app, schema_calls) = test_app();
    let (status, rest, _) = call(
        &app,
        "/api/catalogs/polaris/namespaces/sales/tables",
        "viewer",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rest[0]["name"], "delta_orders");
    assert_eq!(rest[0]["format"], "delta");
    assert_eq!(rest[0]["base_location"], "s3://lake/sales/delta_orders");
    assert_eq!(rest[0]["schema_available"], false);

    let (status, rpc, _) = call(
        &app,
        "/aster.v1.Aster/ListTables",
        "viewer",
        Some(json!({"catalog":"polaris","namespace":"sales"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rpc["tables"][0]["format"], "delta");
    assert_eq!(rpc["tables"][0]["schemaAvailable"], false);

    let (status, _, html) = call(&app, "/catalog/polaris/sales/delta_orders", "viewer", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Schema unavailable"), "{html}");
    assert!(html.contains("s3://lake/sales/delta_orders"), "{html}");
    assert_eq!(schema_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn polaris_generic_rpc_browse_requires_a_viewer_role() {
    let (app, _) = test_app();
    let (status, _, _) = call(
        &app,
        "/aster.v1.Aster/ListTables",
        "",
        Some(json!({"catalog":"polaris","namespace":"sales"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn polaris_generic_browser_links_encode_table_name_as_one_path_segment() {
    let (app, schema_calls) = test_app();
    let (status, _, html) = call(&app, "/catalog/polaris/sales", "viewer", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains("/catalog/polaris/sales/delta%20%2520orders"),
        "catalog link did not encode the table name"
    );

    let (status, _, page) = call(
        &app,
        "/catalog/polaris/sales/delta%20%2520orders",
        "viewer",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Schema unavailable"));
    assert_eq!(schema_calls.load(Ordering::SeqCst), 0);
}
