//! The aster server: the HTTP/HTML surface, the multiprotocol RPC surface, the
//! git-backed notebook store and the shared-state adapters.
//!
//! `main.rs` is only the composition entry point; everything testable lives
//! here so integration tests can build a state with in-memory stores and drive
//! the router in process.

use std::io::Read as _;
use std::sync::Arc;
use std::time::Instant;

use aster_core::{
    authorize, authorize_engine, AppConfig, AuditEvent, AuditSink, CatalogId, CatalogRegistry,
    CoreError, DataContract, EngineId, EngineRegistry, Grants, HandshakeStore, Health,
    IdentityProvider, LlmStore, Notebook, NotebookOwnerChange, NotebookOwners,
    NotebookPrecondition, NotebookSave, NotebookSnapshot, NotebookStore, Principal, QueryRequest,
    Role, SecretStore, SessionRegistry, UserState, WorkingState,
};

mod ai;
mod api;
mod contracts;
mod conversations;
mod current_identity;
mod exchange;
mod github_app;
mod gitstore;
mod identity;
mod notebook_workspaces;
mod providers;
mod shared_models;
mod shared_models_pg;
mod state;
mod store;
mod team_git;
mod telemetry;
mod web;

use axum::{
    extract::{Path, Query, State},
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
pub use gitstore::{GitNotebookStore, NotebookWorkspaceBinding};
pub use notebook_workspaces::{TeamPolicy, TeamTarget, TeamWorkspaces};
pub use shared_models::{DestinationPolicy, Keyring, SharedModels};
pub use team_git::{TeamGitPolicy, TeamGitTargetView, TeamGitTargets, TeamGitVerifier};
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
    pub notebook_owners: Arc<dyn NotebookOwners>,
    pub notebook_write: Arc<tokio::sync::Mutex<()>>,
    /// Opt-in team workspaces; absent during the V2a legacy migration.
    pub team_workspaces: Option<Arc<TeamWorkspaces>>,
    /// Verified team destination metadata. No workspace or Sync activation yet.
    pub team_git_targets: Option<Arc<TeamGitTargets>>,
    pub llm: Arc<dyn LlmStore>,
    /// Disabled until the administrator supplies an approved destination and
    /// versioned encryption key. Shared use is a later authorization slice.
    pub shared_models: Option<Arc<SharedModels>>,
    /// Live current membership authority for shared-model use and management.
    /// Absent unless the explicit production opt-in is fully configured.
    pub current_identity: Option<Arc<dyn aster_core::CurrentIdentityProvider>>,
    /// Separate ordinary-use switch; administrators can pre-register and grant
    /// with fresh authority while this remains false.
    pub shared_model_use_enabled: bool,
    pub conversations: Arc<dyn aster_core::ConversationStore>,
    /// The notebook session exchange: one summary and each cell's last result.
    pub exchanges: Arc<dyn aster_core::ExchangeStore>,
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
    config.validate_catalog_bindings()?;
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

    let shared_models = match std::env::var("ASTER_SHARED_MODELS_ENABLED")
        .unwrap_or_else(|_| "0".into())
        .as_str()
    {
        "0" => None,
        "1" => {
            let active = std::env::var("ASTER_SHARED_MODEL_ACTIVE_KEY")?;
            let keyset = aster_core::require(&*secrets, "ASTER_SHARED_MODEL_KEYS").await?;
            let origins = std::env::var("ASTER_SHARED_MODEL_ALLOWED_ORIGINS").unwrap_or_default();
            let registry = SharedModels::new(
                metadata.shared_models.clone(),
                DestinationPolicy::parse(&origins)?,
                Keyring::parse(&active, &keyset)?,
            );
            registry.verify_keys().await?;
            Some(Arc::new(registry))
        }
        _ => anyhow::bail!("ASTER_SHARED_MODELS_ENABLED must be 0 or 1"),
    };

    let conversation_url = secrets.get("ASTER_CONVERSATION_DATABASE_URL").await?;
    let conversation_store = providers::conversations(
        &providers::kind(
            "ASTER_CONVERSATION_STORE",
            if conversation_url.is_some() {
                "postgres"
            } else {
                "metadata"
            },
        ),
        conversation_url.as_deref(),
        metadata.conversations.clone(),
    )
    .await?;
    let exchange_url = secrets.get("ASTER_EXCHANGE_DATABASE_URL").await?;
    let exchange_store = providers::exchanges(
        &providers::kind(
            "ASTER_EXCHANGE_STORE",
            if exchange_url.is_some() {
                "postgres"
            } else {
                "memory"
            },
        ),
        exchange_url.as_deref(),
    )
    .await?;

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

    let authority_enabled = match std::env::var("ASTER_SHARED_MODEL_AUTHORITY_ENABLED")
        .unwrap_or_else(|_| "0".into())
        .as_str()
    {
        "0" => false,
        "1" => true,
        _ => anyhow::bail!("ASTER_SHARED_MODEL_AUTHORITY_ENABLED must be 0 or 1"),
    };
    let team_git_enabled = match std::env::var("ASTER_TEAM_GIT_ENABLED")
        .unwrap_or_else(|_| "0".into())
        .as_str()
    {
        "0" => false,
        "1" => true,
        _ => anyhow::bail!("ASTER_TEAM_GIT_ENABLED must be 0 or 1"),
    };
    let shared_model_use_enabled = match std::env::var("ASTER_SHARED_MODEL_USE_ENABLED")
        .unwrap_or_else(|_| "0".into())
        .as_str()
    {
        "0" => false,
        "1" => true,
        _ => anyhow::bail!("ASTER_SHARED_MODEL_USE_ENABLED must be 0 or 1"),
    };
    if shared_model_use_enabled && !authority_enabled {
        anyhow::bail!("shared-model use requires current identity authority");
    }
    let current_identity: Option<Arc<dyn aster_core::CurrentIdentityProvider>> =
        match authority_enabled || team_git_enabled {
            false => None,
            true => {
                if authority_enabled && shared_models.is_none() {
                    anyhow::bail!("shared-model authority needs a registry");
                }
                if identity_kind != "oidc"
                    || identity.is_none()
                    || std::env::var("ASTER_IDP_USER_UUID_CLAIM")
                        .ok()
                        .is_none_or(|claim| claim.trim().is_empty())
                {
                    anyhow::bail!(
                        "team Git/shared-model authority needs OIDC and a signed user UUID claim"
                    );
                }
                let required = |name| {
                    std::env::var(name)
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            anyhow::anyhow!("{name} is required for shared-model authority")
                        })
                };
                let origin = required("ASTER_AUTHENTIK_API_ORIGIN")?;
                let token = aster_core::require(&*secrets, "ASTER_AUTHENTIK_READ_TOKEN").await?;
                let reader = if authority_enabled {
                    let admin_group = required("ASTER_AUTHENTIK_ADMIN_GROUP_UUID")?;
                    let editor_group = required("ASTER_AUTHENTIK_EDITOR_GROUP_UUID")?;
                    current_identity::AuthentikCurrentIdentity::new(
                        &origin,
                        token,
                        &admin_group,
                        &editor_group,
                    )?
                } else {
                    current_identity::AuthentikCurrentIdentity::new_for_team_git(&origin, token)?
                };
                Some(Arc::new(reader))
            }
        };

    let team_git_targets = if team_git_enabled {
        if metadata_kind != "postgres" {
            anyhow::bail!("team Git requires PostgreSQL metadata storage");
        }
        let required = |name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("{name} is required for team Git"))
        };
        let policy_file = required("ASTER_TEAM_GIT_POLICY_FILE")?;
        let mut bytes = Vec::new();
        std::fs::File::open(&policy_file)?
            .take(65_537)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 65_536 {
            anyhow::bail!("team Git policy exceeds 64 KiB");
        }
        let policy: std::collections::HashMap<String, TeamGitPolicy> =
            serde_json::from_slice(&bytes)?;
        let app_id: u64 = required("ASTER_GITHUB_APP_ID")?.parse()?;
        let key_name = required("ASTER_GITHUB_APP_KEY_NAME")?;
        aster_core::require(&*secrets, &key_name).await?;
        let app = Arc::new(github_app::GithubApp::new(
            "https://api.github.com/",
            app_id,
            &key_name,
            secrets.clone(),
        )?);
        Some(Arc::new(
            TeamGitTargets::connect(
                database_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("team Git requires DATABASE_URL"))?,
                policy,
                app,
            )
            .await?,
        ))
    } else {
        None
    };

    let metrics = Arc::new(Metrics::new());
    Ok(Arc::new(AppState {
        config,
        engines,
        catalogs,
        grants: metadata.grants,
        audit: metadata.audit,
        notebooks,
        notebook_owners: metadata.notebook_owners,
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: None,
        team_git_targets,
        llm: metadata.llm,
        shared_models,
        current_identity,
        shared_model_use_enabled,
        conversations: conversation_store,
        exchanges: exchange_store,
        contracts: Arc::new(contracts),
        http: ai::outbound_http_client()?,
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

#[cfg(test)]
mod team_git_boot_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL from notebook-team-target-postgres.sh"]
    async fn team_only_boot_needs_no_shared_model_registry_or_role_groups() {
        let root = tempfile::tempdir().unwrap();
        let policy_path = root.path().join("teams.json");
        std::fs::write(
            &policy_path,
            r#"{"alpha":{"member_group_uuid":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","maintainer_group_uuid":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","installation_id":41,"allowed_repositories":{"example/alpha":73}}}"#,
        )
        .unwrap();
        // This filtered test runs alone in the disposable PostgreSQL runner.
        unsafe {
            std::env::set_var(
                "DATABASE_URL",
                std::env::var("ASTER_TEST_METADATA_URL").unwrap(),
            );
            std::env::set_var("ASTER_TEAM_GIT_ENABLED", "1");
            std::env::set_var("ASTER_TEAM_GIT_POLICY_FILE", &policy_path);
            std::env::set_var(
                "ASTER_OIDC_ISSUER",
                "https://id.example.invalid/application/o/aster/",
            );
            std::env::set_var("ASTER_IDP_USER_UUID_CLAIM", "aster_user_uuid");
            std::env::set_var("ASTER_AUTHENTIK_API_ORIGIN", "https://id.example.invalid/");
            std::env::set_var("ASTER_AUTHENTIK_READ_TOKEN", "disposable-test-token");
            std::env::set_var("ASTER_GITHUB_APP_ID", "7");
            std::env::set_var("ASTER_GITHUB_APP_KEY_NAME", "ASTER_TEST_GITHUB_APP_KEY");
            std::env::set_var("ASTER_TEST_GITHUB_APP_KEY", "disposable-test-key");
            std::env::set_var("ASTER_NOTEBOOK_DIR", root.path().join("notebooks"));
            std::env::set_var("ASTER_SHARED_MODELS_ENABLED", "0");
            std::env::set_var("ASTER_SHARED_MODEL_AUTHORITY_ENABLED", "0");
            std::env::remove_var("ASTER_AUTHENTIK_ADMIN_GROUP_UUID");
            std::env::remove_var("ASTER_AUTHENTIK_EDITOR_GROUP_UUID");
        }
        let state = build_state(AppConfig {
            bind: String::new(),
            engines: vec![],
            catalogs: vec![],
            catalog_bindings: vec![],
            default_engine: None,
            default_catalog: None,
        })
        .await
        .unwrap();
        assert!(state.team_git_targets.is_some());
        assert!(state.current_identity.is_some());
        assert!(state.shared_models.is_none());
    }
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
            CoreError::Invalid(_) | CoreError::Query(_) => StatusCode::BAD_REQUEST,
            CoreError::Conflict(_) => StatusCode::CONFLICT,
            _ => StatusCode::BAD_GATEWAY,
        };
        // 5xx details can carry git stderr or upstream internals: log them and
        // hand the client a generic message. A `Query` error is the engine
        // rejecting the statement, so its own wording is what the caller needs.
        let message = match &self.0 {
            CoreError::Unauthorized(message)
            | CoreError::NotFound(message)
            | CoreError::Conflict(message)
            | CoreError::Invalid(message)
            | CoreError::Query(message) => message.clone(),
            other => {
                tracing::error!(error = %other, "request failed");
                "upstream dependency failed".to_string()
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

impl AppState {
    async fn notebook_snapshot(&self, id: &str) -> aster_core::Result<NotebookSnapshot> {
        self.notebooks.snapshot(id).await
    }

    pub(crate) async fn save_notebook_owned(
        &self,
        notebook: &Notebook,
        actor: &str,
        expected: NotebookPrecondition,
    ) -> aster_core::Result<NotebookSave> {
        if self.team_workspaces.is_some() {
            return Err(CoreError::Unauthorized(
                "legacy notebook writes are disabled while team workspaces are active".into(),
            ));
        }
        let _write = self.notebook_write.lock().await;
        let source = self.notebooks.source_key();
        let record = self.notebook_owners.record(&source, &notebook.id).await?;
        match &expected {
            NotebookPrecondition::Absent if record.is_some() => {
                return Err(CoreError::Conflict(
                    "notebook owner already assigned".into(),
                ));
            }
            NotebookPrecondition::Blob(_)
                if record.as_ref().map(|record| record.owner.as_str()) != Some(actor) =>
            {
                return Err(CoreError::Unauthorized(
                    "notebook is unassigned or owned by another user".into(),
                ));
            }
            NotebookPrecondition::Blob(oid)
                if record
                    .as_ref()
                    .is_some_and(|record| record.content_revision != *oid) =>
            {
                return Err(CoreError::Conflict(
                    "notebook owner revision needs administrator recovery".into(),
                ));
            }
            _ => {}
        }
        let saved = self
            .notebooks
            .save_if(notebook, actor, expected.clone())
            .await?;
        if matches!(expected, NotebookPrecondition::Absent) {
            // A failed claim leaves a committed but unassigned read-only file for
            // administrator recovery. The service lock prevents local races.
            self.notebook_owners
                .change(NotebookOwnerChange {
                    source: source.clone(),
                    id: notebook.id.clone(),
                    expected_owner: None,
                    owner: actor.to_string(),
                    source_blob: saved.content_revision.clone(),
                    actor: actor.to_string(),
                    reason: Some("initial create".into()),
                })
                .await?;
        } else if let NotebookPrecondition::Blob(before) = expected {
            self.notebook_owners
                .advance(
                    &source,
                    &notebook.id,
                    actor,
                    &before,
                    &saved.content_revision,
                )
                .await?;
        }
        Ok(saved)
    }

    async fn assign_notebook_owner(
        &self,
        id: &str,
        actor: &str,
        request: AssignNotebookOwner,
    ) -> aster_core::Result<()> {
        let valid_subject = |subject: &str| {
            !subject.is_empty()
                && subject.len() <= 256
                && subject.trim() == subject
                && !subject.chars().any(char::is_control)
        };
        if !valid_subject(&request.owner)
            || request
                .expected_owner
                .as_deref()
                .is_some_and(|owner| !valid_subject(owner))
        {
            return Err(CoreError::Invalid(
                "owner must be an exact canonical subject".into(),
            ));
        }
        if request
            .reason
            .as_ref()
            .is_some_and(|reason| reason.len() > 1024 || reason.chars().any(char::is_control))
        {
            return Err(CoreError::Invalid("invalid owner correction reason".into()));
        }
        if request.expected_owner.is_some()
            && request
                .reason
                .as_ref()
                .is_none_or(|reason| reason.trim().is_empty())
        {
            return Err(CoreError::Invalid("owner correction needs a reason".into()));
        }
        let _write = self.notebook_write.lock().await;
        let snapshot = self.notebook_snapshot(id).await?;
        if snapshot.content_revision != request.expected_content_revision {
            return Err(CoreError::Conflict("notebook content changed".into()));
        }
        self.notebook_owners
            .change(NotebookOwnerChange {
                source: self.notebooks.source_key(),
                id: id.to_string(),
                expected_owner: request.expected_owner,
                owner: request.owner,
                source_blob: snapshot.content_revision,
                actor: actor.to_string(),
                reason: request.reason,
            })
            .await
    }
}

#[derive(Deserialize)]
struct AssignNotebookOwner {
    owner: String,
    expected_content_revision: String,
    #[serde(default)]
    expected_owner: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

fn notebook_precondition(headers: &HeaderMap) -> aster_core::Result<NotebookPrecondition> {
    let matched = headers.get_all(header::IF_MATCH).iter().collect::<Vec<_>>();
    let absent = headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .collect::<Vec<_>>();
    if matched.len() + absent.len() != 1 {
        return Err(CoreError::Invalid(
            "one notebook precondition is required".into(),
        ));
    }
    if let Some(value) = absent.first() {
        return if value.as_bytes() == b"*" {
            Ok(NotebookPrecondition::Absent)
        } else {
            Err(CoreError::Invalid("If-None-Match must be *".into()))
        };
    }
    let value = matched[0]
        .to_str()
        .map_err(|_| CoreError::Invalid("invalid If-Match".into()))?;
    let oid = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| CoreError::Invalid("If-Match needs a quoted blob OID".into()))?;
    if !matches!(oid.len(), 40 | 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid("invalid notebook blob OID".into()));
    }
    Ok(NotebookPrecondition::Blob(oid.to_string()))
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
    /// Browse catalog instance selected by the caller, distinct from SQL alias.
    catalog_context: Option<String>,
    catalog: Option<String>,
    schema: Option<String>,
    max_rows: Option<usize>,
    /// The notebook and cell this run belongs to. When both are set the server
    /// records the cell's last result for the notebook session exchange.
    notebook: Option<String>,
    cell: Option<String>,
}

/// Runs one query attempt for an already identified caller: role check, engine
/// resolution, engine grant, execution, and exactly one audit row. Both the REST
/// route and the RPC leg call this, so the trail is the same whichever protocol
/// the caller used — and a refusal is recorded too, with `ok: false`.
pub(crate) async fn execute_query(
    state: &AppState,
    principal: &Principal,
    verified_session: bool,
    body: &QueryBody,
) -> Result<aster_core::QueryResult, CoreError> {
    let engine_id = body
        .engine
        .clone()
        .or_else(|| state.config.default_engine.clone())
        .map(EngineId::new);

    let started = Instant::now();
    let outcome = match authorize(principal, aster_core::Action::RunQuery) {
        Ok(()) => run_attempt(state, principal, verified_session, body, engine_id.as_ref()).await,
        Err(error) => Err(error),
    };
    let latency_ms = started.elapsed().as_millis() as u64;

    let (ok, row_count) = match &outcome {
        Ok((result, _)) => (true, result.rows.len()),
        Err(_) => (false, 0),
    };
    let event = AuditEvent {
        subject: principal.subject.clone(),
        // A refusal before routing has no engine: name that in the trail instead
        // of dropping the attempt.
        engine: engine_id.unwrap_or_else(|| EngineId::new("(unrouted)")),
        catalog: match &outcome {
            Ok((_, context)) => Some(context.clone()),
            Err(_) => body
                .catalog_context
                .clone()
                .or_else(|| body.catalog.clone())
                .or_else(|| state.config.default_catalog.clone()),
        },
        schema: body.schema.clone(),
        sql: body.sql.clone(),
        latency_ms,
        row_count,
        ok,
    };
    if let Err(error) = state.audit.record(&event).await {
        tracing::error!(%error, "failed to record audit event");
    }

    outcome.map(|(result, _)| result)
}

async fn run_attempt(
    state: &AppState,
    principal: &Principal,
    verified_session: bool,
    body: &QueryBody,
    engine_id: Option<&EngineId>,
) -> Result<(aster_core::QueryResult, String), CoreError> {
    let engine_id =
        engine_id.ok_or_else(|| CoreError::Invalid("no engine selected and no default".into()))?;
    let engine = state
        .engines
        .get(engine_id)
        .ok_or_else(|| CoreError::NotFound("unknown engine".into()))?;
    authorize_engine(state.grants.as_ref(), &principal.subject, engine_id).await?;
    let binding = resolve_catalog_binding(state, engine_id, body)?;
    if binding.policy == aster_core::BindingPolicy::Protected && !verified_session {
        return Err(CoreError::Unauthorized(
            "protected catalog requires a verified session".into(),
        ));
    }
    let request = QueryRequest {
        /* A cell usually ends with a statement terminator, which the engines
        reject; trim it so every route sends a single statement. */
        sql: body
            .sql
            .trim_end()
            .trim_end_matches(';')
            .trim_end()
            .to_string(),
        // Spark Connect has no per-request session catalog; its SQL must
        // remain qualified and its adapter must not receive this value.
        catalog: engine
            .uses_catalog()
            .then(|| binding.native_catalog.clone()),
        schema: body.schema.clone(),
        max_rows: body.max_rows,
    };
    let result = if binding.policy == aster_core::BindingPolicy::Protected {
        engine
            .execute_as_verified(request, &principal.subject)
            .await?
    } else {
        engine.execute(request).await?
    };
    Ok((result, binding.catalog.clone()))
}

fn resolve_catalog_binding<'a>(
    state: &'a AppState,
    engine_id: &EngineId,
    body: &QueryBody,
) -> Result<&'a aster_core::CatalogBindingConfig, CoreError> {
    if let Some(context) = &body.catalog_context {
        if state
            .catalogs
            .get(&CatalogId::new(context.clone()))
            .is_none()
        {
            return Err(CoreError::NotFound("unknown catalog context".into()));
        }
    }

    let mut matches = state.config.catalog_bindings.iter().filter(|binding| {
        binding.engine == engine_id.0
            && match &body.catalog_context {
                Some(context) => binding.catalog == *context,
                None => match body
                    .catalog
                    .as_ref()
                    .or(state.config.default_catalog.as_ref())
                {
                    Some(legacy) => binding.native_catalog == *legacy || binding.catalog == *legacy,
                    None => true,
                },
            }
    });
    let binding = matches.next().ok_or_else(|| {
        if body.catalog_context.is_some() {
            CoreError::Unauthorized("engine is not connected to selected catalog".into())
        } else {
            CoreError::Invalid("no unique catalog context for engine".into())
        }
    })?;
    if matches.next().is_some() {
        return Err(CoreError::Invalid(
            "ambiguous catalog context for engine".into(),
        ));
    }
    if state
        .catalogs
        .get(&CatalogId::new(binding.catalog.clone()))
        .is_none()
    {
        return Err(CoreError::NotFound(
            "configured catalog context is unknown".into(),
        ));
    }
    if let Some(alias) = &body.catalog {
        if alias != &binding.native_catalog {
            return Err(CoreError::Invalid(
                "SQL catalog conflicts with selected context".into(),
            ));
        }
    }
    Ok(binding)
}

pub(crate) fn protected_catalogs_present(state: &AppState) -> bool {
    state
        .config
        .catalog_bindings
        .iter()
        .any(|binding| binding.policy == aster_core::BindingPolicy::Protected)
}

pub(crate) fn catalog_metadata_visible(state: &AppState, id: &str) -> bool {
    !protected_catalogs_present(state)
        && state.config.catalog_bindings.iter().any(|binding| {
            binding.catalog == id && binding.policy == aster_core::BindingPolicy::Unprotected
        })
}

pub(crate) fn require_catalog_metadata(state: &AppState, id: &str) -> Result<(), CoreError> {
    if catalog_metadata_visible(state, id) {
        Ok(())
    } else {
        Err(CoreError::Unauthorized(
            "catalog metadata requires backend user policy".into(),
        ))
    }
}

pub(crate) fn require_unscoped_metadata(state: &AppState) -> Result<(), CoreError> {
    if protected_catalogs_present(state)
        || (state.config.catalog_bindings.is_empty() && !state.contracts.is_empty())
    {
        Err(CoreError::Unauthorized(
            "unscoped metadata lacks an explicit catalog policy".into(),
        ))
    } else {
        Ok(())
    }
}

/// Resolve the caller: an SSO session cookie first, then the pre-SSO dev seam
/// (headers or `aster_subject`/`aster_roles` cookies) when dev login is enabled.
pub(crate) async fn principal(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<Principal, ApiError> {
    Ok(principal_with_session(state, headers).await?.0)
}

/// The provenance bit requires a stored session created after verified OIDC
/// identity. A cookie alone, a legacy/dev session, or a header cannot set it.
pub(crate) async fn principal_with_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(Principal, bool), ApiError> {
    if let Some(sid) = cookie(headers, "aster_session") {
        match state.sessions.get(&sid, now()).await {
            Ok(Some(record)) => {
                let verified_session = record.verified;
                return Ok((
                    Principal {
                        subject: record.subject,
                        roles: record.roles,
                        groups: record.groups,
                        user_uuid: record.user_uuid,
                    },
                    verified_session,
                ));
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

    Ok((
        Principal {
            subject,
            roles,
            groups: vec![],
            user_uuid: None,
        },
        false,
    ))
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
    Query(query): Query<EngineListQuery>,
) -> Result<Json<Vec<EngineSummary>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    let mut summaries = Vec::new();
    for engine in available_engines(&state, &principal, query.catalog_context.as_deref()).await? {
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

#[derive(Default, Deserialize)]
struct EngineListQuery {
    catalog_context: Option<String>,
}

pub(crate) async fn available_engines(
    state: &AppState,
    principal: &Principal,
    requested_context: Option<&str>,
) -> Result<Vec<Arc<dyn aster_core::QueryEngine>>, CoreError> {
    authorize(principal, aster_core::Action::ReadNotebook)?;
    if let Some(context) = requested_context {
        require_catalog_metadata(state, context)?;
    }
    if state.config.catalog_bindings.is_empty() {
        return Err(CoreError::Invalid(
            "no catalog-to-engine binding configured".into(),
        ));
    }
    let candidates: std::collections::HashSet<&str> = state
        .config
        .catalog_bindings
        .iter()
        .filter(|binding| {
            requested_context
                .or(state.config.default_catalog.as_deref())
                .is_none_or(|value| binding.catalog == value || binding.native_catalog == value)
        })
        .map(|binding| binding.catalog.as_str())
        .collect();
    let context = match requested_context {
        Some(context) => {
            if state.catalogs.get(&CatalogId::new(context)).is_none() {
                return Err(CoreError::NotFound("unknown catalog context".into()));
            }
            context
        }
        None if candidates.len() == 1 => candidates.into_iter().next().unwrap(),
        None => {
            return Err(CoreError::Invalid(
                "no unique catalog context for engine choices".into(),
            ))
        }
    };
    let mut available = Vec::new();
    for engine in state.engines.list() {
        let id = &engine.info().id;
        if state.config.catalog_bindings.iter().any(|binding| {
            binding.catalog == context
                && binding.engine == id.0
                && binding.policy == aster_core::BindingPolicy::Unprotected
        }) && state.grants.allowed(&principal.subject, id).await?
        {
            available.push(engine);
        }
    }
    Ok(available)
}

async fn list_catalogs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<CatalogSummary>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let mut summaries = Vec::new();
    for catalog in state.catalogs.list() {
        if !catalog_metadata_visible(&state, &catalog.id().0) {
            continue;
        }
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
    require_unscoped_metadata(&state)?;
    Ok(Json(state.contracts.as_ref().clone()))
}

async fn run_query(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<QueryBody>,
) -> Result<Json<aster_core::QueryResult>, ApiError> {
    let (principal, verified_session) = principal_with_session(&state, &headers).await?;
    let result = execute_query(&state, &principal, verified_session, &body).await?;
    if let (Some(notebook), Some(cell)) = (body.notebook.as_deref(), body.cell.as_deref()) {
        let columns: Vec<String> = result
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
        let rows_json: Vec<String> = result
            .rows
            .iter()
            .filter_map(|row| serde_json::to_string(row).ok())
            .collect();
        /* Bookkeeping for the notebook session exchange: a failure here must
        never fail a run that already succeeded. */
        if let Err(error) = exchange::record_result(
            &state,
            &principal,
            notebook,
            cell,
            &columns,
            &rows_json,
            result.truncated,
        )
        .await
        {
            tracing::warn!(%error, "cell result was not recorded for the session exchange");
        }
    }
    Ok(Json(result))
}

async fn list_namespaces(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Vec<aster_core::Namespace>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    require_catalog_metadata(&state, &id)?;
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
) -> Result<Json<Vec<aster_core::TableDescriptor>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    require_catalog_metadata(&state, &id)?;
    let catalog = state
        .catalogs
        .get(&CatalogId::new(id))
        .ok_or_else(|| CoreError::NotFound("unknown catalog".into()))?;
    Ok(Json(catalog.list_table_descriptors(&namespace).await?))
}

async fn list_notebooks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<String>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    if state.team_workspaces.is_some() {
        return Err(
            CoreError::Unauthorized("workspace-qualified notebook route required".into()).into(),
        );
    }
    Ok(Json(state.notebooks.list(&principal.subject).await?))
}

#[derive(Deserialize)]
pub(crate) struct CompleteQuery {
    /// The cell text up to the caret: the dotted name being typed is its tail.
    sql: String,
}

#[derive(Serialize)]
struct Candidate {
    label: String,
    kind: &'static str,
    /// What to put in the cell: the label quoted when it is not a plain SQL
    /// identifier (a catalog like `polaris-local` has to be `"polaris-local"`).
    insert: String,
}

/// Quoted when it has to be: `polaris-local` is a catalog name, not an identifier.
fn insert_text(label: &str) -> String {
    let mut chars = label.chars();
    let plain = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain {
        label.to_string()
    } else {
        format!("\"{}\"", label.replace('"', "\"\""))
    }
}

fn candidate(label: String, kind: &'static str) -> Candidate {
    let insert = insert_text(&label);
    Candidate {
        label,
        kind,
        insert,
    }
}

/// Offered when the caret is not inside a dotted name. Kept short on purpose:
/// the point of the list is the namespace objects, not a full grammar.
const SQL_KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP BY",
    "ORDER BY",
    "HAVING",
    "LIMIT",
    "JOIN",
    "LEFT JOIN",
    "INNER JOIN",
    "ON",
    "AS",
    "WITH",
    "UNION",
    "DISTINCT",
    "CREATE TABLE",
    "CREATE SCHEMA",
    "INSERT INTO",
    "DESCRIBE",
    "SHOW SCHEMAS",
    "SHOW TABLES",
    "EXPLAIN",
];

/// The dotted path the caret sits in: `select * from polaris.sales.par` gives
/// `["polaris", "sales", "par"]`, and a trailing dot yields a final empty part.
/// Quotes are kept so `"polaris-local".sales.` parses, then stripped per part.
fn dotted_path(text: &str) -> Vec<&str> {
    let tail = text.trim_end();
    let start = tail
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '-' || c == '"'))
        .map(|index| index + 1)
        .unwrap_or(0);
    tail[start..]
        .split('.')
        .map(|part| part.trim_matches('"'))
        .collect()
}

/// Completion for the editor. It never fails the request: an unreachable catalog
/// just yields fewer suggestions, because a suggestion list must not interrupt
/// typing — which is also why an unknown catalog is not an error here.
async fn complete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<CompleteQuery>,
) -> Result<Json<Vec<Candidate>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;

    let path = dotted_path(&params.sql);
    let (catalog, schema, table, prefix) = match path.as_slice() {
        [prefix] => (None, None, None, *prefix),
        [catalog, prefix] => (Some(*catalog), None, None, *prefix),
        [catalog, schema, prefix] => (Some(*catalog), Some(*schema), None, *prefix),
        [catalog, schema, table, prefix] => (Some(*catalog), Some(*schema), Some(*table), *prefix),
        [] => (None, None, None, ""),
        _ => return Ok(Json(Vec::new())),
    };
    let prefix = prefix.to_ascii_lowercase();
    let wanted = |label: &str| label.to_ascii_lowercase().starts_with(&prefix);

    let mut candidates = Vec::new();
    let Some(catalog) = catalog else {
        for entry in state.catalogs.list() {
            if !catalog_metadata_visible(&state, &entry.id().0) {
                continue;
            }
            let label = entry.id().0.clone();
            if wanted(&label) {
                candidates.push(candidate(label, "catalog"));
            }
        }
        for keyword in SQL_KEYWORDS {
            if wanted(keyword) {
                candidates.push(candidate((*keyword).to_string(), "keyword"));
            }
        }
        return Ok(Json(candidates));
    };

    if !catalog_metadata_visible(&state, catalog) {
        return Ok(Json(candidates));
    }

    let Some(entry) = state.catalogs.get(&CatalogId::new(catalog)) else {
        return Ok(Json(candidates));
    };
    match (schema, table) {
        (Some(schema), Some(table)) => {
            let reference = aster_core::TableRef {
                namespace: schema.to_string(),
                name: table.to_string(),
            };
            for column in entry
                .table_schema(&reference)
                .await
                .map(|schema| schema.columns)
                .unwrap_or_default()
            {
                if wanted(&column.name) {
                    candidates.push(candidate(column.name, "column"));
                }
            }
        }
        (Some(schema), None) => {
            for table in entry.list_tables(schema).await.unwrap_or_default() {
                if wanted(&table.name) {
                    candidates.push(candidate(table.name, "table"));
                }
            }
        }
        (None, _) => {
            for namespace in entry.list_namespaces().await.unwrap_or_default() {
                if wanted(&namespace.name) {
                    candidates.push(candidate(namespace.name, "schema"));
                }
            }
        }
    }
    Ok(Json(candidates))
}

