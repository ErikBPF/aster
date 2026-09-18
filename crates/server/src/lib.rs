//! The aster server: the HTTP/HTML surface, the multiprotocol RPC surface, the
//! git-backed notebook store and the shared-state adapters.
//!
//! `main.rs` is only the composition entry point; everything testable lives
//! here so integration tests can build a state with in-memory stores and drive
//! the router in process.

use std::sync::Arc;
use std::time::Instant;

use aster_core::{
    authorize, authorize_engine, AppConfig, AuditEvent, AuditSink, CatalogId, CatalogRegistry,
    CoreError, DataContract, EngineId, EngineRegistry, Grants, HandshakeStore, Health,
    IdentityProvider, LlmStore, Notebook, NotebookStore, Principal, QueryRequest, Role,
    SecretStore, SessionRegistry, UserState, WorkingState,
};

mod ai;
mod api;
mod contracts;
mod gitstore;
mod identity;
mod providers;
mod state;
mod store;
mod telemetry;
mod web;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use identity::OidcConfig;
use serde::{Deserialize, Serialize};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;

/// Also exported for integration tests, which build a state with a temporary
/// checkout.
pub use gitstore::GitNotebookStore;
pub use telemetry::{metrics_router, Metrics};

/// Everything a request needs. Public so integration tests can build one with
/// in-memory stores and a stub engine.
pub struct AppState {
    pub config: AppConfig,
    pub engines: EngineRegistry,
    pub catalogs: CatalogRegistry,
    pub grants: Arc<dyn Grants>,
    pub audit: Arc<dyn AuditSink>,
    pub notebooks: Arc<dyn NotebookStore>,
    pub llm: Arc<dyn LlmStore>,
    /// Contract files, read once at startup; deployment artifacts, not user data.
    pub contracts: Arc<Vec<DataContract>>,
    pub http: reqwest::Client,
    pub sessions: Arc<dyn SessionRegistry>,
    /// OIDC handshake payloads, redeemed once per login (D20).
    pub handshakes: Arc<dyn HandshakeStore>,
    /// Idle timeout of a session, echoed as the session cookie's Max-Age.
    pub session_ttl_seconds: i64,
    /// Where each subject left off, shared across containers (D18).
    pub user_state: Arc<dyn UserState>,
    /// Secret resolution: environment today, Vault/OpenBao or a file next.
    pub secrets: Arc<dyn SecretStore>,
    /// The identity provider, when one is configured.
    pub identity: Option<Arc<dyn IdentityProvider>>,
    /// Accepts the pre-SSO header/cookie identity seam. Off whenever an
    /// identity provider is configured, unless explicitly forced for local
    /// development.
    pub dev_login: bool,
    /// Request counters and latency, rendered by the metrics listener.
    pub metrics: Arc<Metrics>,
}

