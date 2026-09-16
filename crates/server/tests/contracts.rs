//! In-process bindings for the server behavior contracts.
//!
//! The router is driven directly with `tower::ServiceExt::oneshot`, so no port
//! is bound and no external service is needed: engines/catalogs are stubs, the
//! stores are in-memory, and the notebook checkout is a temporary git
//! repository. Behaviors that need a real dependency (Postgres durability)
//! stay in their own draft feature file.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use aster_core::{
    AppConfig, AuditSink, Catalog, CatalogConfig, CatalogId, CatalogRegistry, Column, EngineConfig,
    EngineId, EngineInfo, EngineRegistry, Grants, Health, InMemoryAudit, InMemoryGrants,
    InMemoryHandshakes, InMemoryLlm, InMemorySessions, InMemoryUserState, Namespace, NotebookStore,
    QueryEngine, QueryRequest, QueryResult, TableRef, TableSchema, UserState, WorkingState,
};
use aster_server::{app, AppState, GitNotebookStore};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use cucumber::{given, then, when, World};
use serde_json::{json, Value};
use tower::ServiceExt;

/// Counts executions so "the request is forwarded to the engine" is observable.
struct StubEngine {
    info: EngineInfo,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl QueryEngine for StubEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    async fn health(&self) -> Health {
        Health::Healthy
    }

    async fn execute(&self, _request: QueryRequest) -> aster_core::Result<QueryResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(QueryResult {
            columns: vec![Column {
                name: "one".into(),
                data_type: "integer".into(),
            }],
            rows: vec![vec![json!(1)]],
            truncated: false,
        })
    }
}

struct StubCatalog {
    id: CatalogId,
}

#[async_trait::async_trait]
impl Catalog for StubCatalog {
    fn id(&self) -> &CatalogId {
        &self.id
    }

    fn kind(&self) -> &str {
        "stub"
    }

    async fn health(&self) -> Health {
        Health::Healthy
    }

    async fn list_namespaces(&self) -> aster_core::Result<Vec<Namespace>> {
        Ok(vec![Namespace {
            name: "default".into(),
        }])
    }

    async fn list_tables(&self, _namespace: &str) -> aster_core::Result<Vec<TableRef>> {
        Ok(vec![])
    }

    async fn table_schema(&self, table: &TableRef) -> aster_core::Result<TableSchema> {
        Ok(TableSchema {
            table: table.clone(),
            columns: vec![],
        })
    }
}

#[derive(Default, World)]
struct Contract {
    /// Executions of the stub engine, observable across steps.
    calls: Arc<AtomicUsize>,
    /// Temporary git checkout for the notebook scenarios.
    dir: Option<PathBuf>,
    grants: Option<Arc<InMemoryGrants>>,
    audit: Option<Arc<InMemoryAudit>>,
    user_state: Option<Arc<InMemoryUserState>>,
    default_engine: Option<EngineId>,
    /// Last response, so `Then the response status is N` reads naturally.
    status: Option<StatusCode>,
    body: String,
    json: Option<Value>,
    /// Last RPC response parsed as JSON (Connect legs only).
    rpc: Option<Value>,
    /// Working state carried between steps.
    pending: WorkingState,
    engine: Option<String>,
    catalog: Option<String>,
}

impl fmt::Debug for Contract {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Contract")
            .field("status", &self.status)
            .field("body", &self.body)
            .finish()
    }
}

