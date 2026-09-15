use std::sync::Arc;
use std::time::Instant;

use aster_core::{
    authorize, authorize_engine, AppConfig, AuditEvent, AuditSink, CatalogHealth, CatalogId,
    CatalogRegistry, CoreError, EngineHealth, EngineId, EngineRegistry, Grants, InMemoryAudit,
    InMemoryGrants, Notebook, NotebookStore, Principal, QueryRequest, Role,
};

mod gitstore;
mod store;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use gitstore::GitNotebookStore;
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
    let (grants, audit): (Arc<dyn Grants>, Arc<dyn AuditSink>) = match std::env::var("DATABASE_URL")
    {
        Ok(url) => {
            let store = Arc::new(PgStore::connect(&url).await?);
            for (subject, engine) in &seeds {
                store.seed_grant(subject, engine).await?;
            }
            (store.clone(), store)
        }
        Err(_) => {
            tracing::warn!("DATABASE_URL unset; grants and audit are in-memory only");
            let grants = InMemoryGrants::new();
            for (subject, engine) in seeds {
                grants.grant(subject, engine);
            }
            (Arc::new(grants), Arc::new(InMemoryAudit::new()))
        }
    };

    let notebook_dir =
        std::env::var("ASTER_NOTEBOOK_DIR").unwrap_or_else(|_| "data/notebooks".into());
    let notebook_branch =
        std::env::var("ASTER_NOTEBOOK_BRANCH").unwrap_or_else(|_| "session".into());
    let notebooks: Arc<dyn NotebookStore> =
        Arc::new(GitNotebookStore::open(notebook_dir, notebook_branch)?);

    Ok(Arc::new(AppState {
        config,
        engines,
        catalogs,
        grants,
        audit,
        notebooks,
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
        (
            status,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct EngineSummary {
    id: EngineId,
    kind: String,
    endpoint: String,
    health: EngineHealth,
}

#[derive(Serialize)]
struct CatalogSummary {
    id: CatalogId,
    kind: String,
    health: CatalogHealth,
}

#[derive(Deserialize)]
struct QueryBody {
    sql: String,
    engine: Option<String>,
    catalog: Option<String>,
    schema: Option<String>,
    max_rows: Option<usize>,
}

/// Temporary identity seam for the pre-SSO scaffold: identity arrives as
/// headers. ponytail: replace with the OIDC session extractor in S2 — header
/// trust is dev-only and must never reach production.
fn principal_from_headers(headers: &HeaderMap) -> Result<Principal, ApiError> {
    let subject = headers
        .get("x-aster-subject")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| CoreError::Unauthorized("missing x-aster-subject".into()))?;

    let roles = headers
        .get("x-aster-roles")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(',').filter_map(parse_role).collect())
        .unwrap_or_else(|| vec![Role::Editor]);

    Ok(Principal {
        subject: subject.to_string(),
        roles,
    })
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
    let principal = principal_from_headers(&headers)?;
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

async fn run_query(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<QueryBody>,
) -> Result<Json<aster_core::QueryResult>, ApiError> {
    let principal = principal_from_headers(&headers)?;
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
    let principal = principal_from_headers(&headers)?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    Ok(Json(state.notebooks.list(&principal.subject).await?))
}

async fn get_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Notebook>, ApiError> {
    let principal = principal_from_headers(&headers)?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    Ok(Json(state.notebooks.get(&id).await?))
}

async fn save_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut notebook): Json<Notebook>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = principal_from_headers(&headers)?;
    authorize(&principal, aster_core::Action::WriteNotebook)?;
    notebook.id = id;
    let revision = state.notebooks.save(&notebook, &principal.subject).await?;
    Ok(Json(serde_json::json!({ "revision": revision })))
}

fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/engines", get(list_engines))
        .route("/api/catalogs", get(list_catalogs))
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
    axum::serve(listener, app).await?;
    Ok(())
}