async fn build_state(config: AppConfig) -> anyhow::Result<Arc<AppState>> {
    let mut engines = EngineRegistry::new();
    for engine_config in &config.engines {
        engines.register(aster_engines::engine_from_config(engine_config)?);
    }

    let mut catalogs = CatalogRegistry::new();
    for catalog_config in &config.catalogs {
        catalogs.register(aster_catalogs::catalog_from_config(catalog_config)?);
    }

    let seeds = parse_grants(&std::env::var("ASTER_GRANTS").unwrap_or_default());

    // Where a secret lives is a provider decision; the process asks for a key.
    let secrets = providers::secret_store(&providers::kind("ASTER_SECRET_STORE", "env"))?;
    let database_url = secrets.get("DATABASE_URL").await?;
    let state_url = secrets.get("ASTER_STATE_URL").await?;

    let metadata_kind = providers::kind(
        "ASTER_METADATA_STORE",
        if database_url.is_some() {
            "postgres"
        } else {
            "memory"
        },
    );
    let metadata = providers::metadata(&metadata_kind, database_url.as_deref(), &seeds).await?;

    let notebook_dir =
        std::env::var("ASTER_NOTEBOOK_DIR").unwrap_or_else(|_| "data/notebooks".into());
    let notebook_branch =
        std::env::var("ASTER_NOTEBOOK_BRANCH").unwrap_or_else(|_| "session".into());
    let notebooks = providers::notebooks(
        &providers::kind("ASTER_NOTEBOOK_STORE", "git"),
        notebook_dir,
        notebook_branch,
    )?;

    // Sessions are opaque ids resolved in a shared store (D20), so a session
    // created by one container is accepted by the next and survives a restart.
    let session_ttl = env_seconds("ASTER_SESSION_TTL_SECONDS", 28_800);
    let handshake_ttl = env_seconds("ASTER_HANDSHAKE_TTL_SECONDS", 300);
    let user_ttl = env_seconds("ASTER_USER_STATE_TTL_SECONDS", 2_592_000);
    let state_kind = providers::kind(
        "ASTER_STATE_STORE",
        if state_url.is_some() {
            "valkey"
        } else {
            "memory"
        },
    );
    let stores = providers::state(
        &state_kind,
        state_url.as_deref(),
        session_ttl,
        handshake_ttl,
        user_ttl,
    )
    .await?;
    tracing::info!(metadata = %metadata_kind, state = %state_kind, "providers selected");

    let contract_dir = std::env::var("ASTER_CONTRACTS_DIR").unwrap_or_else(|_| "contracts".into());
    let contracts = contracts::load(std::path::Path::new(&contract_dir));
    tracing::info!(
        "loaded {} data contract(s) from {contract_dir}",
        contracts.len()
    );

    let identity_kind = providers::kind(
        "ASTER_IDP_KIND",
        if std::env::var("ASTER_OIDC_ISSUER").is_ok() {
            "oidc"
        } else {
            "none"
        },
    );
    let identity = providers::identity(&identity_kind, OidcConfig::from_env(&*secrets).await)?;
    let dev_login = identity.is_none() || std::env::var("ASTER_DEV_LOGIN").is_ok();
    tracing::info!(identity = %identity_kind, "identity provider selected");
    if identity.is_none() {
        tracing::warn!("ASTER_OIDC_ISSUER unset; using the header/cookie dev identity seam");
    } else if dev_login {
        tracing::warn!("ASTER_DEV_LOGIN set; the dev identity seam is accepted alongside SSO");
    }

    let metrics = Arc::new(Metrics::new());
    Ok(Arc::new(AppState {
        config,
        engines,
        catalogs,
        grants: metadata.grants,
        audit: metadata.audit,
        notebooks,
        llm: metadata.llm,
        contracts: Arc::new(contracts),
        // ponytail: no redirects on the outbound client; add per-host allowlisting
        // if users ever register endpoints outside the lab.
        http: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(60))
            .build()?,
        sessions: stores.sessions,
        handshakes: stores.handshakes,
        session_ttl_seconds: session_ttl,
        user_state: stores.user_state,
        secrets,
        identity,
        dev_login,
        metrics,
    }))
}

