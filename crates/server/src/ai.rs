//! Per-user LLM endpoint registration and SQL generation.
//!
//! A user may register several OpenAI-compatible endpoints, each under a short
//! name, and choose one per notebook. The token a user registers is stored
//! server-side and never returned: requests leave through this proxy so the
//! browser and TUI never hold the credential.

use std::sync::Arc;

use aster_core::{authorize, Action, CoreError, LlmConfig, Principal};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::{principal, ApiError, AppState};

#[derive(Deserialize)]
pub(crate) struct LlmBody {
    base_url: String,
    model: String,
    #[serde(default)]
    api_key: String,
}

/// What a client may know about a registered helper: its name and destination,
/// never the token.
#[derive(Serialize)]
pub(crate) struct LlmSummary {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    model: String,
    scope: &'static str,
    #[serde(rename = "ref")]
    reference: String,
}

impl LlmSummary {
    fn of(config: LlmConfig) -> Self {
        Self {
            reference: format!("personal/{}", config.id),
            id: config.id,
            base_url: Some(safe_base_url(&config.base_url).to_owned()),
            model: config.model,
            scope: "personal",
        }
    }
}

pub(crate) async fn list_configs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<LlmSummary>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    let configs = state.llm.list(&principal.subject).await?;
    let mut summaries: Vec<_> = configs.into_iter().map(LlmSummary::of).collect();
    if state.shared_model_use_enabled {
        if let Some(shared_models) = state.shared_models.as_ref() {
            // A stale or unavailable authority may never reveal a shared model.
            // Personal registrations remain readable through their own scope.
            if let Ok(current) =
                crate::current_identity::current_principal(&state, &headers, false).await
            {
                if authorize(&current, Action::RunQuery).is_ok() {
                    for shared in shared_models.available_for_current(&current).await? {
                        summaries.push(LlmSummary {
                            reference: format!("shared/{}", shared.id),
                            id: shared.id,
                            base_url: None,
                            model: shared.model,
                            scope: "shared",
                        });
                    }
                }
            }
        }
    }
    Ok(Json(summaries))
}

pub(crate) async fn put_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<LlmBody>,
) -> Result<Json<LlmSummary>, ApiError> {
    let principal = principal(&state, &headers).await?;
    validated_endpoint(&body.base_url)?;
    state
        .llm
        .put(store_config(&principal, id.clone(), body))
        .await?;
    let config = state
        .llm
        .get(&principal.subject, &id)
        .await?
        .ok_or_else(|| CoreError::Storage("llm configuration did not persist".into()))?;
    Ok(Json(LlmSummary::of(config)))
}

pub(crate) async fn remove_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let principal = principal(&state, &headers).await?;
    state.llm.remove(&principal.subject, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Browsers cannot PUT or DELETE from a plain form, so the settings page posts here.
pub(crate) async fn put_config_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Form(form): axum::extract::Form<LlmForm>,
) -> Result<axum::response::Response, ApiError> {
    let principal = principal(&state, &headers).await?;
    let body = LlmBody {
        base_url: form.base_url,
        model: form.model,
        api_key: form.api_key,
    };
    validated_endpoint(&body.base_url)?;
    state
        .llm
        .put(store_config(&principal, form.id, body))
        .await?;
    Ok(axum::response::Redirect::to("/settings/llm").into_response())
}

/// The settings form names the helper, so one route both adds and edits.
#[derive(Deserialize)]
pub(crate) struct LlmForm {
    id: String,
    base_url: String,
    model: String,
    #[serde(default)]
    api_key: String,
}

pub(crate) async fn remove_config_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let principal = principal(&state, &headers).await?;
    state.llm.remove(&principal.subject, &id).await?;
    Ok(axum::response::Redirect::to("/settings/llm").into_response())
}

#[derive(Deserialize)]
pub(crate) struct HelperChoiceInput {
    helper: String,
}

#[derive(Serialize)]
pub(crate) struct HelperChoiceOutput {
    helper: Option<String>,
}

