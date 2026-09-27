//! Protected bindings hide metadata and dispatch only verified sessions to an
//! explicitly authenticated engine adapter.

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

struct CountingEngine {
    info: EngineInfo,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl QueryEngine for CountingEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }
    async fn health(&self) -> Health {
        Health::Healthy
    }
    async fn execute(&self, _: QueryRequest) -> Result<QueryResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(QueryResult {
            columns: vec![],
            rows: vec![vec![json!(1)]],
            truncated: false,
        })
    }
    async fn execute_as_verified(&self, _: QueryRequest, subject: &str) -> Result<QueryResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(QueryResult {
            columns: vec![],
            rows: vec![vec![json!(subject)]],
            truncated: false,
        })
    }
}

struct CountingCatalog {
    id: CatalogId,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Catalog for CountingCatalog {
    fn id(&self) -> &CatalogId {
        &self.id
    }
    fn kind(&self) -> &str {
        "mock"
    }
    async fn health(&self) -> Health {
        Health::Healthy
    }
    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![Namespace {
            name: "secret_schema".into(),
        }])
    }
    async fn list_tables(&self, _: &str) -> Result<Vec<TableRef>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![TableRef {
            namespace: "secret_schema".into(),
            name: "secret_table".into(),
        }])
    }
    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(TableSchema {
            table: table.clone(),
            columns: vec![],
        })
    }
}

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

fn fixture_with_sessions(
    policy: &str,
) -> (
    Router,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
    Arc<InMemorySessions>,
) {
    let engine_calls = Arc::new(AtomicUsize::new(0));
    let catalog_calls = Arc::new(AtomicUsize::new(0));
    let mut engines = EngineRegistry::new();
    engines.register(Arc::new(CountingEngine {
        info: EngineInfo {
            id: EngineId::new("shared"),
            kind: "mock".into(),
            endpoint: "local".into(),
            routing_group: None,
        },
        calls: engine_calls.clone(),
    }));
    let mut catalogs = CatalogRegistry::new();
    catalogs.register(Arc::new(CountingCatalog {
        id: CatalogId::new("lake"),
        calls: catalog_calls.clone(),
    }));
    let grants = Arc::new(InMemoryGrants::new());
    grants.grant("alice", "shared");
    grants.grant("bob", "shared");
    let bindings = if policy == "none" {
        vec![]
    } else {
        vec![serde_json::from_value(json!({
            "catalog": "lake", "engine": "shared", "native_catalog": "lake_sql", "policy": policy
        }))
        .unwrap()]
    };
    let sessions = Arc::new(InMemorySessions::new(3600));
    let state = AppState {
        config: AppConfig {
            bind: String::new(),
            engines: vec![EngineConfig {
                id: "shared".into(),
                kind: "mock".into(),
                endpoint: "local".into(),
                routing_group: None,
                delegation: None,
            }],
            catalogs: vec![CatalogConfig {
                id: "lake".into(),
                kind: "mock".into(),
                endpoint: "local".into(),
                catalog: None,
                token: None,
                credential: None,
            }],
            catalog_bindings: bindings,
            default_engine: Some("shared".into()),
            default_catalog: Some("lake".into()),
        },
        engines,
        catalogs,
        grants,
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
        contracts: Arc::new(if policy == "none" {
            vec![DataContract::parse(
                "private",
                "name: secret_orders\ndescription: private rows\n",
            )
            .unwrap()]
        } else {
            vec![]
        }),
        http: reqwest::Client::new(),
        sessions: sessions.clone(),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(Metrics::new()),
    };
    (app(Arc::new(state)), engine_calls, catalog_calls, sessions)
}

fn fixture(policy: &str) -> (Router, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let (app, engine_calls, catalog_calls, _) = fixture_with_sessions(policy);
    (app, engine_calls, catalog_calls)
}