impl Contract {
    /// Temporary checkout, created on first use so scenarios that never touch
    /// notebooks still build a router. Cucumber runs scenarios concurrently, so
    /// every world gets its own directory.
    fn checkout(&mut self) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        self.dir
            .get_or_insert_with(|| {
                let dir = std::env::temp_dir().join(format!(
                    "aster-contracts-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::SeqCst)
                ));
                let _ = std::fs::remove_dir_all(&dir);
                dir
            })
            .clone()
    }

    fn build(&mut self) -> Router {
        let mut engines = EngineRegistry::new();
        let id = self.engine.clone().unwrap_or_else(|| "trino-local".into());
        engines.register(Arc::new(StubEngine {
            info: EngineInfo {
                id: EngineId::new(id.clone()),
                kind: "stub".into(),
                endpoint: "http://engine.invalid".into(),
                routing_group: None,
            },
            calls: Arc::clone(&self.calls),
        }));
        let mut catalogs = CatalogRegistry::new();
        catalogs.register(Arc::new(StubCatalog {
            id: CatalogId::new(
                self.catalog
                    .clone()
                    .unwrap_or_else(|| "polaris-local".into()),
            ),
        }));

        let config = AppConfig {
            bind: "127.0.0.1:0".into(),
            engines: vec![EngineConfig {
                id: id.clone(),
                kind: "stub".into(),
                endpoint: "http://engine.invalid".into(),
                routing_group: None,
            }],
            catalogs: vec![CatalogConfig {
                id: self
                    .catalog
                    .clone()
                    .unwrap_or_else(|| "polaris-local".into()),
                kind: "stub".into(),
                endpoint: "http://catalog.invalid".into(),
                catalog: None,
            }],
            default_engine: self.default_engine.as_ref().map(|engine| engine.0.clone()),
            default_catalog: None,
        };

        let notebooks: Arc<dyn NotebookStore> = Arc::new(
            GitNotebookStore::open(self.checkout(), "session/alice".to_string())
                .expect("notebook store"),
        );
        let state = Arc::new(AppState {
            config,
            engines,
            catalogs,
            grants: grants_dyn(self),
            audit: audit_dyn(self),
            notebooks,
            llm: Arc::new(InMemoryLlm::new()),
            contracts: Arc::new(Vec::new()),
            http: reqwest::Client::new(),
            sessions: Arc::new(InMemorySessions::new(3600)),
            handshakes: Arc::new(InMemoryHandshakes::new(300)),
            session_ttl_seconds: 3600,
            user_state: user_state_dyn(self),
            oidc: None,
            dev_login: true,
        });
        app(state)
    }

    /// Sends a request through the router and records status, body and JSON.
    async fn send(&mut self, request: Request<Body>) {
        let router = self.build();
        let response = router.oneshot(request).await.expect("router call");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body");
        self.status = Some(status);
        self.body = String::from_utf8_lossy(&bytes).to_string();
        self.json = serde_json::from_slice(&bytes).ok();
    }

    fn get(&self, path: &str) -> Request<Body> {
        Request::builder()
            .uri(path)
            .body(Body::empty())
            .expect("request")
    }
}

/// Builders keep the shared stores in the world so later steps can inspect
/// what the handlers wrote, while the state only sees the trait objects.
fn grants_dyn(world: &mut Contract) -> Arc<dyn Grants> {
    world
        .grants
        .get_or_insert_with(|| Arc::new(InMemoryGrants::new()))
        .clone()
}

fn audit_dyn(world: &mut Contract) -> Arc<dyn AuditSink> {
    world
        .audit
        .get_or_insert_with(|| Arc::new(InMemoryAudit::default()))
        .clone()
}

fn user_state_dyn(world: &mut Contract) -> Arc<dyn UserState> {
    world
        .user_state
        .get_or_insert_with(|| Arc::new(InMemoryUserState::new()))
        .clone()
}

/// Builds a caller request with the dev identity seam the harness enables.
fn caller(
    method: &str,
    path: &str,
    subject: Option<&str>,
    role: &str,
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(subject) = subject {
        builder = builder
            .header("x-aster-subject", subject)
            .header("x-aster-roles", role);
    }
    let (builder, payload) = match body {
        Some(value) => (
            builder.header(header::CONTENT_TYPE, "application/json"),
            Body::from(value.to_string()),
        ),
        None => (builder, Body::empty()),
    };
    builder.body(payload).expect("request")
}

/// A Connect unary call as the browser or curl would make it.
fn connect(path: &str, subject: Option<&str>, role: &str, body: Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/aster.v1.Aster/{path}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header("connect-protocol-version", "1");
    if let Some(subject) = subject {
        builder = builder
            .header("x-aster-subject", subject)
            .header("x-aster-roles", role);
    }
    builder.body(Body::from(body.to_string())).expect("request")
}

/// The same route as a gRPC client would call it: five framing bytes (no
/// compression, zero-length message) in front of the empty request message.
fn grpc(path: &str, subject: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/aster.v1.Aster/{path}"))
        .header(header::CONTENT_TYPE, "application/grpc")
        .header("te", "trailers")
        .header("x-aster-subject", subject)
        .body(Body::from(vec![0u8, 0, 0, 0, 0]))
        .expect("request")
}