pub(crate) async fn get_notebook_helper(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(notebook): Path<String>,
) -> Result<Json<HelperChoiceOutput>, ApiError> {
    if state.team_workspaces.is_some() {
        return Err(
            CoreError::Unauthorized("workspace-qualified helper route required".into()).into(),
        );
    }
    let principal = principal(&state, &headers).await?;
    authorize(&principal, Action::ReadNotebook)?;
    Ok(Json(HelperChoiceOutput {
        helper: selected_helper(&state, &principal.subject, &notebook).await?,
    }))
}

pub(crate) async fn put_notebook_helper(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(notebook): Path<String>,
    Json(body): Json<HelperChoiceInput>,
) -> Result<Json<HelperChoiceOutput>, ApiError> {
    if state.team_workspaces.is_some() {
        return Err(
            CoreError::Unauthorized("workspace-qualified helper route required".into()).into(),
        );
    }
    let principal = principal(&state, &headers).await?;
    authorize(&principal, Action::ReadNotebook)?;
    choose_helper(
        &state,
        &headers,
        &principal.subject,
        &notebook,
        &body.helper,
    )
    .await?;
    Ok(Json(HelperChoiceOutput {
        helper: Some(body.helper),
    }))
}

pub(crate) async fn selected_helper(
    state: &AppState,
    subject: &str,
    notebook: &str,
) -> Result<Option<String>, CoreError> {
    if !aster_core::llm::valid_id(notebook) {
        return Err(CoreError::Invalid("invalid notebook id".into()));
    }
    if let Some(helper) = state.user_state.get_helper(subject, notebook).await? {
        return Ok(Some(helper));
    }
    let legacy = state.user_state.get(subject).await?;
    if let Some(legacy) = legacy {
        if legacy.notebook.as_deref() == Some(notebook) {
            if let Some(helper) = legacy
                .helper
                .filter(|helper| aster_core::llm::valid_id(helper))
            {
                return state
                    .user_state
                    .put_helper_if_absent(subject, notebook, &helper)
                    .await
                    .map(Some);
            }
        }
    }
    Ok(None)
}

/// A team workspace has no legacy unqualified helper migration path.
pub(crate) async fn selected_helper_scoped(
    state: &AppState,
    subject: &str,
    notebook_key: &str,
) -> Result<Option<String>, CoreError> {
    if !aster_core::llm::valid_id(notebook_key) {
        return Err(CoreError::Invalid("invalid notebook context".into()));
    }
    state.user_state.get_helper(subject, notebook_key).await
}

pub(crate) async fn choose_helper(
    state: &AppState,
    headers: &HeaderMap,
    subject: &str,
    notebook: &str,
    helper: &str,
) -> Result<(), CoreError> {
    choose_helper_scoped(state, headers, subject, notebook, helper).await
}

pub(crate) async fn choose_helper_scoped(
    state: &AppState,
    headers: &HeaderMap,
    subject: &str,
    notebook_key: &str,
    helper: &str,
) -> Result<(), CoreError> {
    if !aster_core::llm::valid_id(notebook_key) {
        return Err(CoreError::Invalid("invalid notebook or helper id".into()));
    }
    let (scope, id) = if let Some(id) = helper.strip_prefix("personal/") {
        ("personal", id)
    } else if let Some(id) = helper.strip_prefix("shared/") {
        ("shared", id)
    } else {
        ("legacy", helper)
    };
    if !aster_core::llm::valid_id(id) {
        return Err(CoreError::Invalid("invalid notebook or helper id".into()));
    }
    if scope == "shared" {
        if !state.shared_model_use_enabled {
            return Err(CoreError::Unauthorized("shared helper unavailable".into()));
        }
        let current = crate::current_identity::current_principal(state, headers, false)
            .await
            .map_err(|error| error.0)?;
        authorize(&current, Action::RunQuery)?;
        let registry = state
            .shared_models
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("shared helper unavailable".into()))?;
        if !registry
            .available_for_current(&current)
            .await?
            .iter()
            .any(|model| model.id == id)
        {
            return Err(CoreError::Unauthorized("shared helper unavailable".into()));
        }
    } else if state.llm.get(subject, id).await?.is_none() {
        return Err(CoreError::NotFound(
            "selected helper is not registered".into(),
        ));
    }
    state
        .user_state
        .put_helper(subject, notebook_key, helper)
        .await
}