async fn get_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    if state.team_workspaces.is_some() {
        return Err(
            CoreError::Unauthorized("workspace-qualified notebook route required".into()).into(),
        );
    }
    let snapshot = state.notebook_snapshot(&id).await?;
    let mut response = Json(snapshot.notebook).into_response();
    response.headers_mut().insert(
        header::ETAG,
        format!("\"{}\"", snapshot.content_revision)
            .parse()
            .map_err(|_| CoreError::Storage("invalid blob OID header".into()))?,
    );
    Ok(response)
}

fn team_workspaces(state: &AppState) -> Result<&TeamWorkspaces, ApiError> {
    state
        .team_workspaces
        .as_deref()
        .ok_or_else(|| CoreError::NotFound("team notebooks are disabled".into()).into())
}

/// A team workspace requires an actual server-issued session. The development
/// header seam never carries verified membership or a session branch key.
async fn team_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(Principal, String), ApiError> {
    let sid = cookie(headers, "aster_session")
        .ok_or_else(|| CoreError::Unauthorized("signed-in session required".into()))?;
    let record = state
        .sessions
        .get(&sid, now())
        .await?
        .ok_or_else(|| CoreError::Unauthorized("session expired".into()))?;
    Ok((
        Principal {
            subject: record.subject,
            roles: record.roles,
            groups: record.groups,
            user_uuid: record.user_uuid,
        },
        sid,
    ))
}

