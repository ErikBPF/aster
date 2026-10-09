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
use std::sync::{Arc, Mutex};

use aster_core::{
    AppConfig, AuditSink, BindingPolicy, Catalog, CatalogBindingConfig, CatalogConfig, CatalogId,
    CatalogRegistry, Column, ColumnSchema, CoreError, EngineConfig, EngineId, EngineInfo,
    EngineRegistry, Grants, Health, InMemoryAudit, InMemoryGrants, InMemoryHandshakes, InMemoryLlm,
    InMemorySessions, InMemoryUserState, Namespace, Notebook, NotebookOwners, NotebookStore,
    QueryEngine, QueryRequest, QueryResult, TableRef, TableSchema, UserState, WorkingState,
};
use aster_server::{app_recorded, AppState, GitNotebookStore};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use cucumber::{given, then, when, World};
use serde_json::{json, Value};
use tower::ServiceExt;

#[path = "support/catalog_credentials.rs"]
mod catalog_credentials;

#[then("S6 catalog observations are current complete bounded and target authenticated")]
async fn s6_quality(_: &mut Contract) {
    catalog_credentials::quality::polaris_health_shares_oauth_deadline().await;
    catalog_credentials::quality::polaris_pages_share_bytes_and_elapsed_budget().await;
    catalog_credentials::partial_polaris_environment_refuses_startup().await;
    catalog_credentials::quality::iceberg_current_schema_and_pages().await;
    catalog_credentials::quality::provider_http_errors_are_not_empty_success().await;
    catalog_credentials::quality::openmetadata_pages_and_registered_auth().await;
    catalog_credentials::secret_selection_reaches_only_the_configured_catalog().await;
}

#[path = "support/odcs_resolution.rs"]
mod odcs_resolution;

#[then("S8 physical inventory scopes entries and refuses unknown evidence")]
async fn s8_inventory(_: &mut Contract) {
    odcs_resolution::physical_inventory_admission().await;
}

#[then("mock table contracts keep sibling artifacts and inventory independently admitted")]
async fn mock_table_admission(_: &mut Contract) {
    odcs_resolution::mock_table_grants_do_not_disclose_siblings().await;
}

#[then("S8 unknown and unauthorized schemas have indistinguishable responses")]
async fn s8_inventory_existence(_: &mut Contract) {
    odcs_resolution::inventory_unknown_and_unauthorized_are_indistinguishable().await;
}

#[then("S8 ownership and contract admission use one current identity")]
async fn s8_inventory_identity(_: &mut Contract) {
    odcs_resolution::inventory_uses_one_current_identity_snapshot().await;
}

#[then("S8 nested properties items and maps retain declared semantic annotations")]
async fn s8_inventory_semantics(_: &mut Contract) {
    odcs_resolution::inventory_nested_semantic_annotations_are_detected().await;
}

#[then("S8 unselected assistance discloses no physical metadata")]
async fn s8_ai(_: &mut Contract) {
    odcs_ai_context::selected_flow("inventory-unselected").await;
    odcs_ai_context::selected_flow("inventory-history").await;
}

#[then("S4 original preview preserves source bytes and schema-valid content under whole-document admission")]
async fn s4_original_preview(_: &mut Contract) {
    odcs_resolution::full_document_preview_preserves_all_content().await;
}

#[allow(dead_code)]
#[path = "support/odcs_ai_context.rs"]
mod odcs_ai_context;

#[then("S5 selected reference is bounded and revoked history cannot replay")]
async fn s5_reference(_: &mut Contract) {
    odcs_ai_context::selected_flow("selection-limit").await;
    odcs_ai_context::selected_flow("failed-cache").await;
    odcs_ai_context::selected_flow("observation-revoke").await;
    odcs_ai_context::selected_flow("legacy-observation-revoke").await;
    odcs_ai_context::selected_flow("revoke").await;
    odcs_ai_context::selected_flow("legacy-history").await;
    odcs_ai_context::selected_flow("history-budget").await;
    odcs_ai_context::selected_flow("field-budget").await;
}
#[then("S5 authorized meaning survives denied and unavailable observations")]
async fn s5_observation(_: &mut Contract) {
    odcs_ai_context::selected_flow("denied").await;
    odcs_ai_context::selected_flow("timeout").await;
}
#[then("S5 actual helper requests and returned drafts follow exact selected semantics")]
async fn s5_drafts(_: &mut Contract) {
    odcs_ai_context::selected_flow("drafts").await;
    odcs_ai_context::selected_flow("cell").await;
}

#[then("S7 browser preserves admitted meaning and clears revoked context")]
#[then("S7 browser reviews edited drafts with selected meaning and binding gaps")]
async fn s7_browser(_: &mut Contract) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = tokio::process::Command::new("python3")
        .arg("tests/odcs-catalog-browser.py")
        .current_dir(root)
        .status()
        .await
        .unwrap();
    assert!(status.success(), "S7 real browser assertions failed");
}

#[then("S3 exact object selection preserves its explicit binding")]
async fn s3_binding(_: &mut Contract) {
    odcs_resolution::bound_checks().await;
}
#[then("S3 observed columns do not replace declared meaning")]
async fn s3_observation(_: &mut Contract) {
    odcs_resolution::observation_checks().await;
    odcs_resolution::unavailable_checks().await;
}
#[then("S3 whole document admission checks fresh team membership")]
async fn s3_admission(_: &mut Contract) {
    odcs_resolution::whole_contract_team_access_is_default_deny().await;
}
#[then("S3 revoked and missing authority cannot read contracts")]
async fn s3_denial(_: &mut Contract) {
    odcs_resolution::whole_contract_team_access_is_default_deny().await;
}
#[then("S3 preparation preserves meaning with missing bindings")]
async fn s3_preparation(_: &mut Contract) {
    odcs_resolution::checks(true).await;
}

/// Counts executions so "the request is forwarded to the engine" is observable,
/// and can be told to refuse the statement so the query-error path is drivable.
struct StubEngine {
    info: EngineInfo,
    calls: Arc<AtomicUsize>,
    refusal: Arc<Mutex<Option<String>>>,
    /// The session catalog the last execute saw, so the defaulting is observable.
    catalog_seen: Arc<Mutex<Option<String>>>,
    /// Stands in for an engine with no session catalog (Spark) when true.
    without_catalog: bool,
}

