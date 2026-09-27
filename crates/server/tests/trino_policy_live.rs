//! Disposable V3b1b route acceptance against the runner-owned Trino fixture.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aster_core::*;
use aster_engines::TrinoEngine;
use aster_server::{app, AppState, Metrics};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

struct NoNotebooks;

struct RegisteredCatalog(CatalogId);

#[async_trait::async_trait]
impl Catalog for RegisteredCatalog {
    fn id(&self) -> &CatalogId {
        &self.0
    }
    fn kind(&self) -> &str {
        "polaris"
    }
    async fn health(&self) -> Health {
        Health::Healthy
    }
    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        unreachable!()
    }
    async fn list_tables(&self, _: &str) -> Result<Vec<TableRef>> {
        unreachable!()
    }
    async fn table_schema(&self, _: &TableRef) -> Result<TableSchema> {
        unreachable!()
    }
}

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

async fn query(app: &Router, sid: &str, connect: bool) -> (StatusCode, Value) {
    let path = if connect {
        "/aster.v1.Aster/RunQuery"
    } else {
        "/api/query"
    };
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("cookie", format!("aster_session={sid}"));
    if connect {
        request = request.header("connect-protocol-version", "1");
    }
    let body = if connect {
        json!({"engine":"trino-protected", "catalogContext":"polaris",
               "sql":"SELECT id, label FROM polaris.sales.orders ORDER BY id"})
    } else {
        json!({"engine":"trino-protected", "catalog_context":"polaris",
               "sql":"SELECT id, label FROM polaris.sales.orders ORDER BY id"})
    };
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
#[ignore]
async fn protected_rest_and_connect_use_trino_table_policy() {
    let endpoint = std::env::var("V3B1B_TRINO_ENDPOINT").expect("fixture endpoint");
    let delegation = TrinoDelegationConfig {
        token_file: std::env::var("V3B1B_TRINO_TOKEN_FILE").expect("fixture token file"),
        ca_file: std::env::var("V3B1B_TRINO_CA_FILE").expect("fixture CA file"),
    };
    let mut engines = EngineRegistry::new();
    engines.register(Arc::new(
        TrinoEngine::new_authenticated("trino-protected", &endpoint, None, delegation.clone())
            .expect("fixture Trino delegation"),
    ));
    let mut catalogs = CatalogRegistry::new();
    catalogs.register(Arc::new(RegisteredCatalog(CatalogId::new("polaris"))));
    let grants = Arc::new(InMemoryGrants::new());
    grants.grant("alice", "trino-protected");
    grants.grant("bob", "trino-protected");
    let sessions = Arc::new(InMemorySessions::new(3600));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let alice = sessions
        .create_verified(
            &Identity {
                subject: "alice".into(),
                roles: vec![Role::Editor],
                groups: vec![],
                user_uuid: None,
            },
            None,
            now,
        )
        .await
        .unwrap();
    let bob = sessions
        .create_verified(
            &Identity {
                subject: "bob".into(),
                roles: vec![Role::Editor],
                groups: vec![],
                user_uuid: None,
            },
            None,
            now,
        )
        .await
        .unwrap();
    let audit = Arc::new(InMemoryAudit::new());
    let state = AppState {
        config: AppConfig {
            bind: String::new(),
            engines: vec![EngineConfig {
                id: "trino-protected".into(),
                kind: "trino".into(),
                endpoint,
                routing_group: None,
                delegation: Some(delegation),
            }],
            catalogs: vec![CatalogConfig {
                id: "polaris".into(),
                kind: "polaris".into(),
                endpoint: "fixture".into(),
                catalog: Some("v3b1b_catalog".into()),
                token: None,
                credential: None,
            }],
            catalog_bindings: vec![CatalogBindingConfig {
                catalog: "polaris".into(),
                engine: "trino-protected".into(),
                native_catalog: "polaris".into(),
                policy: BindingPolicy::Protected,
            }],
            default_engine: Some("trino-protected".into()),
            default_catalog: Some("polaris".into()),
        },
        engines,
        catalogs,
        grants,
        audit: audit.clone(),
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
        contracts: Arc::new(vec![]),
        http: reqwest::Client::new(),
        sessions,
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    };
    state.config.validate_catalog_bindings().unwrap();
    let app = app(Arc::new(state));
    for connect in [false, true] {
        let (status, result) = query(&app, &alice.sid, connect).await;
        assert_eq!(status, StatusCode::OK, "Alice route response: {result}");
        let rows = if connect {
            Value::Array(
                result["rowsJson"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| serde_json::from_str(row.as_str().unwrap()).unwrap())
                    .collect(),
            )
        } else {
            result["rows"].clone()
        };
        assert_eq!(rows, json!([[1, "alpha"], [2, "beta"]]));
        let (status, result) = query(&app, &bob.sid, connect).await;
        assert_ne!(
            status,
            StatusCode::OK,
            "Bob bypassed Trino policy: {result}"
        );
        assert!(
            result.to_string().contains("Access Denied"),
            "Bob denial: {result}"
        );
    }
    let events = audit.events().await.unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(
        events.iter().map(|event| event.ok).collect::<Vec<_>>(),
        vec![true, false, true, false]
    );
    assert!(events
        .iter()
        .all(|event| event.catalog.as_deref() == Some("polaris")));
}
