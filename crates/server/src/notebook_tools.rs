//! Bounded tool turns. Every external operation is reserved durably before dispatch.
use crate::AppState;
use aster_core::{
    conversation::{NotebookTool, NotebookToolCall, ToolTurn},
    Conversation, CoreError, Principal, Result,
};
use axum::http::HeaderMap;
use serde_json::{json, Value};

pub(crate) async fn check_history(
    state: &AppState,
    headers: &HeaderMap,
    conversation: &Conversation,
) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    for message in conversation
        .messages
        .iter()
        .chain(conversation.pending.iter().map(|p| &p.user))
    {
        ids.extend(message.mcp_connections.iter());
    }
    if ids.len() > 32 {
        return Err(CoreError::Invalid(
            "too many MCP sources; start a new session".into(),
        ));
    }
    for id in ids {
        crate::mcp::check_access(state, headers, id)
            .await
            .map_err(|_| {
                CoreError::Unauthorized("MCP context unavailable; start a new session".into())
            })?;
    }
    Ok(())
}

pub(crate) async fn definitions(
    state: &AppState,
    headers: &HeaderMap,
    ids: &[String],
) -> Result<Vec<NotebookTool>> {
    let mut seen = std::collections::HashSet::new();
    if ids.len() > 4
        || ids
            .iter()
            .any(|id| !aster_core::llm::valid_id(id) || !seen.insert(id))
    {
        return Err(CoreError::Invalid(
            "select at most 4 distinct MCP connections".into(),
        ));
    }
    let mut tools = Vec::new();
    for id in ids {
        for (tool, _, _) in crate::mcp::discover(state, headers, id).await? {
            if tools.len() >= 32 {
                return Err(CoreError::Invalid(
                    "selected MCP connections exceed 32 tools".into(),
                ));
            }
            let name = format!("mcp_{}", tools.len());
            let definition = json!({"type":"function","function":{"name":name,"description":format!("{} / {}: {}",id,tool.name,tool.description),"parameters":tool.input_schema}});
            tools.push(NotebookTool {
                name,
                connection: id.clone(),
                tool: tool.name,
                definition,
            });
        }
    }
    crate::ai_context::serialize(&tools, 64 * 1024)?;
    Ok(tools)
}

async fn save(
    state: &AppState,
    p: &Principal,
    key: &str,
    conversation: Conversation,
) -> Result<Conversation> {
    if let Some(pending) = &conversation.pending {
        crate::ai_context::serialize(pending, 256 * 1024)?;
    }
    state
        .conversations
        .save_turn(&p.subject, key, conversation.revision, conversation)
        .await
}

async fn authorize_pending(
    state: &AppState,
    headers: &HeaderMap,
    conversation: &Conversation,
) -> Result<()> {
    check_history(state, headers, conversation).await?;
    let mut context = conversation.clone();
    if let Some(pending) = &conversation.pending {
        context.messages.push(pending.user.clone());
    }
    crate::ai_context::history(state, headers, &context).await?;
    crate::conversations::skill_context(state, headers, &context, None, Some(&[])).await?;
    Ok(())
}

async fn finish(
    state: &AppState,
    p: &Principal,
    key: &str,
    mut conversation: Conversation,
    content: String,
) -> Result<Conversation> {
    let turn = conversation
        .pending
        .take()
        .ok_or_else(|| CoreError::Conflict("no pending tool turn".into()))?;
    let mut assistant = turn.user.clone();
    assistant.role = "assistant".into();
    assistant.content = content;
    let expected = conversation.revision;
    let mut next = conversation.with_exchange(expected, turn.user, assistant)?;
    crate::ai_context::check_dependency_limits(&next)?;
    next.revision = expected;
    save(state, p, key, next).await
}

pub(crate) async fn start(
    state: &AppState,
    headers: &HeaderMap,
    p: &Principal,
    key: &str,
    mut conversation: Conversation,
    pending: ToolTurn,
) -> Result<Conversation> {
    conversation.pending = Some(pending);
    let reserved = save(state, p, key, conversation).await?;
    drive(state, headers, p, key, reserved).await
}