impl Contract {
    fn rpc_json(&self) -> &Value {
        self.rpc.as_ref().expect("rpc response")
    }
}

#[given(expr = "the server is running with engine {string} registered")]
async fn server_running(world: &mut Contract, engine: String) {
    world
        .grants
        .get_or_insert_with(|| Arc::new(InMemoryGrants::new()))
        .grant("alice", &engine);
    world.engine = Some(engine)
}

#[given(expr = "the server runs with engine {string} and catalog {string}")]
async fn server_running_with_catalog(world: &mut Contract, engine: String, catalog: String) {
    server_running(world, engine).await;
    world.catalog = Some(catalog)
}

#[given(expr = "subject {string} holds the editor role with a grant on {string}")]
async fn holds_grant(world: &mut Contract, subject: String, engine: String) {
    world
        .grants
        .get_or_insert_with(|| Arc::new(InMemoryGrants::new()))
        .grant(subject, engine)
}

#[given(expr = "the server is running with the notebook directory on a git repository")]
async fn notebook_repo(_world: &mut Contract) {
    // The checkout is created on first use by `Contract::checkout`.
}

#[given(expr = "the notebook branch is {string}")]
async fn notebook_branch(_world: &mut Contract, _branch: String) {}

#[given(expr = "subject {string} has no grant for engine {string}")]
async fn no_grant(_world: &mut Contract, _subject: String, _engine: String) {}

#[given(expr = "subject {string} is granted engine {string}")]
async fn is_granted(world: &mut Contract, subject: String, engine: String) {
    world
        .grants
        .get_or_insert_with(|| Arc::new(InMemoryGrants::new()))
        .grant(subject, engine)
}

#[given(expr = "subject {string} has only the {string} role")]
async fn viewer_role(_world: &mut Contract, _subject: String, _role: String) {}

#[given(expr = "subject {string} has saved notebook {string} with one cell {string}")]
async fn saved_notebook_with_cell(world: &mut Contract, subject: String, id: String, sql: String) {
    save_notebook_via_api(world, Some(&subject), "editor", &id, &sql).await;
}

#[given(expr = "subject {string} has saved notebook {string}")]
async fn saved_notebook(world: &mut Contract, subject: String, id: String) {
    save_notebook_via_api(world, Some(&subject), "editor", &id, "SELECT 1").await;
}

#[given(expr = "{string} currently has notebook {string} open at cell {string}")]
async fn has_open_notebook(world: &mut Contract, _subject: String, notebook: String, cell: String) {
    let mut pending = WorkingState::default();
    pending.notebook = Some(notebook);
    pending.cell = Some(cell);
    world.pending = pending;
}

#[given(expr = "{string} holds the editor role without a grant on {string}")]
async fn no_grant_for(_world: &mut Contract, _subject: String, _engine: String) {}

#[given(expr = "the server has no default engine")]
async fn no_default_engine(world: &mut Contract) {
    world.default_engine = None;
}

#[when(expr = "the health endpoint is requested")]
async fn health(world: &mut Contract) {
    let request = world.get("/healthz");
    world.send(request).await;
}

#[when(expr = "the engine inventory is requested")]
async fn engines(world: &mut Contract) {
    let request = caller("GET", "/api/engines", Some("alice"), "editor", None);
    world.send(request).await;
}

#[when(expr = "the catalog inventory is requested")]
async fn catalogs(world: &mut Contract) {
    let request = caller("GET", "/api/catalogs", Some("alice"), "editor", None);
    world.send(request).await;
}

#[when(expr = "a query is sent to an unknown engine")]
async fn unknown_engine(world: &mut Contract) {
    let request = caller(
        "POST",
        "/api/query",
        Some("alice"),
        "editor",
        Some(json!({"sql": "SELECT 1", "engine": "nope"})),
    );
    world.send(request).await;
}

#[when(expr = "a query is sent without a subject header")]
async fn anonymous_query(world: &mut Contract) {
    let request = caller(
        "POST",
        "/api/query",
        None,
        "editor",
        Some(json!({"sql": "SELECT 1"})),
    );
    world.send(request).await;
}