fn store_config(principal: &Principal, id: String, body: LlmBody) -> LlmConfig {
    LlmConfig {
        subject: principal.subject.clone(),
        id,
        base_url: body.base_url,
        model: body.model,
        api_key: body.api_key,
    }
}

#[derive(Deserialize)]
pub(crate) struct AiBody {
    prompt: String,
    #[serde(default)]
    sql: Option<String>,
    /// Which registered helper to ask; the first one when omitted.
    #[serde(default)]
    helper: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct Completion {
    sql: String,
}

const SYSTEM_PROMPT: &str = "You write Trino SQL. Answer with one SQL statement and no prose.";

/// Shared and personal helper calls use the process outbound client. A
/// redirect must never carry a bearer token to another destination.
pub(crate) fn outbound_http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(60))
        .build()
}

/// How many bytes of catalog/contract/semantic reference one turn may carry, so
/// a wide catalog cannot crowd out the conversation.
const GROUNDING_LIMIT: usize = 16 * 1024;

/// Reference material for one question, retrieval-scoped: each data contract
/// whose table the question or attached cell SQL names, followed by the
/// catalog's live schema and the Cube model emitted for that table. Empty when
/// nothing matches. It is transient context for this turn, never history.
pub(crate) async fn grounding(state: &AppState, text: &str) -> String {
    let mut out = String::new();
    for contract in aster_core::relevant(&state.contracts, text) {
        if out.len() >= GROUNDING_LIMIT {
            break;
        }
        out.push_str(&contract.summary());
        out.push('\n');
        if let Some(schema) = find_table(state, &contract.name).await {
            let columns = schema
                .columns
                .iter()
                .map(|column| format!("{} {}", column.name, column.data_type))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "catalog table {}.{}: {columns}\n",
                schema.table.namespace, schema.table.name
            ));
            if let Ok(format) = aster_core::semantic::format("cube") {
                if let Ok(model) = format.render(&aster_core::TableModel {
                    schema: &schema,
                    contract: Some(contract),
                }) {
                    out.push_str(&model);
                    out.push('\n');
                }
            }
        }
    }
    out
}

/// The first catalog table whose name matches, case-insensitively.
async fn find_table(state: &AppState, name: &str) -> Option<aster_core::TableSchema> {
    // ponytail: a full registry scan per referenced table; index by name if a
    // deployment ever carries enough catalogs for this to matter.
    for catalog in state.catalogs.list() {
        let Ok(namespaces) = catalog.list_namespaces().await else {
            continue;
        };
        for namespace in namespaces {
            let Ok(tables) = catalog.list_tables(&namespace.name).await else {
                continue;
            };
            for table in tables {
                if table.name.eq_ignore_ascii_case(name) {
                    return catalog.table_schema(&table).await.ok();
                }
            }
        }
    }
    None
}

/// Proxies one completion against one of the caller's own OpenAI-compatible endpoints.
pub(crate) async fn generate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AiBody>,
) -> Result<Json<Completion>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, Action::RunQuery)?;
    crate::require_unscoped_metadata(&state)?;

    let config = resolve_helper(&state, &headers, &principal, body.helper.as_deref()).await?;

    let mut prompt = body.prompt;
    if let Some(sql) = body.sql.filter(|sql| !sql.trim().is_empty()) {
        let contracts = aster_core::relevant(&state.contracts, &sql);
        if !contracts.is_empty() {
            prompt.push_str("\n\nData contracts:\n");
            for contract in contracts {
                prompt.push_str(&contract.summary());
                prompt.push('\n');
            }
        }
        prompt.push_str("\n\nCurrent cell:\n");
        prompt.push_str(&sql);
    }

    let response = completion_request(&state.http, &config, &prompt, &headers)?
        .send()
        .await
        .map_err(|error| CoreError::Storage(format!("llm request failed: {error}")))?;

    let status = response.status();
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|error| CoreError::Storage(format!("llm response unreadable: {error}")))?;
    if !status.is_success() {
        return Err(CoreError::Storage(format!("llm returned {status}")).into());
    }

    let text = payload
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .ok_or_else(|| CoreError::Storage("llm response had no message content".into()))?;

    Ok(Json(Completion {
        sql: text.trim().to_string(),
    }))
}