pub(crate) async fn resume(
    state: &AppState,
    headers: &HeaderMap,
    p: &Principal,
    key: &str,
    mut conversation: Conversation,
    confirmation: &str,
    cancel: bool,
) -> Result<Conversation> {
    let pending = conversation
        .pending
        .as_ref()
        .ok_or_else(|| CoreError::Conflict("no pending tool turn".into()))?;
    if cancel {
        if matches!(pending.phase.as_str(), "model" | "executing" | "ready")
            && pending.expires_at > crate::now()
        {
            return Err(CoreError::Conflict(
                "tool turn is running; reload its status".into(),
            ));
        }
        return finish(state,p,key,conversation,"Tool turn cancelled. Any actions already executed are not undone. If an operation was interrupted, verify its outcome before retrying.".into()).await;
    }
    if pending.phase != "confirmation"
        || pending.expires_at <= crate::now()
        || confirmation.is_empty()
        || pending
            .call
            .as_ref()
            .is_none_or(|c| c.confirmation != confirmation)
    {
        return Err(CoreError::Conflict(
            "confirmation expired or turn is not awaiting this action; reload its status".into(),
        ));
    }
    authorize_pending(state, headers, &conversation).await?;
    conversation.pending.as_mut().unwrap().phase = "executing".into();
    let reserved = save(state, p, key, conversation).await?;
    let next = dispatch(state, headers, p, key, reserved).await?;
    if next.pending.as_ref().is_some_and(|t| t.phase == "ready") {
        drive(state, headers, p, key, next).await
    } else {
        Ok(next)
    }
}

async fn failed(
    state: &AppState,
    p: &Principal,
    key: &str,
    mut conversation: Conversation,
    error: CoreError,
) -> Result<Conversation> {
    tracing::warn!(%error,"notebook tool turn stopped");
    // Never retry an operation whose dispatch or persistence outcome is unknown.
    conversation.pending.as_mut().unwrap().phase = "failed".into();
    save(state, p, key, conversation).await
}

async fn dispatch(
    state: &AppState,
    headers: &HeaderMap,
    p: &Principal,
    key: &str,
    mut conversation: Conversation,
) -> Result<Conversation> {
    let call = conversation
        .pending
        .as_ref()
        .unwrap()
        .call
        .as_ref()
        .unwrap()
        .clone();
    let scope = json!([
        "notebook",
        key,
        conversation.id,
        conversation.pending.as_ref().unwrap().id
    ])
    .to_string();
    let response = crate::mcp::call_scoped(
        state,
        headers,
        &call.connection,
        &call.tool,
        &call.arguments,
        &call.confirmation,
        &scope,
    )
    .await;
    let result = match response {
        Ok(result) => result,
        Err(error) => return failed(state, p, key, conversation, error).await,
    };
    let turn = conversation.pending.as_mut().unwrap();
    if !result.confirmation.is_empty() {
        turn.phase = "confirmation".into();
        turn.call.as_mut().unwrap().confirmation = result.confirmation;
    } else {
        if result.result.len() > 32 * 1024 {
            return failed(
                state,
                p,
                key,
                conversation,
                CoreError::Invalid("tool result exceeds notebook limit".into()),
            )
            .await;
        }
        turn.messages
            .push(json!({"role":"tool","tool_call_id":call.call_id,"content":result.result}));
        turn.calls += 1;
        turn.call = None;
        turn.phase = "ready".into();
    }
    save(state, p, key, conversation).await
}