/// Seconds from the environment, falling back on anything unparseable or
/// non-positive: a negative TTL would cast to a gigantic expiry in the store.
fn env_seconds(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// Parse `subject:engine,subject:engine` dev seeding.
fn parse_grants(spec: &str) -> Vec<(String, String)> {
    spec.split(',')
        .filter_map(|entry| entry.trim().split_once(':'))
        .map(|(subject, engine)| (subject.trim().to_string(), engine.trim().to_string()))
        .filter(|(subject, engine)| !subject.is_empty() && !engine.is_empty())
        .collect()
}

#[derive(Debug)]
struct ApiError(CoreError);

impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            CoreError::Unauthorized(_) => StatusCode::FORBIDDEN,
            CoreError::NotFound(_) => StatusCode::NOT_FOUND,
            CoreError::Invalid(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::BAD_GATEWAY,
        };
        // 5xx details can carry git stderr or upstream internals: log them and
        // hand the client a generic message.
        let message = match &self.0 {
            CoreError::Unauthorized(message)
            | CoreError::NotFound(message)
            | CoreError::Invalid(message) => message.clone(),
            other => {
                tracing::error!(error = %other, "request failed");
                "upstream dependency failed".to_string()
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

#[derive(Serialize)]
struct EngineSummary {
    id: EngineId,
    kind: String,
    endpoint: String,
    routing_group: Option<String>,
    health: Health,
}

#[derive(Serialize)]
struct CatalogSummary {
    id: CatalogId,
    kind: String,
    health: Health,
}

#[derive(Deserialize)]
pub(crate) struct QueryBody {
    sql: String,
    engine: Option<String>,
    catalog: Option<String>,
    schema: Option<String>,
    max_rows: Option<usize>,
}

/// Runs one query attempt for an already identified caller: role check, engine
/// resolution, engine grant, execution, and exactly one audit row. Both the REST
/// route and the RPC leg call this, so the trail is the same whichever protocol
/// the caller used — and a refusal is recorded too, with `ok: false`.
pub(crate) async fn execute_query(
    state: &AppState,
    principal: &Principal,
    body: &QueryBody,
) -> Result<aster_core::QueryResult, CoreError> {
    let engine_id = body
        .engine
        .clone()
        .or_else(|| state.config.default_engine.clone())
        .map(EngineId::new);

    let started = Instant::now();
    let outcome = match authorize(principal, aster_core::Action::RunQuery) {
        Ok(()) => run_attempt(state, principal, body, engine_id.as_ref()).await,
        Err(error) => Err(error),
    };
    let latency_ms = started.elapsed().as_millis() as u64;

    let (ok, row_count) = match &outcome {
        Ok(result) => (true, result.rows.len()),
        Err(_) => (false, 0),
    };
    let event = AuditEvent {
        subject: principal.subject.clone(),
        // A refusal before routing has no engine: name that in the trail instead
        // of dropping the attempt.
        engine: engine_id.unwrap_or_else(|| EngineId::new("(unrouted)")),
        catalog: body.catalog.clone(),
        schema: body.schema.clone(),
        sql: body.sql.clone(),
        latency_ms,
        row_count,
        ok,
    };
    if let Err(error) = state.audit.record(&event).await {
        tracing::error!(%error, "failed to record audit event");
    }

    outcome
}

async fn run_attempt(
    state: &AppState,
    principal: &Principal,
    body: &QueryBody,
    engine_id: Option<&EngineId>,
) -> Result<aster_core::QueryResult, CoreError> {
    let engine_id =
        engine_id.ok_or_else(|| CoreError::Invalid("no engine selected and no default".into()))?;
    let engine = state
        .engines
        .get(engine_id)
        .ok_or_else(|| CoreError::NotFound("unknown engine".into()))?;
    authorize_engine(state.grants.as_ref(), &principal.subject, engine_id).await?;
    engine
        .execute(QueryRequest {
            sql: body.sql.clone(),
            catalog: body.catalog.clone(),
            schema: body.schema.clone(),
            max_rows: body.max_rows,
        })
        .await
}

/// Resolve the caller: an SSO session cookie first, then the pre-SSO dev seam
/// (headers or `aster_subject`/`aster_roles` cookies) when dev login is enabled.
pub(crate) async fn principal(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, ApiError> {
    if let Some(sid) = cookie(headers, "aster_session") {
        match state.sessions.get(&sid, now()).await {
            Ok(Some(record)) => {
                return Ok(Principal {
                    subject: record.subject,
                    roles: record.roles,
                })
            }
            // Fail closed (D20): an unknown, revoked or expired session is not a
            // signed-in caller, and a store failure must not fall through to the
            // dev seam.
            Ok(None) => return Err(CoreError::Unauthorized("session expired".into()).into()),
            Err(error) => {
                tracing::error!(%error, "session store unavailable");
                return Err(CoreError::Unauthorized("session store unavailable".into()).into());
            }
        }
    }

    if !state.dev_login {
        return Err(CoreError::Unauthorized("sign in required".into()).into());
    }

    let subject = headers
        .get("x-aster-subject")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .or_else(|| cookie(headers, "aster_subject"))
        .ok_or_else(|| CoreError::Unauthorized("missing x-aster-subject".into()))?;

    let roles = headers
        .get("x-aster-roles")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .or_else(|| cookie(headers, "aster_roles"))
        .map(|value| value.split(',').filter_map(parse_role).collect())
        .unwrap_or_else(|| vec![Role::Editor]);

    Ok(Principal { subject, roles })
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

pub(crate) fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
}

fn parse_role(value: &str) -> Option<Role> {
    match value.trim() {
        "viewer" => Some(Role::Viewer),
        "editor" => Some(Role::Editor),
        "admin" => Some(Role::Admin),
        _ => None,
    }
}

async fn list_audit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<AuditEvent>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::Administer)?;
    Ok(Json(state.audit.events().await?))
}

async fn healthz() -> &'static str {
    "ok"
}

/// Where this subject left off. Any signed-in user may keep their own place;
/// the subject always comes from the session, never from the body.
async fn get_state(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<WorkingState>, ApiError> {
    let principal = principal(&state, &headers).await?;
    let state = state.user_state.get(&principal.subject).await?;
    Ok(Json(state.unwrap_or_default()))
}

async fn put_state(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<WorkingState>,
) -> Result<Json<WorkingState>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    state.user_state.put(&principal.subject, &body).await?;
    Ok(Json(body))
}

async fn list_engines(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<EngineSummary>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let mut summaries = Vec::new();
    for engine in state.engines.list() {
        let info = engine.info().clone();
        summaries.push(EngineSummary {
            id: info.id,
            kind: info.kind,
            endpoint: info.endpoint,
            routing_group: info.routing_group,
            health: engine.health().await,
        });
    }
    Ok(Json(summaries))
}

async fn list_catalogs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<CatalogSummary>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let mut summaries = Vec::new();
    for catalog in state.catalogs.list() {
        summaries.push(CatalogSummary {
            id: catalog.id().clone(),
            kind: catalog.kind().to_string(),
            health: catalog.health().await,
        });
    }
    Ok(Json(summaries))
}

async fn list_contracts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<DataContract>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    Ok(Json(state.contracts.as_ref().clone()))
}

async fn run_query(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<QueryBody>,
) -> Result<Json<aster_core::QueryResult>, ApiError> {
    let principal = principal(&state, &headers).await?;
    Ok(Json(execute_query(&state, &principal, &body).await?))
}