#[async_trait::async_trait]
impl QueryEngine for StubEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn uses_catalog(&self) -> bool {
        !self.without_catalog
    }

    async fn health(&self) -> Health {
        Health::Healthy
    }

    async fn execute(&self, request: QueryRequest) -> aster_core::Result<QueryResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.catalog_seen.lock().expect("catalog lock") = request.catalog.clone();
        if let Some(message) = self.refusal.lock().expect("refusal lock").clone() {
            return Err(CoreError::Query(message));
        }
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
            segments: Vec::new(),
            name: "default".into(),
        }])
    }

    async fn list_tables(&self, namespace: &str) -> aster_core::Result<Vec<TableRef>> {
        Ok(match namespace {
            "default" => vec![TableRef {
                namespace_segments: Vec::new(),
                namespace: "default".into(),
                name: "orders".into(),
            }],
            _ => vec![],
        })
    }

    async fn table_schema(&self, table: &TableRef) -> aster_core::Result<TableSchema> {
        Ok(TableSchema {
            table: table.clone(),
            columns: vec![
                ColumnSchema {
                    name: "order_id".into(),
                    data_type: "bigint".into(),
                    nullable: false,
                },
                ColumnSchema {
                    name: "total".into(),
                    data_type: "decimal(12,2)".into(),
                    nullable: false,
                },
            ],
        })
    }
}