#[when(expr = "subject {string} sends a query to {string}")]
async fn query_to_engine(world: &mut Contract, subject: String, engine: String) {
    let request = caller(
        "POST",
        "/api/query",
        Some(&subject),
        "editor",
        Some(json!({"sql": "SELECT 1", "engine": engine})),
    );
    world.send(request).await;
}

#[when(expr = "subject {string} sends a query")]
async fn query(world: &mut Contract, subject: String) {
    let request = caller(
        "POST",
        "/api/query",
        Some(&subject),
        "viewer",
        Some(json!({"sql": "SELECT 1"})),
    );
    world.send(request).await;
}

#[when(expr = "subject {string} with role {string} reads the audit trail")]
async fn read_audit(world: &mut Contract, subject: String, role: String) {
    let request = caller("GET", "/api/audit", Some(&subject), &role, None);
    world.send(request).await;
}

#[when(expr = "subject {string} saves notebook {string} with one cell {string}")]
async fn save_notebook(world: &mut Contract, subject: String, id: String, sql: String) {
    save_notebook_via_api(world, Some(&subject), "editor", &id, &sql).await;
}

#[when(expr = "subject {string} with role {string} saves notebook {string}")]
async fn save_notebook_viewer(world: &mut Contract, subject: String, role: String, id: String) {
    save_notebook_via_api(world, Some(&subject), &role, &id, "SELECT 1").await;
}

#[when(expr = "subject {string} saves notebook {string}")]
async fn save_notebook_plain(world: &mut Contract, subject: String, id: String) {
    save_notebook_via_api(world, Some(&subject), "editor", &id, "SELECT 1").await;
}

#[when(expr = "an unidentified caller saves notebook {string}")]
async fn save_notebook_anonymous(world: &mut Contract, id: String) {
    save_notebook_via_api(world, None, "editor", &id, "SELECT 1").await;
}

#[when(expr = "subject {string} opens notebook {string}")]
async fn open_notebook(world: &mut Contract, subject: String, id: String) {
    let request = caller(
        "GET",
        &format!("/api/notebooks/{id}"),
        Some(&subject),
        "editor",
        None,
    );
    world.send(request).await;
}

#[when(expr = "subject {string} lists notebooks")]
async fn list_notebooks(world: &mut Contract, subject: String) {
    let request = caller("GET", "/api/notebooks", Some(&subject), "editor", None);
    world.send(request).await;
}