/// The local workspace still uses session path claims to locate its checkout.
/// Once a current identity reader is configured, only the fresh UUID policy
/// can grant team access; cached claims are an additional local constraint.
async fn team_member_session(
    state: &AppState,
    headers: &HeaderMap,
    team: &str,
) -> Result<(Principal, String), ApiError> {
    let (session, sid) = team_session(state, headers).await?;
    if state.current_identity.is_some() || state.team_git_targets.is_some() {
        let fresh = current_identity::current_principal(state, headers, false).await?;
        state
            .team_git_targets
            .as_deref()
            .ok_or_else(|| CoreError::Unauthorized("current team policy unavailable".into()))?
            .member(team, &fresh)?;
    }
    Ok((session, sid))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamTargetRequest {
    repository: String,
    default_branch: String,
}

fn team_target_precondition(headers: &HeaderMap) -> aster_core::Result<Option<u64>> {
    match (
        headers.get(header::IF_NONE_MATCH),
        headers.get(header::IF_MATCH),
    ) {
        (Some(absent), None) if absent == "*" => Ok(None),
        (None, Some(version)) => {
            let value = version
                .to_str()
                .map_err(|_| CoreError::Invalid("invalid target If-Match".into()))?;
            let digits = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .ok_or_else(|| {
                    CoreError::Invalid("target If-Match must be a quoted version".into())
                })?;
            let version = digits
                .parse::<u64>()
                .map_err(|_| CoreError::Invalid("invalid target version".into()))?;
            Ok(Some(version))
        }
        _ => Err(CoreError::Invalid(
            "target version precondition required".into(),
        )),
    }
}

async fn configure_team_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(team): Path<String>,
    Json(request): Json<TeamTargetRequest>,
) -> Result<Response, ApiError> {
    if let Some(targets) = &state.team_git_targets {
        let principal = current_identity::current_principal(&state, &headers, false).await?;
        targets.maintainer(&team, &principal)?;
        let expected = team_target_precondition(&headers)?;
        return Ok(Json(
            targets
                .configure(
                    &team,
                    &principal,
                    &request.repository,
                    &request.default_branch,
                    expected,
                )
                .await?,
        )
        .into_response());
    }
    let workspaces = team_workspaces(&state)?;
    let (session_principal, _) = team_session(&state, &headers).await?;
    let principal = if state.current_identity.is_some() {
        current_identity::current_principal(&state, &headers, false).await?
    } else {
        session_principal
    };
    workspaces.maintainer(&team, &principal)?;
    let expected = team_target_precondition(&headers)?;
    Ok(Json(workspaces.configure(
        &team,
        &request.repository,
        &request.default_branch,
        expected,
        &principal.subject,
    )?)
    .into_response())
}