#[derive(Default, World)]
struct Contract {
    /// Executions of the stub engine, observable across steps.
    calls: Arc<AtomicUsize>,
    /// When set, the stub engine refuses the statement with this message.
    refusal: Arc<Mutex<Option<String>>>,
    /// The session catalog the stub engine last saw.
    catalog_seen: Arc<Mutex<Option<String>>>,
    /// The configured default catalog for the query path.
    default_catalog: Option<String>,
    /// Makes the stub engine one without a session catalog.
    engine_without_catalog: bool,
    /// Temporary git checkout for the notebook scenarios.
    dir: Option<PathBuf>,
    owner_revision: Option<String>,
    owner_head: Option<String>,
    race_statuses: Option<(StatusCode, StatusCode)>,
    owner_source: Option<String>,
    notebook_owners: Arc<aster_core::InMemoryNotebookOwners>,
    notebook_write: Arc<tokio::sync::Mutex<()>>,
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
    extra_engine: Option<String>,
    extra_bound_catalog: Option<String>,
    binding_disabled: bool,
    exchanges: Arc<aster_core::InMemoryExchanges>,
    exchange_notebook: Option<String>,
    exchange_cell: Option<String>,
    exchange_original_revision: Option<String>,
    exchange_summary: Option<Value>,
    exchange_query: Option<Value>,
    exchange_result: Option<Value>,
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
        app_recorded(self.state())
    }

    /// The shared state, reused when a scenario drives a second router (the
    /// metrics listener) against the same process.
    fn state(&mut self) -> Arc<AppState> {
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
            refusal: Arc::clone(&self.refusal),
            catalog_seen: Arc::clone(&self.catalog_seen),
            without_catalog: self.engine_without_catalog,
        }));
        if let Some(extra) = &self.extra_engine {
            engines.register(Arc::new(StubEngine {
                info: EngineInfo {
                    id: EngineId::new(extra.clone()),
                    kind: "stub".into(),
                    endpoint: "http://engine.invalid".into(),
                    routing_group: None,
                },
                calls: Arc::clone(&self.calls),
                refusal: Arc::clone(&self.refusal),
                catalog_seen: Arc::clone(&self.catalog_seen),
                without_catalog: false,
            }));
        }
        let mut catalogs = CatalogRegistry::new();
        catalogs.register(Arc::new(StubCatalog {
            id: CatalogId::new(
                self.catalog
                    .clone()
                    .unwrap_or_else(|| "polaris-local".into()),
            ),
        }));
        if let Some(extra) = &self.extra_bound_catalog {
            catalogs.register(Arc::new(StubCatalog {
                id: CatalogId::new(extra.clone()),
            }));
        }

        let config = AppConfig {
            bind: "127.0.0.1:0".into(),
            engines: vec![EngineConfig {
                id: id.clone(),
                kind: "stub".into(),
                endpoint: "http://engine.invalid".into(),
                routing_group: None,
                delegation: None,
            }]
            .into_iter()
            .chain(self.extra_engine.iter().map(|extra| EngineConfig {
                id: extra.clone(),
                kind: "stub".into(),
                endpoint: "http://engine.invalid".into(),
                routing_group: None,
                delegation: None,
            }))
            .collect(),
            catalogs: vec![CatalogConfig {
                id: self
                    .catalog
                    .clone()
                    .unwrap_or_else(|| "polaris-local".into()),
                kind: "stub".into(),
                endpoint: "http://catalog.invalid".into(),
                catalog: None,
                token: None,
                credential: None,
            }]
            .into_iter()
            .chain(self.extra_bound_catalog.iter().map(|extra| CatalogConfig {
                id: extra.clone(),
                kind: "stub".into(),
                endpoint: "http://catalog.invalid".into(),
                catalog: None,
                token: None,
                credential: None,
            }))
            .collect(),
            catalog_bindings: (!self.binding_disabled)
                .then_some(CatalogBindingConfig {
                    catalog: self
                        .catalog
                        .clone()
                        .unwrap_or_else(|| "polaris-local".into()),
                    engine: id.clone(),
                    native_catalog: "polaris".into(),
                    policy: BindingPolicy::Unprotected,
                })
                .into_iter()
                .chain(
                    self.extra_bound_catalog
                        .iter()
                        .map(|extra| CatalogBindingConfig {
                            catalog: extra.clone(),
                            engine: id.clone(),
                            native_catalog: extra.clone(),
                            policy: BindingPolicy::Unprotected,
                        }),
                )
                .collect(),
            default_engine: self.default_engine.as_ref().map(|engine| engine.0.clone()),
            default_catalog: self.default_catalog.clone(),
        };

        let notebooks: Arc<dyn NotebookStore> = Arc::new(
            GitNotebookStore::open(self.checkout(), "session/alice".to_string())
                .expect("notebook store"),
        );
        Arc::new(AppState {
            conversations: Arc::new(aster_core::InMemoryConversations::default()),
            exchanges: self.exchanges.clone(),
            config,
            engines,
            catalogs,
            grants: grants_dyn(self),
            audit: audit_dyn(self),
            notebooks,
            notebook_owners: self.notebook_owners.clone(),
            notebook_write: self.notebook_write.clone(),
            team_workspaces: None,
            team_git_targets: None,
            llm: Arc::new(InMemoryLlm::new()),
            shared_models: None,
            current_identity: None,
            shared_model_use_enabled: false,
            compiled_contracts: None,
            contracts: Arc::new(Vec::new()),
            http: reqwest::Client::new(),
            sessions: Arc::new(InMemorySessions::new(3600)),
            handshakes: Arc::new(InMemoryHandshakes::new(300)),
            session_ttl_seconds: 3600,
            user_state: user_state_dyn(self),
            secrets: Arc::new(aster_core::InMemorySecrets::new()),
            identity: None,
            dev_login: true,
            metrics: Arc::new(aster_server::Metrics::new()),
        })
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

    /// Drives the public router once, then scrapes the metrics listener of the
    /// same state, so the counters have something to report.
    async fn scrape(&mut self) {
        let state = self.state();
        let public = app_recorded(Arc::clone(&state));
        let request = Request::builder()
            .uri("/healthz")
            .body(Body::empty())
            .expect("healthz request");
        let _ = public.oneshot(request).await.expect("healthz");

        let metrics = aster_server::metrics_router(state);
        let request = Request::builder()
            .uri("/metrics")
            .body(Body::empty())
            .expect("metrics request");
        let response = metrics.oneshot(request).await.expect("metrics");
        self.status = Some(response.status());
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body");
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

#[given(expr = "the engine refuses the statement with {string}")]
async fn engine_refuses(world: &mut Contract, message: String) {
    *world.refusal.lock().expect("refusal lock") = Some(message);
}

#[given(expr = "the default catalog is {string}")]
async fn default_catalog(world: &mut Contract, catalog: String) {
    world.default_catalog = Some(catalog);
}

#[given(expr = "the engine has no session catalog")]
async fn engine_without_catalog(world: &mut Contract) {
    world.engine_without_catalog = true;
}

#[given(expr = "subject {string} is granted engine {string}")]
async fn is_granted(world: &mut Contract, subject: String, engine: String) {
    world
        .grants
        .get_or_insert_with(|| Arc::new(InMemoryGrants::new()))
        .grant(subject, engine)
}

#[given(expr = "engine {string} is registered without a catalog binding")]
async fn extra_unbound_engine(world: &mut Contract, engine: String) {
    world.extra_engine = Some(engine);
}

#[given(expr = "no catalog binding is configured")]
async fn no_catalog_binding(world: &mut Contract) {
    world.binding_disabled = true;
}

#[given(expr = "the engine also binds catalog {string}")]
async fn extra_bound_catalog(world: &mut Contract, catalog: String) {
    world.extra_bound_catalog = Some(catalog);
}

#[when(expr = "{string} calls ListEngines for browse catalog {string}")]
async fn rpc_list_engines_in_context(world: &mut Contract, subject: String, context: String) {
    let request = connect(
        "ListEngines",
        Some(&subject),
        "editor",
        json!({"catalogContext": context}),
    );
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[when(expr = "{string} without a role calls ListEngines for browse catalog {string}")]
async fn rpc_list_engines_without_role(world: &mut Contract, subject: String, context: String) {
    let request = connect(
        "ListEngines",
        Some(&subject),
        "none",
        json!({"catalogContext": context}),
    );
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[then(expr = "the RPC call is permission denied")]
async fn rpc_permission_denied(world: &mut Contract) {
    assert_eq!(world.rpc_json()["code"], "permission_denied");
}

#[then(expr = "only RPC engine {string} is offered")]
async fn only_rpc_engine_offered(world: &mut Contract, engine: String) {
    let ids: Vec<&str> = world.rpc_json()["engines"]
        .as_array()
        .expect("RPC engines")
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert_eq!(ids, vec![engine.as_str()]);
}

#[when(expr = "subject {string} lists engines for browse catalog {string}")]
async fn list_engines_in_context(world: &mut Contract, subject: String, context: String) {
    let request = caller(
        "GET",
        &format!("/api/engines?catalog_context={context}"),
        Some(&subject),
        "editor",
        None,
    );
    world.send(request).await;
}

#[then(expr = "only engine {string} is offered")]
async fn only_engine_offered(world: &mut Contract, engine: String) {
    let ids: Vec<&str> = world
        .json
        .as_ref()
        .expect("JSON inventory")
        .as_array()
        .expect("engine array")
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    assert_eq!(ids, vec![engine.as_str()]);
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
    world
        .grants
        .get_or_insert_with(|| Arc::new(InMemoryGrants::new()))
        .grant("alice", "trino-local");
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

#[when(expr = "an unidentified caller reads the engine inventory")]
async fn anonymous_engines(world: &mut Contract) {
    let request = caller("GET", "/api/engines", None, "editor", None);
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

#[when(expr = "subject {string} queries {string} in browse catalog {string}")]
async fn query_in_browse_catalog(
    world: &mut Contract,
    subject: String,
    engine: String,
    catalog_context: String,
) {
    let request = caller(
        "POST",
        "/api/query",
        Some(&subject),
        "editor",
        Some(json!({"sql": "SELECT 1", "engine": engine, "catalog_context": catalog_context})),
    );
    world.send(request).await;
}

#[when(
    expr = "subject {string} queries {string} in browse catalog {string} with SQL catalog {string}"
)]
async fn query_in_browse_catalog_with_alias(
    world: &mut Contract,
    subject: String,
    engine: String,
    context: String,
    alias: String,
) {
    let request = caller(
        "POST",
        "/api/query",
        Some(&subject),
        "editor",
        Some(
            json!({"sql": "SELECT 1", "engine": engine, "catalog_context": context, "catalog": alias}),
        ),
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

/// Percent-encode for the query string: `Request::builder().uri` rejects the
/// spaces and `*` a SQL snippet carries.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[when(expr = "subject {string} asks for completion after {string}")]
async fn completion_after(world: &mut Contract, subject: String, sql: String) {
    let path = format!("/api/sql/complete?sql={}", encode(&sql));
    let request = caller("GET", &path, Some(&subject), "editor", None);
    world.send(request).await;
}

#[then(expr = "the suggestions include catalog {string}")]
async fn suggestions_include_catalog(world: &mut Contract, label: String) {
    assert_suggestion(world, &label, "catalog");
}

#[then(expr = "the suggestions include schema {string}")]
async fn suggestions_include_schema(world: &mut Contract, label: String) {
    assert_suggestion(world, &label, "schema");
}

#[then(expr = "the suggestions include table {string}")]
async fn suggestions_include_table(world: &mut Contract, label: String) {
    assert_suggestion(world, &label, "table");
}

#[then(expr = "the suggestions include column {string}")]
async fn suggestions_include_column(world: &mut Contract, label: String) {
    assert_suggestion(world, &label, "column");
}

#[then(expr = "the suggestions are empty")]
async fn suggestions_are_empty(world: &mut Contract) {
    let items = suggestions(world);
    assert!(items.is_empty(), "expected no suggestions, got {items:?}");
}

/// A name that is not a plain identifier has to reach the cell quoted, or the
/// suggestion would produce SQL the engine refuses.
#[then(expr = "the catalog {string} is suggested quoted")]
async fn catalog_suggested_quoted(world: &mut Contract, label: String) {
    let items = suggestions(world);
    let item = items
        .iter()
        .find(|item| item.get("label").and_then(Value::as_str) == Some(&label))
        .unwrap_or_else(|| panic!("no suggestion for {label:?} in {items:?}"));
    let insert = item
        .get("insert")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert_eq!(insert, format!("\"{label}\""), "unquoted insert");
}

fn suggestions(world: &Contract) -> Vec<Value> {
    world
        .json
        .as_ref()
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn assert_suggestion(world: &Contract, label: &str, kind: &str) {
    let items = suggestions(world);
    let found = items.iter().any(|item| {
        item.get("label").and_then(Value::as_str) == Some(label)
            && item.get("kind").and_then(Value::as_str) == Some(kind)
    });
    assert!(found, "expected a {kind} suggestion {label:?} in {items:?}");
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
        json!({"id": id, "ifAbsent": true, "notebook": {"id": id, "title": "Sales", "cells": [{"id": "c1", "sql": sql}]}}),
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

#[when(expr = "{string} renders the {string} table {string} as {string}")]
async fn render_semantic(
    world: &mut Contract,
    subject: String,
    namespace: String,
    table: String,
    target: String,
) {
    let request = connect(
        "RenderSemantic",
        Some(&subject),
        "editor",
        json!({"target": target, "catalog": "polaris-local", "namespace": namespace, "table": table}),
    );
    world.send(request).await;
    world.rpc = world.json.clone();
}

#[then(expr = "the response names the cube {string} and its path")]
async fn response_names_cube(world: &mut Contract, name: String) {
    let response = world.rpc_json();
    let content = response
        .get("content")
        .and_then(Value::as_str)
        .expect("content");
    assert!(
        content.contains(&format!("cubes:\n  - name: \"{name}\"")),
        "content was:\n{content}"
    );
    assert_eq!(
        response.get("path").and_then(Value::as_str),
        Some("model/cubes/orders.yml")
    );
}

#[then("the response carries an odcs contract with fields")]
async fn response_carries_contract(world: &mut Contract) {
    let response = world.rpc_json();
    let content = response
        .get("content")
        .and_then(Value::as_str)
        .expect("content");
    assert!(
        content.contains("kind: DataContract"),
        "content was:\n{content}"
    );
    assert!(
        content.contains("      - name: \"total\""),
        "content was:\n{content}"
    );
    assert_eq!(
        response.get("path").and_then(Value::as_str),
        Some("contracts/orders.yaml")
    );
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

/// The engine's own wording reaches the caller, rather than the generic message
/// the 5xx path hands out: a statement mistake has to read as a statement mistake.
/// A statement with no qualifier needs the configured default to reach the
/// engine as its session catalog, or Trino refuses it outright.
#[then(expr = "the query ran with catalog {string}")]
async fn ran_with_catalog(world: &mut Contract, catalog: String) {
    let seen = world.catalog_seen.lock().expect("catalog lock").clone();
    assert_eq!(seen.as_deref(), Some(catalog.as_str()), "session catalog");
}

#[then(expr = "the query ran without a catalog")]
async fn ran_without_catalog(world: &mut Contract) {
    let seen = world.catalog_seen.lock().expect("catalog lock").clone();
    assert!(seen.is_none(), "expected no session catalog, saw {seen:?}");
}

#[then(expr = "the error names the engine's reason")]
async fn error_names_engine_reason(world: &mut Contract) {
    let expected = world
        .refusal
        .lock()
        .expect("refusal lock")
        .clone()
        .expect("a refusal was configured");
    let message = world
        .json
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(Value::as_str)
        .unwrap_or(&world.body)
        .to_string();
    assert!(
        message.contains(&expected),
        "expected the engine's message {expected:?}, got {message:?}"
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

#[then(expr = "the engine was not called")]
async fn engine_not_called(world: &mut Contract) {
    assert_eq!(world.calls.load(Ordering::SeqCst), 0, "engine was called");
}

#[then(expr = "a refused audit event is recorded for subject {string}")]
async fn refused_audit_recorded(world: &mut Contract, subject: String) {
    let events = world
        .audit
        .as_ref()
        .expect("audit sink")
        .events()
        .await
        .expect("audit events");
    assert!(
        events
            .iter()
            .any(|event| event.subject == subject && !event.ok),
        "no refused audit event for {subject}"
    );
}

#[then(expr = "the latest audit event names browse catalog {string}")]
async fn audit_names_browse_catalog(world: &mut Contract, context: String) {
    let events = world
        .audit
        .as_ref()
        .expect("audit sink")
        .events()
        .await
        .expect("audit events");
    assert_eq!(
        events.last().expect("audit event").catalog.as_deref(),
        Some(context.as_str())
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

#[then(expr = "the call fails as permission denied naming the missing grant")]
async fn permission_denied_with_grant(world: &mut Contract) {
    // The caller is authenticated, so a missing grant is 403, not 401 — the
    // same meaning the REST layer gives it.
    assert_eq!(world.status.expect("response").as_u16(), 403);
    assert_eq!(world.rpc_json()["code"], "permission_denied");
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

#[when(expr = "metrics are requested from the public router")]
async fn public_metrics(world: &mut Contract) {
    let request = world.get("/metrics");
    world.send(request).await;
}

#[when(expr = "the metrics listener is scraped")]
async fn scrape_metrics(world: &mut Contract) {
    world.scrape().await;
}

#[then(expr = "the scrape names {string}")]
async fn scrape_names(world: &mut Contract, metric: String) {
    assert!(
        world.body.contains(&metric),
        "{metric} missing from {}",
        world.body
    );
}

#[then(expr = "the scrape carries a route label")]
async fn scrape_has_route_label(world: &mut Contract) {
    assert!(
        world.body.contains("route="),
        "no route label in {}",
        world.body
    );
}

#[then(expr = "the call does not succeed")]
async fn call_does_not_succeed(world: &mut Contract) {
    let status = world.status.expect("response");
    assert!(!status.is_success(), "unexpected {status}: {}", world.body);
}

#[given(expr = "subject {string} has saved notebook {string} titled {string}")]
async fn saved_notebook_titled(world: &mut Contract, subject: String, id: String, title: String) {
    let mut request = caller(
        "PUT",
        &format!("/api/notebooks/{id}"),
        Some(&subject),
        "editor",
        Some(json!({"id": id, "title": title, "cells": [{"id": "c1", "sql": "SELECT 1"}]})),
    );
    request
        .headers_mut()
        .insert(header::IF_NONE_MATCH, "*".parse().unwrap());
    world.send(request).await;
}

#[when(expr = "subject {string} opens the index page")]
async fn open_index(world: &mut Contract, subject: String) {
    let request = caller("GET", "/", Some(&subject), "editor", None);
    world.send(request).await;
}

#[when(expr = "a caller without identity opens the index page")]
async fn open_index_anonymous(world: &mut Contract) {
    let request = caller("GET", "/", None, "editor", None);
    world.send(request).await;
}

#[when(expr = "subject {string} opens the {string} notebook page")]
async fn open_notebook_page(world: &mut Contract, subject: String, id: String) {
    let request = caller(
        "GET",
        &format!("/notebooks/{id}"),
        Some(&subject),
        "editor",
        None,
    );
    world.send(request).await;
}

#[then(expr = "the page carries the application shell")]
async fn page_carries_shell(world: &mut Contract) {
    for marker in [
        "class=\"topbar\"",
        "<nav class=\"nav\">",
        "id=\"status\"",
        "href=\"/logout\"",
    ] {
        assert!(
            world.body.contains(marker),
            "page is missing {marker}: {}",
            world.body
        );
    }
}

#[then(expr = "the page renders one editor and one run action per cell")]
async fn page_renders_cells(world: &mut Contract) {
    let editors = world.body.matches("class=\"editor\"").count();
    let runs = world.body.matches("data-action=\"run\"").count();
    assert!(editors > 0, "no cell editor rendered: {}", world.body);
    assert_eq!(editors, runs, "every cell needs an editor and a run action");
    assert!(world.body.contains("class=\"engine\""), "no engine choice");
    assert!(
        world.body.contains("id=\"nb\""),
        "notebook JSON not embedded"
    );
}

#[then(expr = "the notebook title is escaped in the markup")]
async fn title_escaped(world: &mut Contract) {
    assert!(
        !world.body.contains("<script>alert(1)</script>"),
        "raw notebook title reached the markup"
    );
    assert!(
        world.body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "escaped title missing: {}",
        world.body
    );
}

/// Saves through the JSON API, so the git store sees the same path the page uses.
async fn save_notebook_via_api(
    world: &mut Contract,
    subject: Option<&str>,
    role: &str,
    id: &str,
    sql: &str,
) {
    let current = notebook_blob(world, id);
    let mut request = caller(
        "PUT",
        &format!("/api/notebooks/{id}"),
        subject,
        role,
        Some(json!({"id": id, "title": "Sales", "cells": [{"id": "c1", "sql": sql}]})),
    );
    match current {
        Some(oid) => {
            request
                .headers_mut()
                .insert(header::IF_MATCH, format!("\"{oid}\"").parse().unwrap());
        }
        None => {
            request
                .headers_mut()
                .insert(header::IF_NONE_MATCH, "*".parse().unwrap());
        }
    }
    world.send(request).await;
}

fn notebook_blob(world: &mut Contract, id: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(world.checkout())
        .args(["rev-parse", "--verify", &format!("HEAD:{id}.aster")])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn owner_notebook(id: &str, sql: &str) -> Notebook {
    Notebook {
        id: id.into(),
        title: "Sales".into(),
        cells: vec![aster_core::Cell {
            id: "c1".into(),
            sql: sql.into(),
            engine: None,
            metadata: Default::default(),
        }],
    }
}

fn git_read(world: &mut Contract, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(world.checkout())
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn owner_put(
    world: &mut Contract,
    subject: &str,
    role: &str,
    id: &str,
    sql: &str,
    precondition: Option<(&str, String)>,
) {
    let mut request = caller(
        "PUT",
        &format!("/api/notebooks/{id}"),
        Some(subject),
        role,
        Some(json!(owner_notebook(id, sql))),
    );
    if let Some((name, value)) = precondition {
        request.headers_mut().insert(
            name.parse::<header::HeaderName>().unwrap(),
            value.parse().unwrap(),
        );
    }
    world.send(request).await;
}

async fn owner_assign(world: &mut Contract, subject: &str, role: &str, body: Value) {
    let request = caller(
        "PUT",
        "/api/admin/notebooks/sales/owner",
        Some(subject),
        role,
        Some(body),
    );
    world.send(request).await;
}

#[given("a notebook is committed on the legacy source branch without an owner")]
async fn owner_legacy_unassigned(world: &mut Contract) {
    let store = GitNotebookStore::open(world.checkout(), "session/alice").unwrap();
    store
        .save(&owner_notebook("sales", "SELECT 1"), "seed")
        .await
        .unwrap();
    drop(store);
    world.owner_revision = notebook_blob(world, "sales");
    world.owner_head = git_read(world, &["rev-parse", "HEAD"]);
}

#[when("an editor saves a change with the current content revision")]
async fn owner_unassigned_save(world: &mut Contract) {
    let oid = world.owner_revision.clone().unwrap();
    owner_put(
        world,
        "alice",
        "editor",
        "sales",
        "SELECT 2",
        Some(("if-match", format!("\"{oid}\""))),
    )
    .await;
}

#[then("the save is refused without changing the file, index or branch revision")]
async fn owner_save_refused_unchanged(world: &mut Contract) {
    assert_eq!(world.status, Some(StatusCode::FORBIDDEN));
    let head = git_read(world, &["rev-parse", "HEAD"]);
    let blob = notebook_blob(world, "sales");
    assert_eq!(world.owner_head, head);
    assert_eq!(world.owner_revision, blob);
    assert_eq!(
        git_read(world, &["diff", "--cached", "--name-only"]).as_deref(),
        Some("")
    );
    assert_eq!(
        git_read(world, &["status", "--porcelain"]).as_deref(),
        Some("")
    );
}

#[given("a committed legacy notebook has a known content revision")]
async fn owner_known_legacy(world: &mut Contract) {
    owner_legacy_unassigned(world).await;
}

#[when("a verified administrator assigns an exact owner against that revision")]
async fn owner_admin_assign(world: &mut Contract) {
    owner_assign(
        world,
        "root",
        "admin",
        json!({"owner":"alice", "expected_content_revision":world.owner_revision.clone().unwrap()}),
    )
    .await;
    assert_eq!(world.status, Some(StatusCode::OK));
}

#[then("the assignment is recorded without changing Git")]
async fn owner_recorded_without_git(world: &mut Contract) {
    let head = git_read(world, &["rev-parse", "HEAD"]);
    let blob = notebook_blob(world, "sales");
    assert_eq!(world.owner_head, head);
    assert_eq!(world.owner_revision, blob);
    let events = world.notebook_owners.events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].owner, "alice");
}

#[then("an ordinary editor cannot assign or correct the owner")]
async fn owner_editor_cannot_assign(world: &mut Contract) {
    owner_assign(world, "bob", "editor", json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":world.owner_revision.clone().unwrap(), "reason":"unauthorized"})).await;
    assert_eq!(world.status, Some(StatusCode::FORBIDDEN));
}

#[when("that administrator corrects a mistaken owner with the current owner and revision")]
async fn owner_admin_correct(world: &mut Contract) {
    owner_assign(world, "root", "admin", json!({"owner":"bob", "expected_owner":"alice", "expected_content_revision":world.owner_revision.clone().unwrap(), "reason":"correct typo"})).await;
    assert_eq!(world.status, Some(StatusCode::OK));
}

#[then("the old owner loses write access immediately")]
async fn owner_old_owner_revoked(world: &mut Contract) {
    let oid = world.owner_revision.clone().unwrap();
    owner_put(
        world,
        "alice",
        "editor",
        "sales",
        "SELECT 2",
        Some(("if-match", format!("\"{oid}\""))),
    )
    .await;
    assert_eq!(world.status, Some(StatusCode::FORBIDDEN));
}

#[then("the correction reason and administrator are recorded")]
async fn owner_correction_audited(world: &mut Contract) {
    let events = world.notebook_owners.events().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].expected_owner.as_deref(), Some("alice"));
    assert_eq!(events[1].owner, "bob");
    assert_eq!(events[1].actor, "root");
    assert_eq!(events[1].reason.as_deref(), Some("correct typo"));
}

#[given("Alice owns a notebook and two tabs loaded the same content revision")]
async fn owner_two_tabs(world: &mut Contract) {
    owner_put(
        world,
        "alice",
        "editor",
        "sales",
        "SELECT 1",
        Some(("if-none-match", "*".into())),
    )
    .await;
    assert_eq!(world.status, Some(StatusCode::OK));
    world.owner_revision = notebook_blob(world, "sales");
}

#[when("the first tab saves a change")]
async fn owner_first_tab(world: &mut Contract) {
    let oid = world.owner_revision.clone().unwrap();
    owner_put(
        world,
        "alice",
        "editor",
        "sales",
        "SELECT 2",
        Some(("if-match", format!("\"{oid}\""))),
    )
    .await;
    assert_eq!(world.status, Some(StatusCode::OK));
}

#[then("the second tab's save is refused as a conflict")]
async fn owner_second_tab_conflict(world: &mut Contract) {
    let oid = world.owner_revision.clone().unwrap();
    owner_put(
        world,
        "alice",
        "editor",
        "sales",
        "SELECT 3",
        Some(("if-match", format!("\"{oid}\""))),
    )
    .await;
    assert_eq!(world.status, Some(StatusCode::CONFLICT));
}

#[then("the first tab's committed content remains readable")]
async fn owner_first_tab_readable(world: &mut Contract) {
    let request = caller("GET", "/api/notebooks/sales", Some("alice"), "editor", None);
    world.send(request).await;
    assert_eq!(world.json.as_ref().unwrap()["cells"][0]["sql"], "SELECT 2");
}

#[given("Alice loaded notebook sales with its content revision")]
async fn owner_loaded_sales(world: &mut Contract) {
    owner_two_tabs(world).await;
}

#[when("another notebook is committed before Alice saves sales")]
async fn owner_unrelated_commit(world: &mut Contract) {
    owner_put(
        world,
        "alice",
        "editor",
        "other",
        "SELECT 9",
        Some(("if-none-match", "*".into())),
    )
    .await;
    assert_eq!(world.status, Some(StatusCode::OK));
}

#[then("Alice can save sales with her original content revision")]
async fn owner_unrelated_allows_save(world: &mut Contract) {
    let oid = world.owner_revision.clone().unwrap();
    owner_put(
        world,
        "alice",
        "editor",
        "sales",
        "SELECT 2",
        Some(("if-match", format!("\"{oid}\""))),
    )
    .await;
    assert_eq!(world.status, Some(StatusCode::OK));
}

#[given("no committed notebook has the requested ID")]
async fn owner_absent(world: &mut Contract) {
    world.checkout();
}

#[when("Alice creates it without an explicit absence precondition")]
async fn owner_missing_precondition(world: &mut Contract) {
    owner_put(world, "alice", "editor", "sales", "SELECT 1", None).await;
}

#[then("the create is refused before a Git write")]
async fn owner_missing_precondition_no_write(world: &mut Contract) {
    assert_eq!(world.status, Some(StatusCode::BAD_REQUEST));
    assert!(git_read(world, &["rev-parse", "--verify", "HEAD"]).is_none());
    assert!(git_read(world, &["status", "--porcelain"])
        .unwrap()
        .is_empty());
}

#[given("Alice is creating a notebook while an admin assigns legacy ownership")]
async fn owner_create_assignment_race(world: &mut Contract) {
    world.checkout();
}

#[when("both operations target the same notebook ID")]
async fn owner_race(world: &mut Contract) {
    use std::io::Write;
    let state = world.state();
    let source = state.notebooks.source_key();
    let mut child = std::process::Command::new("git")
        .args(["hash-object", "--stdin"])
        .current_dir(world.checkout())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(owner_notebook("sales", "SELECT 1").to_text().as_bytes())
        .unwrap();
    let oid = String::from_utf8(child.wait_with_output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    let mut create = caller(
        "PUT",
        "/api/notebooks/sales",
        Some("alice"),
        "editor",
        Some(json!(owner_notebook("sales", "SELECT 1"))),
    );
    create
        .headers_mut()
        .insert(header::IF_NONE_MATCH, "*".parse().unwrap());
    let assign = caller(
        "PUT",
        "/api/admin/notebooks/sales/owner",
        Some("root"),
        "admin",
        Some(json!({"owner":"bob", "expected_content_revision":oid})),
    );
    let app = app_recorded(state);
    let (created, assigned) = tokio::join!(app.clone().oneshot(create), app.oneshot(assign));
    world.race_statuses = Some((created.unwrap().status(), assigned.unwrap().status()));
    world.owner_source = Some(source);
}

#[then("one serial order determines the owner")]
async fn owner_race_one_winner(world: &mut Contract) {
    let (created, assigned) = world.race_statuses.unwrap();
    assert_eq!(created, StatusCode::OK);
    assert!(matches!(
        assigned,
        StatusCode::NOT_FOUND | StatusCode::CONFLICT
    ));
    let record = world
        .notebook_owners
        .record(world.owner_source.as_deref().unwrap(), "sales")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.owner, "alice");
}

#[then("no caller commits under another owner's assignment")]
async fn owner_race_git_consistent(world: &mut Contract) {
    assert_eq!(
        git_read(world, &["show", "HEAD:sales.aster"]).unwrap(),
        owner_notebook("sales", "SELECT 1").to_text().trim_end()
    );
    assert!(git_read(world, &["log", "-1", "--pretty=%s"])
        .unwrap()
        .starts_with("alice:"));
}

// --- notebook session exchange -------------------------------------------

use aster_core::ExchangeStore as _;

/// Create the notebook through the real API so the git-backed document
/// survives the per-request AppState rebuilds of this harness.
async fn exchange_open(world: &mut Contract, notebook: &str, cell: &str, sql: &str) {
    let mut request = caller(
        "PUT",
        &format!("/api/notebooks/{notebook}"),
        Some("alice"),
        "editor",
        Some(serde_json::json!({
            "id": notebook,
            "title": notebook,
            "cells": [{"id": cell, "sql": sql, "engine": null}],
        })),
    );
    request
        .headers_mut()
        .insert(header::IF_NONE_MATCH, "*".parse().unwrap());
    world.send(request).await;
    assert_eq!(world.status, Some(StatusCode::OK), "{}", world.body);
    world.exchange_notebook = Some(notebook.to_string());
    world.exchange_cell = Some(cell.to_string());
}

/// Record a cell's last result directly in the exchange store the router uses.
async fn exchange_record(world: &Contract, notebook: &str, cell: &str, column: &str, rows: &str) {
    world
        .exchanges
        .set_result(
            "alice",
            notebook,
            cell,
            aster_core::CellResult {
                columns: vec![column.to_string()],
                rows_json: vec![rows.to_string()],
                truncated: false,
            },
        )
        .await
        .expect("record result");
}

async fn exchange_call(world: &mut Contract, method: &str, body: Value) -> Value {
    let request = connect(method, Some("alice"), "editor", body);
    world.send(request).await;
    world.json.clone().unwrap_or(Value::Null)
}

async fn exchange_revision(world: &mut Contract, notebook: &str, cell: &str) -> String {
    let response = exchange_call(
        world,
        "FetchQuery",
        serde_json::json!({"notebook": notebook, "cell": cell}),
    )
    .await;
    response["contentRevision"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn exchange_notebook(world: &Contract) -> String {
    world.exchange_notebook.clone().expect("notebook")
}

fn exchange_cell(world: &Contract) -> String {
    world.exchange_cell.clone().expect("cell")
}

#[given(expr = "a notebook {string} with a cell {string} whose SQL is {string}")]
async fn exchange_given_notebook(
    world: &mut Contract,
    notebook: String,
    cell: String,
    sql: String,
) {
    exchange_open(world, &notebook, &cell, &sql).await;
    world.exchange_original_revision = Some(exchange_revision(world, &notebook, &cell).await);
}

#[given(
    expr = "a notebook {string} with a cell {string} whose SQL is {string} and whose last result column is {string}"
)]
async fn exchange_given_notebook_with_result(
    world: &mut Contract,
    notebook: String,
    cell: String,
    sql: String,
    column: String,
) {
    exchange_open(world, &notebook, &cell, &sql).await;
    exchange_record(world, &notebook, &cell, &column, "[1]").await;
}

#[given(expr = "a notebook {string} with no summary")]
async fn exchange_given_without_summary(world: &mut Contract, notebook: String) {
    exchange_open(world, &notebook, "q1", "SELECT 1 AS one").await;
}

#[given(expr = "a notebook {string} and a notebook {string} that each have a cell {string}")]
async fn exchange_given_two_notebooks(
    world: &mut Contract,
    first: String,
    second: String,
    cell: String,
) {
    exchange_open(world, &first, &cell, "SELECT 1 AS one").await;
    exchange_record(world, &first, &cell, "one", "[1]").await;
    exchange_open(world, &second, &cell, "SELECT 9 AS nine").await;
    exchange_record(world, &second, &cell, "nine", "[9]").await;
    /* Opening the second notebook moved the world's current notebook; this
    scenario is about the first one, so point it back. */
    world.exchange_notebook = Some(first);
}

#[when(expr = "the notebook session fetches the query and the result for cell {string}")]
async fn exchange_when_fetches_query_and_result(world: &mut Contract, cell: String) {
    let notebook = exchange_notebook(world);
    let query = exchange_call(
        world,
        "FetchQuery",
        serde_json::json!({"notebook": notebook, "cell": cell}),
    )
    .await;
    world.exchange_query = Some(query);
    let result = exchange_call(
        world,
        "FetchResult",
        serde_json::json!({"notebook": notebook, "cell": cell}),
    )
    .await;
    world.exchange_result = Some(result);
}

#[then(expr = "it receives the SQL {string} and the result column {string}")]
async fn exchange_then_sql_and_column(world: &mut Contract, sql: String, column: String) {
    let query = world.exchange_query.clone().expect("query response");
    assert_eq!(query["sql"], serde_json::json!(sql), "{query}");
    let result = world.exchange_result.clone().expect("result response");
    let columns = result["columns"].as_array().cloned().unwrap_or_default();
    assert!(columns.contains(&serde_json::json!(column)), "{result}");
}

#[when(expr = "the notebook session updates the query of cell {string} to {string}")]
async fn exchange_when_updates_query(world: &mut Contract, cell: String, sql: String) {
    let notebook = exchange_notebook(world);
    let expected = world
        .exchange_original_revision
        .clone()
        .expect("original revision");
    let updated = exchange_call(
        world,
        "UpdateQuery",
        serde_json::json!({
            "notebook": notebook, "cell": cell, "sql": sql,
            "expectedContentRevision": expected,
        }),
    )
    .await;
    world.exchange_query = Some(updated);
}

#[then(expr = "cell {string} holds {string} at a higher revision")]
async fn exchange_then_cell_holds(world: &mut Contract, cell: String, sql: String) {
    let notebook = exchange_notebook(world);
    let updated = world.exchange_query.clone().expect("update response");
    let revision = updated["contentRevision"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert_ne!(
        revision,
        world.exchange_original_revision.clone().unwrap_or_default(),
        "the revision did not move"
    );
    let fetched = exchange_call(
        world,
        "FetchQuery",
        serde_json::json!({"notebook": notebook, "cell": cell}),
    )
    .await;
    assert_eq!(fetched["sql"], serde_json::json!(sql), "{fetched}");
    assert_eq!(
        fetched["contentRevision"],
        serde_json::json!(revision),
        "{fetched}"
    );
}

#[then(expr = "an update carrying the previous revision is refused as a conflict")]
async fn exchange_then_stale_refused(world: &mut Contract) {
    let notebook = exchange_notebook(world);
    let cell = exchange_cell(world);
    let stale = world
        .exchange_original_revision
        .clone()
        .expect("original revision");
    let request = connect(
        "UpdateQuery",
        Some("alice"),
        "editor",
        serde_json::json!({
            "notebook": notebook, "cell": cell,
            "sql": "SELECT 3 AS three", "expectedContentRevision": stale,
        }),
    );
    world.send(request).await;
    assert_eq!(world.status, Some(StatusCode::CONFLICT), "{}", world.body);
}

#[when(expr = "the notebook session sends the summary {string}")]
async fn exchange_when_sends_summary(world: &mut Contract, summary: String) {
    let notebook = exchange_notebook(world);
    let sent = exchange_call(
        world,
        "SendSummary",
        serde_json::json!({"notebook": notebook, "summary": summary}),
    )
    .await;
    world.exchange_summary = Some(sent);
}

#[then(expr = "a later fetch of the summary returns {string}")]
async fn exchange_then_summary_round_trips(world: &mut Contract, summary: String) {
    let notebook = exchange_notebook(world);
    let fetched = exchange_call(
        world,
        "FetchSummary",
        serde_json::json!({"notebook": notebook}),
    )
    .await;
    assert_eq!(fetched["summary"], serde_json::json!(summary), "{fetched}");
}

#[when(
    expr = "the notebook session fetches the summary, the query and the result for cell {string}"
)]
async fn exchange_when_fetches_all(world: &mut Contract, cell: String) {
    let notebook = exchange_notebook(world);
    let summary = exchange_call(
        world,
        "FetchSummary",
        serde_json::json!({"notebook": notebook}),
    )
    .await;
    let query = exchange_call(
        world,
        "FetchQuery",
        serde_json::json!({"notebook": notebook, "cell": cell}),
    )
    .await;
    let result = exchange_call(
        world,
        "FetchResult",
        serde_json::json!({"notebook": notebook, "cell": cell}),
    )
    .await;
    world.exchange_summary = Some(summary);
    world.exchange_query = Some(query);
    world.exchange_result = Some(result);
}

#[then("what it receives is the summary, the query and the result")]
async fn exchange_then_receives_all(world: &mut Contract) {
    let summary = world.exchange_summary.clone().expect("summary response");
    /* The notebook may carry no summary yet; the fetch still carries the index. */
    assert!(summary["cells"].is_array(), "{summary}");
    let query = world.exchange_query.clone().expect("query response");
    assert!(query["sql"].is_string(), "{query}");
    let result = world.exchange_result.clone().expect("result response");
    assert!(result["columns"].is_array(), "{result}");
}

#[then("none of what it receives carries a conversation transcript")]
async fn exchange_then_no_transcript(world: &mut Contract) {
    for response in [
        world.exchange_summary.clone(),
        world.exchange_query.clone(),
        world.exchange_result.clone(),
    ] {
        let value = response.expect("response");
        assert!(value.get("messages").is_none(), "{value}");
        assert!(value.get("role").is_none(), "{value}");
    }
}

#[when(expr = "a session for {string} fetches the result for cell {string}")]
async fn exchange_when_fetches_for(world: &mut Contract, notebook: String, cell: String) {
    let result = exchange_call(
        world,
        "FetchResult",
        serde_json::json!({"notebook": notebook, "cell": cell}),
    )
    .await;
    world.exchange_result = Some(result);
}

#[then(expr = "it receives the result held by {string}")]
async fn exchange_then_receives_held(world: &mut Contract, notebook: String) {
    let result = world.exchange_result.clone().expect("result response");
    let rows = result["rowsJson"].as_array().cloned().unwrap_or_default();
    let expected = if notebook == exchange_notebook(world) {
        "[1]"
    } else {
        "[9]"
    };
    assert!(rows.contains(&serde_json::json!(expected)), "{result}");
}

#[then(expr = "a fetch for {string} returns the result held by {string}")]
async fn exchange_then_other_holds(world: &mut Contract, notebook: String, held_by: String) {
    let cell = exchange_cell(world);
    let other = exchange_call(
        world,
        "FetchResult",
        serde_json::json!({"notebook": notebook, "cell": cell}),
    )
    .await;
    /* The endpoint has no session binding: the caller names the notebook, and
    each notebook keeps its own recorded result, so asking for the other one
    returns the other one's rows. */
    let expected = if held_by == "ops" { "[9]" } else { "[1]" };
    let rows = other["rowsJson"].as_array().cloned().unwrap_or_default();
    assert!(
        rows.contains(&serde_json::json!(expected)),
        "{notebook} should hold {expected}: {other}"
    );
}

#[path = "support/ai_context_boundary.rs"]
mod ai_context_boundary;

#[then("protected metadata makes zero catalog or helper requests with a separate allowed schema control")]
async fn odcs_denied_metadata(_: &mut Contract) {
    ai_context_boundary::denied_metadata().await;
}

#[then("mixed metadata sends the allowed schema but never reads unbound catalogs or discloses unadmitted contracts")]
async fn odcs_mixed_metadata(_: &mut Contract) {
    ai_context_boundary::mixed_metadata().await;
}

#[then(
    "two teams and two sessions and a personal workspace send only their admitted notebook index"
)]
async fn odcs_workspace_index(_: &mut Contract) {
    ai_context_boundary::workspace_index().await;
}

#[path = "support/odcs_intake.rs"]
mod odcs_intake;

#[then("compiled artifact preservation and manifest selection pass without author files")]
async fn s2_compiled(_: &mut Contract) {
    odcs_intake::compiled_bundle_requires_no_author_tree();
    odcs_intake::compiled_bundle_selection_and_pins_are_explicit();
    odcs_intake::intake_preservation_does_not_expand_legacy_disclosure();
}
#[then("all compiled document objects and original identity remain intact")]
async fn s2_preserved(_: &mut Contract) {
    odcs_intake::compiled_bundle_requires_no_author_tree();
}
#[then("invalid v3.2 fails the pinned offline schema")]
async fn s2_invalid(_: &mut Contract) {
    odcs_intake::offline_schema_validation_is_distinct_from_support();
}
#[then("schema-valid v3.1 fails support without conversion")]
async fn s2_version(_: &mut Contract) {
    odcs_intake::schema_valid_v31_is_rejected_without_conversion();
}
#[then("qualified namespaces retain exact provider segments through API transport")]
async fn s2_namespace(_: &mut Contract) {
    odcs_intake::namespace_segments_survive_adapter_boundaries().await;
}

/// Only the features that are bound to this harness run: the repository marks
/// drafts `@unautomated`, so dropping that tag is what wires a contract in.
#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("error")
        .try_init();
    Contract::cucumber()
        // Browser/HTTP fixtures share the host; isolate their wall-clock bounds.
        .max_concurrent_scenarios(1)
        .filter_run_and_exit("features", |feature, _, _| {
            let tagged = |tag: &str| feature.tags.iter().any(|value| value == tag);
            tagged("contract") && !tagged("unautomated")
        })
        .await;
}