#[when(expr = "{string} calls ListEngines over the Connect protocol as JSON")]
async fn connect_list_engines(world: &mut Contract, subject: String) {
    let request = connect("ListEngines", Some(&subject), "editor", json!({}));
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[when(expr = "{string} calls PutState with that working state")]
async fn put_state(world: &mut Contract, subject: String) {
    let body = json!({
        "notebook": world.pending.notebook,
        "cell": world.pending.cell,
        "engine": world.pending.engine,
    });
    let request = connect("PutState", Some(&subject), "editor", body);
    world.send(request).await;
}

#[when(expr = "{string} calls GetState")]
async fn get_state(world: &mut Contract, subject: String) {
    let request = connect("GetState", Some(&subject), "editor", json!({}));
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[when(expr = "{string} saves a notebook {string} with one cell {string}")]
async fn rpc_save_notebook(world: &mut Contract, subject: String, id: String, sql: String) {
    let request = connect(
        "SaveNotebook",
        Some(&subject),
        "editor",
        json!({"id": id, "notebook": {"id": id, "title": "Sales", "cells": [{"id": "c1", "sql": sql}]}}),
    );
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[when(expr = "{string} calls RunQuery on {string} with {string}")]
async fn rpc_run_query(world: &mut Contract, subject: String, engine: String, sql: String) {
    let request = connect(
        "RunQuery",
        Some(&subject),
        "editor",
        json!({"sql": sql, "engine": engine}),
    );
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[when(expr = "an unidentified caller calls ListNotebooks")]
async fn anon_list_notebooks(world: &mut Contract) {
    let request = connect("ListNotebooks", None, "editor", json!({}));
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[when(expr = "{string} calls RunQuery on {string}")]
async fn rpc_run_query_default(world: &mut Contract, subject: String, engine: String) {
    let request = connect(
        "RunQuery",
        Some(&subject),
        "editor",
        json!({"sql": "SELECT 1", "engine": engine}),
    );
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[when(expr = "{string} calls RunQuery without selecting an engine")]
async fn rpc_run_query_no_engine(world: &mut Contract, subject: String) {
    let request = connect(
        "RunQuery",
        Some(&subject),
        "editor",
        json!({"sql": "SELECT 1"}),
    );
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[then(expr = "the response status is {int}")]
async fn status_is(world: &mut Contract, status: u16) {
    assert_eq!(
        world.status.expect("a response").as_u16(),
        status,
        "body: {}",
        world.body
    );
}

#[then(expr = "the body is {string}")]
async fn body_is(world: &mut Contract, expected: String) {
    assert_eq!(world.body.trim(), expected);
}

#[then(expr = "each engine reports id, kind, endpoint and health")]
async fn engines_have_fields(world: &mut Contract) {
    let engines = world
        .json
        .as_ref()
        .and_then(|value| value.as_array())
        .expect("engine array");
    assert!(!engines.is_empty(), "no engines listed");
    for engine in engines {
        for field in ["id", "kind", "endpoint", "health"] {
            assert!(
                engine.get(field).is_some(),
                "engine missing {field}: {engine}"
            );
        }
    }
    assert_eq!(engines[0]["id"], "trino-local");
}

#[then(expr = "each catalog reports id, kind and health")]
async fn catalogs_have_fields(world: &mut Contract) {
    let catalogs = world
        .json
        .as_ref()
        .and_then(|value| value.as_array())
        .expect("catalog array");
    assert!(!catalogs.is_empty(), "no catalogs listed");
    for catalog in catalogs {
        for field in ["id", "kind", "health"] {
            assert!(catalog.get(field).is_some(), "catalog missing {field}");
        }
    }
    assert_eq!(catalogs[0]["id"], "polaris-local");
}

#[then(expr = "the error names the missing grant")]
async fn error_names_grant(world: &mut Contract) {
    let message = world
        .json
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or(&world.body)
        .to_string();
    assert!(
        message.contains("not granted") && message.contains("trino-local"),
        "unhelpful error: {message}"
    );
}

#[then(expr = "the request is forwarded to the engine")]
async fn forwarded(world: &mut Contract) {
    assert_eq!(
        world.calls.load(Ordering::SeqCst),
        1,
        "engine was not called (status {:?}, body {})",
        world.status,
        world.body
    );
}

#[then(expr = "an audit event is recorded for subject {string}")]
async fn audit_recorded(world: &mut Contract, subject: String) {
    let events = world
        .audit
        .as_ref()
        .expect("audit sink")
        .events()
        .await
        .expect("audit events");
    assert!(
        events.iter().any(|event| event.subject == subject),
        "no audit event for {subject}"
    );
}

#[then(expr = "the response carries a non-empty revision")]
async fn non_empty_revision(world: &mut Contract) {
    let revision = world
        .json
        .as_ref()
        .and_then(|value| value.get("revision"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(!revision.is_empty(), "no revision in {}", world.body);
}

#[then(expr = "the current branch is {string}")]
async fn current_branch(world: &mut Contract, branch: String) {
    let dir = world.checkout();
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git branch");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), branch);
}

#[then(expr = "the notebook has one cell whose SQL is {string}")]
async fn notebook_has_cell(world: &mut Contract, sql: String) {
    let cells = world
        .json
        .as_ref()
        .and_then(|value| value.get("cells"))
        .and_then(Value::as_array)
        .expect("cells");
    assert_eq!(cells.len(), 1, "expected one cell in {}", world.body);
    assert_eq!(cells[0]["sql"], sql);
}

#[then(expr = "the list contains {string}")]
async fn list_contains(world: &mut Contract, id: String) {
    let ids = world.json.as_ref().and_then(Value::as_array).expect("ids");
    assert!(
        ids.iter().any(|value| value == &id),
        "{id} not in {}",
        world.body
    );
}

#[then(expr = "the response lists {string} with its kind, endpoint and health")]
async fn rpc_lists_engine(world: &mut Contract, engine: String) {
    let engines = world
        .rpc_json()
        .get("engines")
        .and_then(Value::as_array)
        .expect("engines");
    let found = engines.iter().find(|value| value["id"] == engine);
    let found = found.unwrap_or_else(|| panic!("{engine} not in {}", world.body));
    for field in ["kind", "endpoint", "health"] {
        assert!(found.get(field).is_some(), "missing {field}");
    }
}

#[then(expr = "the same call over gRPC returns the same engines")]
async fn same_engine_over_grpc(world: &mut Contract) {
    let request = grpc("ListEngines", "alice");
    world.send(request).await;
    assert_eq!(
        world.status.expect("gRPC response").as_u16(),
        200,
        "body: {}",
        world.body
    );
    // The response is a gRPC-framed protobuf; a string field is encoded
    // verbatim, so the engine id must appear in the payload.
    let raw = world.body.as_bytes();
    assert!(
        raw.windows(11).any(|window| window == b"trino-local"),
        "gRPC payload did not name the engine"
    );
}

#[then(expr = "the returned state names notebook {string} and cell {string}")]
async fn state_names(world: &mut Contract, notebook: String, cell: String) {
    assert_eq!(
        world.rpc_json()["notebook"],
        notebook,
        "body: {}",
        world.body
    );
    assert_eq!(world.rpc_json()["cell"], cell);
}

#[then(expr = "the response carries the commit revision")]
async fn commit_revision(world: &mut Contract) {
    let revision = world
        .rpc_json()
        .get("revision")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert!(!revision.is_empty(), "no revision in {}", world.body);
}

#[then(expr = "GetNotebook returns the same cell")]
async fn rpc_get_notebook(world: &mut Contract) {
    let request = connect(
        "GetNotebook",
        Some("alice"),
        "editor",
        json!({"id": "sales"}),
    );
    world.send(request).await;
    let cells = world
        .json
        .as_ref()
        .and_then(|value| value.get("cells"))
        .and_then(Value::as_array)
        .expect("cells");
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0]["sql"], "SELECT 1");
}

#[then(expr = "the call reaches the engine")]
async fn call_reaches_engine(world: &mut Contract) {
    assert!(
        world.calls.load(Ordering::SeqCst) >= 1,
        "engine not reached: {}",
        world.body
    );
}

#[then(expr = "the audit trail records the query for {string}")]
async fn rpc_audit(world: &mut Contract, subject: String) {
    let events = world
        .audit
        .as_ref()
        .expect("audit sink")
        .events()
        .await
        .expect("audit events");
    assert!(events.iter().any(|event| event.subject == subject));
}

#[then(expr = "the call fails as unauthenticated")]
async fn unauthenticated(world: &mut Contract) {
    assert_eq!(
        world.status.expect("response").as_u16(),
        401,
        "body: {}",
        world.body
    );
    assert_eq!(world.rpc_json()["code"], "unauthenticated");
}

#[then(expr = "the call fails as unauthenticated naming the missing grant")]
async fn unauthenticated_with_grant(world: &mut Contract) {
    assert_eq!(world.status.expect("response").as_u16(), 401);
    let message = world.rpc_json()["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("not granted") && message.contains("trino-local"),
        "unhelpful message: {message}"
    );
}

#[then(expr = "no notebook content is returned")]
async fn no_notebook_content(world: &mut Contract) {
    assert!(
        world.rpc_json().get("ids").is_none(),
        "body: {}",
        world.body
    );
}

#[then(expr = "the call fails as invalid input")]
async fn invalid_input(world: &mut Contract) {
    assert_eq!(
        world.status.expect("response").as_u16(),
        400,
        "body: {}",
        world.body
    );
    assert_eq!(world.rpc_json()["code"], "invalid_argument");
}

/// Saves through the JSON API, so the git store sees the same path the page uses.
async fn save_notebook_via_api(
    world: &mut Contract,
    subject: Option<&str>,
    role: &str,
    id: &str,
    sql: &str,
) {
    let request = caller(
        "PUT",
        &format!("/api/notebooks/{id}"),
        subject,
        role,
        Some(json!({"id": id, "title": "Sales", "cells": [{"id": "c1", "sql": sql}]})),
    );
    world.send(request).await;
}

/// Only the features that are bound to this harness run: the repository marks
/// drafts `@unautomated`, so dropping that tag is what wires a contract in.
#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("error")
        .try_init();
    Contract::cucumber()
        .filter_run_and_exit("features", |feature, _, _| {
            let tagged = |tag: &str| feature.tags.iter().any(|value| value == tag);
            tagged("contract") && !tagged("unautomated")
        })
        .await;
}