async fn drive(
    state: &AppState,
    headers: &HeaderMap,
    p: &Principal,
    key: &str,
    mut conversation: Conversation,
) -> Result<Conversation> {
    loop {
        let turn = conversation.pending.as_ref().unwrap();
        if turn.expires_at <= crate::now() {
            return failed(
                state,
                p,
                key,
                conversation,
                CoreError::Invalid("tool turn expired".into()),
            )
            .await;
        }
        if let Err(error) = authorize_pending(state, headers, &conversation).await {
            return failed(state, p, key, conversation, error).await;
        }
        let config =
            match crate::ai::resolve_helper(state, headers, p, Some(&turn.user.helper)).await {
                Ok(c) => c,
                Err(error) => return failed(state, p, key, conversation, error).await,
            };
        let mut request = json!({"model":config.model,"messages":turn.messages});
        if !turn.tools.is_empty() {
            request["tools"] = json!(turn.tools.iter().map(|t| &t.definition).collect::<Vec<_>>());
            request["parallel_tool_calls"] = json!(false);
            if turn.calls >= 4 {
                request["tool_choice"] = json!("none");
            }
        }
        let payload = match crate::ai_context::serialize(&request, crate::ai_context::REQUEST_BYTES)
        {
            Ok(p) => p,
            Err(error) => return failed(state, p, key, conversation, error).await,
        };
        conversation.pending.as_mut().unwrap().phase = "model".into();
        conversation = save(state, p, key, conversation).await?;
        let result = async {
            let response = crate::ai::completion_request(&state.http, &config, "")
                .map_err(|e| e.0)?
                .timeout(std::time::Duration::from_secs(30))
                .body(payload)
                .send()
                .await
                .map_err(|e| CoreError::Storage(e.to_string()))?;
            crate::ai::read_completion_message(response).await
        }
        .await;
        let message = match result {
            Ok(m) => m,
            Err(error) => return failed(state, p, key, conversation, error).await,
        };
        if let Some(calls) = message
            .get("tool_calls")
            .and_then(Value::as_array)
            .filter(|c| !c.is_empty())
        {
            let turn = conversation.pending.as_mut().unwrap();
            if calls.len() != 1 || turn.calls >= 4 {
                return failed(
                    state,
                    p,
                    key,
                    conversation,
                    CoreError::Invalid("tool call limit exceeded".into()),
                )
                .await;
            }
            let call = &calls[0];
            let Some(tool) = turn
                .tools
                .iter()
                .find(|t| Some(t.name.as_str()) == call["function"]["name"].as_str())
            else {
                return failed(
                    state,
                    p,
                    key,
                    conversation,
                    CoreError::Invalid("unselected tool".into()),
                )
                .await;
            };
            let Some(arguments) = call["function"]["arguments"]
                .as_str()
                .filter(|v| v.len() <= 32768)
            else {
                return failed(
                    state,
                    p,
                    key,
                    conversation,
                    CoreError::Invalid("invalid tool arguments".into()),
                )
                .await;
            };
            let Some(call_id) = call["id"]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() <= 128)
            else {
                return failed(
                    state,
                    p,
                    key,
                    conversation,
                    CoreError::Invalid("invalid tool call id".into()),
                )
                .await;
            };
            turn.call = Some(NotebookToolCall {
                connection: tool.connection.clone(),
                tool: tool.tool.clone(),
                arguments: arguments.into(),
                call_id: call_id.into(),
                confirmation: String::new(),
            });
            // Store only the documented message fields; no provider-specific continuation state.
            turn.messages
                .push(json!({"role":"assistant","content":message["content"],"tool_calls":calls}));
            turn.phase = "executing".into();
            conversation = save(state, p, key, conversation).await?;
            conversation = dispatch(state, headers, p, key, conversation).await?;
            if conversation.pending.as_ref().unwrap().phase != "ready" {
                return Ok(conversation);
            }
        } else {
            let Some(text) = message["content"]
                .as_str()
                .filter(|s| !s.trim().is_empty() && s.len() <= 32768)
            else {
                return failed(
                    state,
                    p,
                    key,
                    conversation,
                    CoreError::Invalid("invalid helper reply".into()),
                )
                .await;
            };
            return finish(state, p, key, conversation, text.into()).await;
        }
    }
}
