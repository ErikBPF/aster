use std::sync::Arc;
use std::time::Instant;

use aster_core::{
    authorize, authorize_engine, AppConfig, AuditEvent, AuditSink, CatalogId, CatalogRegistry,
    CoreError, DataContract, EngineId, EngineRegistry, Grants, Health, InMemoryAudit,
    InMemoryGrants, InMemoryLlm, LlmStore, Notebook, NotebookStore, Principal, QueryRequest, Role,
    SessionStore,
};

mod ai;
mod contracts;
mod gitstore;
mod oidc;
mod store;
mod web;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use gitstore::GitNotebookStore;
use oidc::{Oidc, OidcConfig};
use serde::{Deserialize, Serialize};
use store::PgStore;
use tower_http::trace::TraceLayer;

struct AppState {
    config: AppConfig,
    engines: EngineRegistry,
    catalogs: CatalogRegistry,
    grants: Arc<dyn Grants>,
    audit: Arc<dyn AuditSink>,
    notebooks: Arc<dyn NotebookStore>,
    llm: Arc<dyn LlmStore>,
    /// Contract files, read once at startup; deployment artifacts, not user data.
    contracts: Arc<Vec<DataContract>>,
    http: reqwest::Client,
    sessions: Arc<SessionStore>,
    oidc: Option<Arc<Oidc>>,
    /// Accepts the pre-SSO header/cookie identity seam. Off whenever OIDC is
    /// configured, unless explicitly forced for local development.
    dev_login: bool,
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
    let (grants, audit, llm): (Arc<dyn Grants>, Arc<dyn AuditSink>, Arc<dyn LlmStore>) =
        match std::env::var("DATABASE_URL") {
            Ok(url) => {
                let store = Arc::new(PgStore::connect(&url).await?);
                for (subject, engine) in &seeds {
                    store.seed_grant(subject, engine).await?;
                }
                (store.clone(), store.clone(), store)
            }
            Err(_) => {
                tracing::warn!(
                    "DATABASE_URL unset; grants, audit and llm configs are in-memory only"
                );
                let grants = InMemoryGrants::new();
                for (subject, engine) in seeds {
                    grants.grant(subject, engine);
                }
                (
                    Arc::new(grants),
                    Arc::new(InMemoryAudit::new()),
                    Arc::new(InMemoryLlm::new()),
                )
            }
        };

    let notebook_dir =
        std::env::var("ASTER_NOTEBOOK_DIR").unwrap_or_else(|_| "data/notebooks".into());
    let notebook_branch =
        std::env::var("ASTER_NOTEBOOK_BRANCH").unwrap_or_else(|_| "session".into());
    let notebooks: Arc<dyn NotebookStore> =
        Arc::new(GitNotebookStore::open(notebook_dir, notebook_branch)?);

    // ponytail: a fixed development key keeps local runs working; a deployment
    // must set ASTER_SESSION_KEY, which also invalidates sessions on rotation.
    let session_key = match std::env::var("ASTER_SESSION_KEY") {
        Ok(key) => key,
        Err(_) => {
            tracing::warn!("ASTER_SESSION_KEY unset; using the insecure development key");
            "aster-development-key".into()
        }
    };
    let sessions = Arc::new(SessionStore::new(session_key));

    let contract_dir = std::env::var("ASTER_CONTRACTS_DIR").unwrap_or_else(|_| "contracts".into());
    let contracts = contracts::load(std::path::Path::new(&contract_dir));
    tracing::info!(
        "loaded {} data contract(s) from {contract_dir}",
        contracts.len()
    );

    let oidc = OidcConfig::from_env().map(|config| Arc::new(Oidc::new(config)));
    let dev_login = oidc.is_none() || std::env::var("ASTER_DEV_LOGIN").is_ok();
    if oidc.is_none() {
        tracing::warn!("ASTER_OIDC_ISSUER unset; using the header/cookie dev identity seam");
    } else if dev_login {
        tracing::warn!("ASTER_DEV_LOGIN set; the dev identity seam is accepted alongside SSO");
    }

    Ok(Arc::new(AppState {
        config,
        engines,
        catalogs,
        grants,
        audit,
        notebooks,
        llm,
        contracts: Arc::new(contracts),
        // ponytail: no redirects on the outbound client; add per-host allowlisting
        // if users ever register endpoints outside the lab.
        http: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(60))
            .build()?,
        sessions,
        oidc,
        dev_login,
    }))
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
struct QueryBody {
    sql: String,
    engine: Option<String>,
    catalog: Option<String>,
    schema: Option<String>,
    max_rows: Option<usize>,
}