async fn call(app: &Router, subject: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .uri(path)
        .header("x-aster-subject", subject)
        .header("x-aster-roles", "editor");
    if body.is_some() {
        request = request
            .method("POST")
            .header("content-type", "application/json");
        if path.starts_with("/aster.v1.Aster/") {
            request = request.header("connect-protocol-version", "1");
        }
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[test]
fn catalog_binding_requires_explicit_policy() {
    let missing = json!({"catalog":"lake","engine":"shared","native_catalog":"lake_sql"});
    assert!(serde_json::from_value::<CatalogBindingConfig>(missing).is_err());
}

#[tokio::test]
async fn protected_binding_refuses_rest_connect_and_legacy_queries_before_engine_call() {
    let (app, engine_calls, _) = fixture("protected");
    for subject in ["alice", "bob"] {
        let (status, _) = call(&app, subject, "/api/query", Some(json!({
            "sql":"SELECT * FROM lake_sql.secret_schema.secret_table", "engine":"shared", "catalog_context":"lake"
        }))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _) = call(
        &app,
        "bob",
        "/api/query",
        Some(json!({
            "sql":"SELECT * FROM lake_sql.secret_schema.secret_table", "engine":"shared"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(&app, "bob", "/api/query", Some(json!({
        "sql":"SELECT * FROM lake_sql.secret_schema.secret_table", "engine":"shared", "catalog":"lake_sql"
    }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(&app, "bob", "/aster.v1.Aster/RunQuery", Some(json!({
        "sql":"SELECT * FROM lake_sql.secret_schema.secret_table", "engine":"shared", "catalogContext":"lake"
    }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(engine_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn protected_binding_metadata_is_hidden_before_catalog_calls() {
    let (app, _, catalog_calls) = fixture("protected");
    let (status, catalogs) = call(&app, "bob", "/api/catalogs", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalogs, json!([]));
    let (status, rpc_catalogs) =
        call(&app, "bob", "/aster.v1.Aster/ListCatalogs", Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(rpc_catalogs["catalogs"]
        .as_array()
        .is_none_or(Vec::is_empty));
    for path in [
        "/api/catalogs/lake/namespaces",
        "/api/catalogs/lake/namespaces/secret_schema/tables",
    ] {
        let (status, _) = call(&app, "bob", path, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _) = call(
        &app,
        "bob",
        "/aster.v1.Aster/ListTables",
        Some(json!({"catalog":"lake","namespace":"secret_schema"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, choices) = call(&app, "bob", "/api/sql/complete?sql=lake.", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(choices, json!([]));
    let (status, _) = call(&app, "bob", "/api/engines?catalog_context=lake", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(&app, "bob", "/api/ai", Some(json!({
        "prompt":"Describe the catalog", "sql":"SELECT * FROM lake_sql.secret_schema.secret_table"
    }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(catalog_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn explicitly_unprotected_local_mock_retains_query_and_browse_control() {
    let (app, engine_calls, catalog_calls) = fixture("unprotected");
    let (status, _) = call(
        &app,
        "alice",
        "/api/query",
        Some(json!({
            "sql":"SELECT 1", "engine":"shared", "catalog_context":"lake"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(engine_calls.load(Ordering::SeqCst), 1);
    let (status, catalogs) = call(&app, "alice", "/api/catalogs", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalogs[0]["id"], "lake");
    let (status, namespaces) = call(&app, "alice", "/api/catalogs/lake/namespaces", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(namespaces[0]["name"], "secret_schema");
    assert_eq!(catalog_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn missing_binding_cannot_browse_privileged_catalog_metadata() {
    let (app, _, catalog_calls) = fixture("none");
    let (status, catalogs) = call(&app, "alice", "/api/catalogs", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalogs, json!([]));
    let (status, _) = call(&app, "alice", "/api/catalogs/lake/namespaces", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(catalog_calls.load(Ordering::SeqCst), 0);
    let (status, _) = call(&app, "alice", "/api/contracts", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(&app, "alice", "/contracts", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &app,
        "alice",
        "/api/ai",
        Some(json!({
            "prompt":"Describe secret_orders", "sql":"SELECT * FROM secret_orders"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn protected_query_requires_a_live_session_before_authenticated_engine_handoff() {
    let (app, engine_calls, _, sessions) = fixture_with_sessions("protected");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let identity = Identity {
        subject: "alice".into(),
        roles: vec![Role::Editor],
        groups: vec![],
        user_uuid: None,
    };
    let record = sessions
        .create_verified(&identity, None, now)
        .await
        .unwrap();
    let expired = sessions
        .create_verified(&identity, None, now - 7200)
        .await
        .unwrap();
    let dev_session = sessions
        .create("alice", vec![Role::Editor], vec![], None, now)
        .await
        .unwrap();
    let body = json!({"sql":"SELECT 1", "engine":"shared", "catalog_context":"lake"});
    for cookie in [
        None,
        Some("aster_session=forged".to_string()),
        Some(format!("aster_session={}", expired.sid)),
        Some(format!("aster_session={}", dev_session.sid)),
    ] {
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/query")
            .header("content-type", "application/json")
            .header("x-aster-subject", "alice")
            .header("x-aster-roles", "editor");
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    assert_eq!(engine_calls.load(Ordering::SeqCst), 0);

    let request = Request::builder()
        .method("POST")
        .uri("/api/query")
        .header("content-type", "application/json")
        .header("cookie", format!("aster_session={}", record.sid))
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let rows: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(rows["rows"][0][0], "alice");
    assert_eq!(engine_calls.load(Ordering::SeqCst), 1);

    let request = Request::builder()
        .method("POST")
        .uri("/aster.v1.Aster/RunQuery")
        .header("content-type", "application/json")
        .header("connect-protocol-version", "1")
        .header("cookie", format!("aster_session={}", record.sid))
        .body(Body::from(
            json!({
                "sql":"SELECT 1", "engine":"shared", "catalogContext":"lake"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(engine_calls.load(Ordering::SeqCst), 2);

    sessions.revoke(&record.sid).await.unwrap();
    let request = Request::builder()
        .method("POST")
        .uri("/api/query")
        .header("content-type", "application/json")
        .header("cookie", format!("aster_session={}", record.sid))
        .header("x-aster-subject", "alice")
        .header("x-aster-roles", "editor")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(engine_calls.load(Ordering::SeqCst), 2);
}