pub(crate) fn completion_request(
    http: &reqwest::Client,
    config: &LlmConfig,
    prompt: &str,
    headers: &HeaderMap,
) -> Result<reqwest::RequestBuilder, ApiError> {
    let session = match headers.get("x-opencode-session") {
        Some(value) => value
            .to_str()
            .ok()
            .filter(|value| aster_core::llm::valid_id(value))
            .ok_or_else(|| CoreError::Invalid("invalid helper conversation identifier".into()))?
            .to_owned(),
        None => openidconnect::Nonce::new_random().secret().clone(),
    };
    Ok(http
        .post(validated_endpoint(&config.base_url)?)
        .header("x-opencode-session", session)
        .header("user-agent", concat!("aster/", env!("CARGO_PKG_VERSION")))
        .bearer_auth(&config.api_key)
        .json(&serde_json::json!({
            "model": config.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": prompt},
            ],
        })))
}

/// Resolve an explicit scope before any model call. An omitted choice retains
/// the historical first-personal behavior; shared models are never a fallback.
pub(crate) async fn resolve_helper(
    state: &AppState,
    headers: &HeaderMap,
    principal: &Principal,
    wanted: Option<&str>,
) -> Result<LlmConfig, CoreError> {
    if let Some(shared_id) = wanted.and_then(|reference| reference.strip_prefix("shared/")) {
        if !aster_core::llm::valid_id(shared_id) {
            return Err(CoreError::Invalid("invalid helper name".into()));
        }
        if !state.shared_model_use_enabled {
            return Err(CoreError::Unauthorized("shared helper unavailable".into()));
        }
        let current = crate::current_identity::current_principal(state, headers, false)
            .await
            .map_err(|error| error.0)?;
        authorize(&current, Action::RunQuery)?;
        let registry = state
            .shared_models
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("shared helper unavailable".into()))?;
        return registry.authorized_config(shared_id, &current).await;
    }
    if let Some(id) = wanted {
        let id = id.strip_prefix("personal/").unwrap_or(id);
        if !aster_core::llm::valid_id(id) {
            return Err(CoreError::Invalid("invalid helper name".into()));
        }
        return state
            .llm
            .get(&principal.subject, id)
            .await?
            .ok_or_else(|| CoreError::NotFound("selected helper is not registered".into()));
    }
    state
        .llm
        .list(&principal.subject)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| CoreError::NotFound("no llm endpoint registered".into()))
}

/// Users register either the host, the `/v1` base, or the full chat URL.
fn chat_endpoint(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{}/v1/chat/completions", base.trim_end_matches("/v1"))
    }
}

fn validated_endpoint(base_url: &str) -> Result<String, ApiError> {
    let url = reqwest::Url::parse(base_url)
        .map_err(|_| CoreError::Invalid("invalid helper base URL".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CoreError::Invalid("invalid helper base URL".into()).into());
    }
    Ok(chat_endpoint(base_url))
}