async fn get_team_target(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(team): Path<String>,
) -> Result<Response, ApiError> {
    if let Some(targets) = &state.team_git_targets {
        let principal = current_identity::current_principal(&state, &headers, false).await?;
        return Ok(Json(targets.get(&team, &principal).await?).into_response());
    }
    let workspaces = team_workspaces(&state)?;
    let (principal, _) = team_session(&state, &headers).await?;
    workspaces.member(&team, &principal)?;
    Ok(Json(
        workspaces
            .target(&team)?
            .ok_or_else(|| CoreError::NotFound("team notebook target not configured".into()))?,
    )
    .into_response())
}

fn personal_workspace(headers: &HeaderMap) -> aster_core::Result<bool> {
    match headers
        .get("x-aster-workspace")
        .and_then(|value| value.to_str().ok())
    {
        None | Some("session") => Ok(false),
        Some("personal") => Ok(true),
        _ => Err(CoreError::Invalid("invalid workspace selection".into())),
    }
}

async fn team_notebook_context(
    state: &AppState,
    headers: &HeaderMap,
    team: &str,
    id: &str,
) -> Result<(Principal, String), ApiError> {
    let personal = personal_workspace(headers)?;
    team_notebook_context_selected(state, headers, team, id, personal)
        .await
        .map_err(Into::into)
}