async fn list_namespaces(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<aster_core::Namespace>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let catalog = state
        .catalogs
        .get(&CatalogId::new(id))
        .ok_or_else(|| CoreError::NotFound("unknown catalog".into()))?;
    Ok(Json(catalog.list_namespaces().await?))
}

async fn list_tables(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((id, namespace)): Path<(String, String)>,
) -> Result<Json<Vec<aster_core::TableRef>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let catalog = state
        .catalogs
        .get(&CatalogId::new(id))
        .ok_or_else(|| CoreError::NotFound("unknown catalog".into()))?;
    Ok(Json(catalog.list_tables(&namespace).await?))
}

async fn list_notebooks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<String>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    Ok(Json(state.notebooks.list(&principal.subject).await?))
}

async fn get_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Notebook>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    Ok(Json(state.notebooks.get(&id).await?))
}

async fn save_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut notebook): Json<Notebook>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::WriteNotebook)?;
    notebook.id = id;
    let revision = state.notebooks.save(&notebook, &principal.subject).await?;
    Ok(Json(serde_json::json!({ "revision": revision })))
}

pub fn app(state: Arc<AppState>) -> Router {
    // gRPC, Connect and gRPC-Web all arrive on the same router, so the proto in
    // `proto/aster.proto` is the only endpoint declaration. Unknown paths still
    // fall through to a 404 from the Connect router.
    let rpc = api::router(Arc::clone(&state)).into_axum_service();
    Router::new()
        .route("/", get(web::index))
        .route("/dev-login", get(web::dev_login))
        .route("/login", get(web::login))
        .route("/callback", get(web::callback))
        .route("/logout", get(web::logout))
        .route("/notebooks/{id}", get(web::notebook_view))
        .route("/catalog", get(web::catalog))
        .route("/contracts", get(web::contracts))
        .route(
            "/settings/llm",
            get(web::llm_settings).post(ai::put_config_form),
        )
        .route("/api/llm", get(ai::get_config).put(ai::put_config))
        .route("/api/ai", post(ai::generate))
        .route("/catalog/{id}/{namespace}", get(web::catalog_namespace))
        .route("/catalog/{id}/{namespace}/{table}", get(web::catalog_table))
        .route("/healthz", get(healthz))
        .route("/api/engines", get(list_engines))
        .route("/api/catalogs", get(list_catalogs))
        .route("/api/contracts", get(list_contracts))
        .route("/api/query", post(run_query))
        .route("/api/audit", get(list_audit))
        .route("/api/state", get(get_state).put(put_state))
        .route("/api/notebooks", get(list_notebooks))
        .route("/api/notebooks/{id}", get(get_notebook).put(save_notebook))
        .route("/api/catalogs/{id}/namespaces", get(list_namespaces))
        .route(
            "/api/catalogs/{id}/namespaces/{namespace}/tables",
            get(list_tables),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(telemetry::http_span)
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .fallback_service(rpc)
        .with_state(state)
}

/// The public router with the request recorder attached. `run` and the tests
/// share it, so a scenario counts the same requests production counts.
pub fn app_recorded(state: Arc<AppState>) -> Router {
    app(Arc::clone(&state)).layer(axum::middleware::from_fn_with_state(
        Arc::clone(&state),
        telemetry::record,
    ))
}

/// Reads configuration from the environment, builds the state and serves until
/// SIGTERM or ctrl-c.
pub async fn run() -> anyhow::Result<()> {
    let telemetry = telemetry::Telemetry::init("aster-server");

    let config = AppConfig::from_env();
    let bind = config.bind.clone();
    let state = build_state(config).await?;
    let app = app_recorded(Arc::clone(&state));

    // Metrics live on their own listener, next to the app rather than inside it.
    let metrics_bind =
        std::env::var("ASTER_METRICS_BIND").unwrap_or_else(|_| "0.0.0.0:9090".into());
    let metrics_listener = match metrics_bind.trim() {
        "" => None,
        _ => Some(tokio::net::TcpListener::bind(&metrics_bind).await?),
    };

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("aster-server listening on {bind}");
    match metrics_listener {
        Some(metrics_listener) => {
            tracing::info!("metrics listening on {metrics_bind}");
            let metrics_app = telemetry::metrics_router(Arc::clone(&state));
            tokio::select! {
                result = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()) => result?,
                result = axum::serve(metrics_listener, metrics_app).with_graceful_shutdown(shutdown_signal()) => result?,
            }
        }
        None => {
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await?;
        }
    }

    telemetry.shutdown();
    Ok(())
}

/// Kubernetes sends SIGTERM before killing a pod; drain instead of dying.
async fn shutdown_signal() {
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("ctrl-c, shutting down"),
        _ = terminate => tracing::info!("SIGTERM, shutting down"),
    }
}
