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
    base_url: String,
    model: String,
}

impl LlmSummary {
    fn of(config: LlmConfig) -> Self {
        Self {
            id: config.id,
            base_url: config.base_url,
            model: config.model,
        }
    }
}

pub(crate) async fn list_configs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<LlmSummary>>, ApiError> {
    let principal = principal(&state, &headers).await?;
    let configs = state.llm.list(&principal.subject).await?;
    Ok(Json(configs.into_iter().map(LlmSummary::of).collect()))
}

pub(crate) async fn put_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<LlmBody>,
) -> Result<Json<LlmSummary>, ApiError> {
    let principal = principal(&state, &headers).await?;
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

/// Proxies one completion against one of the caller's own OpenAI-compatible endpoints.
pub(crate) async fn generate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AiBody>,
) -> Result<Json<Completion>, ApiError> {
    let principal = principal(&state, &headers).await?;
    authorize(&principal, Action::RunQuery)?;

    let config = helper(&state, &principal.subject, body.helper.as_deref()).await?;

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

fn completion_request(
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
        .post(chat_endpoint(&config.base_url))
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

/// The named helper, or the first one the subject registered.
async fn helper(
    state: &AppState,
    subject: &str,
    wanted: Option<&str>,
) -> Result<LlmConfig, ApiError> {
    if let Some(id) = wanted.map(str::trim).filter(|id| !id.is_empty()) {
        return state
            .llm
            .get(subject, id)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("helper {id} is not registered")).into());
    }
    state
        .llm
        .list(subject)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| CoreError::NotFound("no llm endpoint registered".into()).into())
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