pub(crate) async fn team_notebook_context_selected(
    state: &AppState,
    headers: &HeaderMap,
    team: &str,
    id: &str,
    personal: bool,
) -> aster_core::Result<(Principal, String)> {
    let workspaces = state
        .team_workspaces
        .as_deref()
        .ok_or_else(|| CoreError::NotFound("team notebooks are disabled".into()))?;
    let (principal, sid) = team_member_session(state, headers, team)
        .await
        .map_err(|error| error.0)?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let key = workspaces.notebook_context_key(team, &principal, &sid, personal, id)?;
    let store = workspaces.workspace(team, &principal, &sid, personal)?;
    store.get(id).await?;
    Ok((principal, key))
}

async fn get_team_conversation(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((team, id)): Path<(String, String)>,
) -> Result<Json<aster_core::Conversation>, ApiError> {
    let (principal, key) = team_notebook_context(&state, &headers, &team, &id).await?;
    Ok(Json(
        state.conversations.get(&principal.subject, &key).await?,
    ))
}

async fn get_team_helper(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((team, id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (principal, key) = team_notebook_context(&state, &headers, &team, &id).await?;
    let helper = ai::selected_helper_scoped(&state, &principal.subject, &key).await?;
    Ok(Json(serde_json::json!({"helper": helper})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamHelperRequest {
    helper: String,
}

async fn put_team_helper(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((team, id)): Path<(String, String)>,
    Json(request): Json<TeamHelperRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (principal, key) = team_notebook_context(&state, &headers, &team, &id).await?;
    ai::choose_helper_scoped(&state, &headers, &principal.subject, &key, &request.helper).await?;
    Ok(Json(serde_json::json!({"helper": request.helper})))
}

async fn get_team_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((team, id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let workspaces = team_workspaces(&state)?;
    let (principal, sid) = team_member_session(&state, &headers, &team).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let store = workspaces.workspace(&team, &principal, &sid, personal_workspace(&headers)?)?;
    let snapshot = store.snapshot(&id).await?;
    let mut response = Json(snapshot.notebook).into_response();
    response.headers_mut().insert(
        header::ETAG,
        format!("\"{}\"", snapshot.content_revision)
            .parse()
            .map_err(|_| CoreError::Storage("invalid blob OID header".into()))?,
    );
    Ok(response)
}

async fn list_team_recovery_sessions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(team): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let workspaces = team_workspaces(&state)?;
    let (principal, sid) = team_member_session(&state, &headers, &team).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let keys = workspaces.recoverable(&team, &principal, &sid)?;
    Ok(Json(
        keys.into_iter()
            .map(|key| serde_json::json!({"session_key": key}))
            .collect(),
    ))
}

async fn get_team_recovery_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((team, key, id)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let workspaces = team_workspaces(&state)?;
    let (principal, _) = team_member_session(&state, &headers, &team).await?;
    authorize(&principal, aster_core::Action::ReadNotebook)?;
    let store = workspaces.recovery_workspace(&team, &principal, &key)?;
    let snapshot = store.snapshot(&id).await?;
    let mut response = Json(snapshot.notebook).into_response();
    response.headers_mut().insert(
        header::ETAG,
        format!("\"{}\"", snapshot.content_revision)
            .parse()
            .map_err(|_| CoreError::Storage("invalid blob OID header".into()))?,
    );
    Ok(response)
}

async fn sync_team_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((team, id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let workspaces = team_workspaces(&state)?;
    let (principal, sid) = team_member_session(&state, &headers, &team).await?;
    authorize(&principal, aster_core::Action::WriteNotebook)?;
    workspaces.member(&team, &principal)?;
    if body != serde_json::json!({}) {
        return Err(
            CoreError::Invalid("Sync accepts no repository or branch override".into()).into(),
        );
    }
    let remote_revision = workspaces
        .sync_local_fixture(&team, &principal, &sid, personal_workspace(&headers)?, &id)
        .await?;
    Ok(Json(
        serde_json::json!({"status":"synced", "remote_revision":remote_revision}),
    ))
}

async fn save_team_notebook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((team, id)): Path<(String, String)>,
    Json(value): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let workspaces = team_workspaces(&state)?;
    let (principal, sid) = team_member_session(&state, &headers, &team).await?;
    authorize(&principal, aster_core::Action::WriteNotebook)?;
    workspaces.member(&team, &principal)?;
    let fields = value
        .as_object()
        .ok_or_else(|| CoreError::Invalid("notebook body must be an object".into()))?;
    if fields
        .keys()
        .any(|field| !matches!(field.as_str(), "id" | "title" | "cells"))
    {
        return Err(CoreError::Invalid(
            "client workspace or repository fields are not accepted".into(),
        )
        .into());
    }
    let mut notebook: Notebook = serde_json::from_value(value)
        .map_err(|_| CoreError::Invalid("invalid notebook body".into()))?;
    notebook.id = id;
    let expected = notebook_precondition(&headers)?;
    let store = workspaces.workspace(&team, &principal, &sid, personal_workspace(&headers)?)?;
    let saved = store
        .save_if(&notebook, &principal.subject, expected)
        .await?;
    Ok(Json(
        serde_json::json!({ "revision": saved.revision, "content_revision": saved.content_revision }),
    ))
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
    let expected = notebook_precondition(&headers)?;
    let saved = state
        .save_notebook_owned(&notebook, &principal.subject, expected)
        .await?;
    Ok(Json(serde_json::json!({
        "revision": saved.revision,
        "content_revision": saved.content_revision,
    })))
}

async fn assign_notebook_owner(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<AssignNotebookOwner>,
) -> Result<StatusCode, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::Administer)?;
    state
        .assign_notebook_owner(&id, &principal.subject, request)
        .await?;
    Ok(StatusCode::OK)
}

async fn list_unassigned_notebooks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, aster_core::Action::Administer)?;
    let source = state.notebooks.source_key();
    let mut result = Vec::new();
    for id in state.notebooks.list(&principal.subject).await? {
        if state.notebook_owners.record(&source, &id).await?.is_none() {
            let snapshot = state.notebook_snapshot(&id).await?;
            result.push(serde_json::json!({
                "id": id,
                "content_revision": snapshot.content_revision,
                "source": source,
            }));
        }
    }
    Ok(Json(result))
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
        .route(
            "/settings/llm/{id}/delete",
            axum::routing::post(ai::remove_config_form),
        )
        .route("/api/llm", get(ai::list_configs))
        .route(
            "/api/llm/{id}",
            axum::routing::put(ai::put_config).delete(ai::remove_config),
        )
        .route("/api/admin/shared-models", get(shared_models::list_admin))
        .route(
            "/api/admin/shared-models/rekey",
            post(shared_models::reencrypt_admin),
        )
        .route(
            "/api/admin/shared-models/{id}",
            axum::routing::put(shared_models::put_admin).delete(shared_models::remove_admin),
        )
        .route(
            "/api/admin/shared-models/{id}/grants",
            get(shared_models::get_grants_admin).put(shared_models::replace_grants_admin),
        )
        .route(
            "/api/admin/shared-models/{id}/grants/events",
            get(shared_models::grant_events_admin),
        )
        .route("/api/ai", post(ai::generate))
        .route(
            "/api/notebooks/{id}/helper",
            get(ai::get_notebook_helper).put(ai::put_notebook_helper),
        )
        .route("/catalog/{id}/{namespace}", get(web::catalog_namespace))
        .route("/catalog/{id}/{namespace}/{table}", get(web::catalog_table))
        .route("/healthz", get(healthz))
        .route("/api/engines", get(list_engines))
        .route("/api/catalogs", get(list_catalogs))
        .route("/api/contracts", get(list_contracts))
        .route("/api/query", post(run_query))
        .route("/api/sql/complete", get(complete))
        .route("/api/audit", get(list_audit))
        .route("/api/state", get(get_state).put(put_state))
        .route("/api/notebooks", get(list_notebooks))
        .route("/api/notebooks/{id}", get(get_notebook).put(save_notebook))
        .route(
            "/api/teams/{team}/notebook-target",
            axum::routing::put(configure_team_target).get(get_team_target),
        )
        .route(
            "/api/teams/{team}/recovery/sessions",
            get(list_team_recovery_sessions),
        )
        .route(
            "/api/teams/{team}/recovery/sessions/{key}/notebooks/{id}",
            get(get_team_recovery_notebook),
        )
        .route(
            "/api/teams/{team}/notebooks/{id}",
            get(get_team_notebook).put(save_team_notebook),
        )
        .route(
            "/api/teams/{team}/notebooks/{id}/conversation",
            get(get_team_conversation),
        )
        .route(
            "/api/teams/{team}/notebooks/{id}/helper",
            get(get_team_helper).put(put_team_helper),
        )
        .route(
            "/api/teams/{team}/notebooks/{id}/sync",
            post(sync_team_notebook),
        )
        .route("/teams/{team}/notebooks/{id}", get(web::team_notebook_view))
        .route(
            "/api/admin/notebooks/unassigned",
            get(list_unassigned_notebooks),
        )
        .route(
            "/api/admin/notebooks/{id}/owner",
            axum::routing::put(assign_notebook_owner),
        )
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

    let config = AppConfig::from_env()?;
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
