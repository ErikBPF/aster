//! Per-user LLM endpoint registration and SQL generation.
//!
//! The token a user registers is stored server-side and never returned: requests
//! leave through this proxy so the browser and TUI never hold the credential.

use std::sync::Arc;

use aster_core::{authorize, Action, CoreError, LlmConfig, Principal};
use axum::extract::State;
use axum::http::HeaderMap;
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

#[derive(Serialize)]
pub(crate) struct LlmSummary {
    base_url: String,
    model: String,
}

/// The client learns whether an endpoint is registered and which one; the token
/// stays on the server.
pub(crate) async fn get_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Option<LlmSummary>>, ApiError> {
    let principal = principal(&state, &headers)?;
    let config = state.llm.get(&principal.subject).await?;
    Ok(Json(config.map(|config| LlmSummary {
        base_url: config.base_url,
        model: config.model,
    })))
}

pub(crate) async fn put_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<LlmBody>,
) -> Result<Json<LlmSummary>, ApiError> {
    let principal = principal(&state, &headers)?;
    state.llm.put(store_config(&principal, body)).await?;
    let config = state
        .llm
        .get(&principal.subject)
        .await?
        .ok_or_else(|| CoreError::Storage("llm configuration did not persist".into()))?;
    Ok(Json(LlmSummary {
        base_url: config.base_url,
        model: config.model,
    }))
}

/// Browsers cannot PUT from a plain form, so the settings page posts here.
pub(crate) async fn put_config_form(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Form(body): axum::extract::Form<LlmBody>,
) -> Result<axum::response::Response, ApiError> {
    let principal = principal(&state, &headers)?;
    state.llm.put(store_config(&principal, body)).await?;
    Ok(axum::response::Redirect::to("/settings/llm").into_response())
}

fn store_config(principal: &Principal, body: LlmBody) -> LlmConfig {
    LlmConfig {
        subject: principal.subject.clone(),
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
}

#[derive(Serialize)]
pub(crate) struct Completion {
    sql: String,
}

const SYSTEM_PROMPT: &str = "You write Trino SQL. Answer with one SQL statement and no prose.";

/// Proxies one completion against the caller's own OpenAI-compatible endpoint.
pub(crate) async fn generate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AiBody>,
) -> Result<Json<Completion>, ApiError> {
    let principal = principal(&state, &headers)?;
    authorize(&principal, Action::RunQuery)?;

    let config = state
        .llm
        .get(&principal.subject)
        .await?
        .ok_or_else(|| CoreError::NotFound("no llm endpoint registered".into()))?;

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

    let response = state
        .http
        .post(chat_endpoint(&config.base_url))
        .bearer_auth(&config.api_key)
        .json(&serde_json::json!({
            "model": config.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": prompt},
            ],
        }))
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
    use super::chat_endpoint;

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