/// Older records may contain URL credentials. Hide their entire URL in any UI.
pub(crate) fn safe_base_url(base_url: &str) -> &str {
    if validated_endpoint(base_url).is_ok() {
        base_url
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn outbound_client_refuses_a_token_bearing_redirect() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let redirected_hits = Arc::new(AtomicUsize::new(0));
        let hits = redirected_hits.clone();
        let destination = axum::Router::new().route(
            "/steal",
            axum::routing::any(move || {
                let hits = hits.clone();
                async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    StatusCode::OK
                }
            }),
        );
        let destination_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination_url = format!(
            "http://{}/steal",
            destination_listener.local_addr().unwrap()
        );
        let destination_task = tokio::spawn(async move {
            axum::serve(destination_listener, destination)
                .await
                .unwrap()
        });

        let redirect = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(move || {
                let destination_url = destination_url.clone();
                async move {
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        axum::http::header::LOCATION,
                        destination_url.parse().unwrap(),
                    );
                    (StatusCode::FOUND, headers)
                }
            }),
        );
        let redirect_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let redirect_base = format!("http://{}", redirect_listener.local_addr().unwrap());
        let redirect_task =
            tokio::spawn(async move { axum::serve(redirect_listener, redirect).await.unwrap() });

        let config = LlmConfig {
            subject: "shared".into(),
            id: "qwen".into(),
            base_url: redirect_base,
            model: "disposable".into(),
            api_key: "disposable-redirect-token".into(),
        };
        let client = outbound_http_client().unwrap();
        let response = completion_request(&client, &config, "select 1", &HeaderMap::new())
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(redirected_hits.load(Ordering::SeqCst), 0);
        redirect_task.abort();
        destination_task.abort();
    }

    #[test]
    fn helper_requests_preserve_conversation_affinity_without_forwarding_credentials() {
        let config = LlmConfig {
            subject: "alice".into(),
            id: "go".into(),
            base_url: "https://example.invalid/v1".into(),
            model: "deepseek-v4.1-flash".into(),
            api_key: "upstream-token".into(),
        };
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert("x-opencode-session", "conversation-123".parse().unwrap());
        headers.insert("cookie", "aster_session=private".parse().unwrap());
        let request = completion_request(&client, &config, "select 1", &headers)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request
                .headers()
                .get("x-opencode-session")
                .map(|s| s.to_str().unwrap()),
            Some("conversation-123")
        );
        assert_eq!(
            request.headers()["user-agent"],
            concat!("aster/", env!("CARGO_PKG_VERSION"))
        );
        assert!(!request.headers().contains_key("cookie"));
        assert_eq!(request.headers()["authorization"], "Bearer upstream-token");
        headers.insert("x-opencode-session", "not a session".parse().unwrap());
        assert!(completion_request(&client, &config, "select 1", &headers).is_err());
        let first = completion_request(&client, &config, "select 1", &HeaderMap::new())
            .unwrap()
            .build()
            .unwrap();
        let second = completion_request(&client, &config, "select 1", &HeaderMap::new())
            .unwrap()
            .build()
            .unwrap();
        assert_ne!(
            first.headers()["x-opencode-session"],
            second.headers()["x-opencode-session"]
        );
    }

    #[test]
    fn accepts_a_host_a_v1_base_or_a_full_endpoint() {
        assert_eq!(
            chat_endpoint("http://llm.local:11434/v1"),
            "http://llm.local:11434/v1/chat/completions"
        );
        assert_eq!(
            chat_endpoint("http://llm.local:4000"),
            "http://llm.local:4000/v1/chat/completions"
        );
        assert_eq!(
            chat_endpoint("http://llm.local:4000/v1/chat/completions"),
            "http://llm.local:4000/v1/chat/completions"
        );
    }

    #[test]
    fn refuses_url_embedded_credentials_before_building_a_request() {
        let config = LlmConfig {
            subject: "alice".into(),
            id: "unsafe".into(),
            base_url: "http://url-user:url-secret@127.0.0.1:9/v1".into(),
            model: "test".into(),
            api_key: "separate-token".into(),
        };
        assert!(completion_request(
            &reqwest::Client::new(),
            &config,
            "select 1",
            &HeaderMap::new(),
        )
        .is_err());
    }

    #[test]
    fn public_summary_hides_credentials_in_legacy_url() {
        let config = LlmConfig {
            subject: "alice".into(),
            id: "legacy".into(),
            base_url: "http://url-user:url-secret@example.invalid/v1".into(),
            model: "test".into(),
            api_key: "separate-token".into(),
        };
        let summary = serde_json::to_string(&LlmSummary::of(config)).unwrap();
        assert!(!summary.contains("url-user"));
        assert!(!summary.contains("url-secret"));
        assert!(!summary.contains("separate-token"));
    }
}