/// Resolve the caller: an SSO session cookie first, then the pre-SSO dev seam
/// (headers or `aster_subject`/`aster_roles` cookies) when dev login is enabled.
pub(crate) fn principal(state: &AppState, headers: &HeaderMap) -> Result<Principal, ApiError> {
    if let Some(token) = cookie(headers, "aster_session") {
        if let Some(session) = state.sessions.decode(&token, now()) {
            return Ok(Principal {
                subject: session.subject,
                roles: session.roles,
            });
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
    let principal = principal(&state, &headers)?;
    authorize(&principal, aster_core::Action::Administer)?;
    Ok(Json(state.audit.events().await?))
}

async fn healthz() -> &'static str {
    "ok"
}

async fn list_engines(State(state): State<Arc<AppState>>) -> Json<Vec<EngineSummary>> {
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
    Json(summaries)
}

async fn list_catalogs(State(state): State<Arc<AppState>>) -> Json<Vec<CatalogSummary>> {
    let mut summaries = Vec::new();
    for catalog in state.catalogs.list() {
        summaries.push(CatalogSummary {
            id: catalog.id().clone(),
            kind: catalog.kind().to_string(),
            health: catalog.health().await,
        });
    }
    Json(summaries)
}

async fn list_contracts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<DataContract>>, ApiError> {
    let principal = principal(&state, &headers)?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    Ok(Json(state.contracts.as_ref().clone()))
}

async fn run_query(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<QueryBody>,
) -> Result<Json<aster_core::QueryResult>, ApiError> {
    let principal = principal(&state, &headers)?;
    authorize(&principal, aster_core::Action::RunQuery)?;

    let engine_id = body
        .engine
        .clone()
        .or_else(|| state.config.default_engine.clone())
        .ok_or_else(|| CoreError::Invalid("no engine selected and no default".into()))?;
    let engine_id = EngineId::new(engine_id);

    let engine = state
        .engines
        .get(&engine_id)
        .ok_or_else(|| CoreError::NotFound("unknown engine".into()))?;

    authorize_engine(state.grants.as_ref(), &principal.subject, &engine_id).await?;

    let started = Instant::now();
    let outcome = engine
        .execute(QueryRequest {
            sql: body.sql.clone(),
            catalog: body.catalog.clone(),
            schema: body.schema.clone(),
            max_rows: body.max_rows,
        })
        .await;
    let latency_ms = started.elapsed().as_millis() as u64;

    let (ok, row_count) = match &outcome {
        Ok(result) => (true, result.rows.len()),
        Err(_) => (false, 0),
    };
    if let Err(error) = state
        .audit
        .record(&AuditEvent {
            subject: principal.subject,
            engine: engine_id,
            catalog: body.catalog,
            schema: body.schema,
            sql: body.sql,
            latency_ms,
            row_count,
            ok,
        })
        .await
    {
        tracing::error!(%error, "failed to record audit event");
    }

    Ok(Json(outcome?))
}

async fn list_namespaces(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<aster_core::Namespace>>, ApiError> {
    let catalog = state
        .catalogs
        .get(&CatalogId::new(id))
        .ok_or_else(|| CoreError::NotFound("unknown catalog".into()))?;
    Ok(Json(catalog.list_namespaces().await?))
}

async fn list_tables(
    State(state): State<Arc<AppState>>,
    Path((id, namespace)): Path<(String, String)>,
) -> Result<Json<Vec<aster_core::TableRef>>, ApiError> {
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
    let principal = principal(&state, &headers)?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    Ok(Json(state.notebooks.list(&principal.subject).await?))
}

async fn get_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Notebook>, ApiError> {
    let principal = principal(&state, &headers)?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    Ok(Json(state.notebooks.get(&id).await?))
}

async fn save_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut notebook): Json<Notebook>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = principal(&state, &headers)?;
    authorize(&principal, aster_core::Action::WriteNotebook)?;
    notebook.id = id;
    let revision = state.notebooks.save(&notebook, &principal.subject).await?;
    Ok(Json(serde_json::json!({ "revision": revision })))
}

fn app(state: Arc<AppState>) -> Router {
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
        .route("/api/notebooks", get(list_notebooks))
        .route("/api/notebooks/{id}", get(get_notebook).put(save_notebook))
        .route("/api/catalogs/{id}/namespaces", get(list_namespaces))
        .route(
            "/api/catalogs/{id}/namespaces/{namespace}/tables",
            get(list_tables),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let config = AppConfig::from_env();
    let bind = config.bind.clone();
    let state = build_state(config).await?;
    let app = app(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!("aster-server listening on {bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
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
