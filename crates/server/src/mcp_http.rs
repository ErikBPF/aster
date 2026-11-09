//! Bounded Streamable HTTP MCP transport. No redirects or automatic call retries.
use aster_core::{
    mcp::{McpTool, McpTransport},
    CoreError, Result,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{collections::HashSet, time::Duration};
#[derive(Default)]
pub struct HttpMcp;
const LIMIT: usize = 256 * 1024;
fn invalid(message: &str) -> CoreError {
    CoreError::Invalid(message.into())
}
fn upstream(_: impl std::fmt::Display) -> CoreError {
    CoreError::Storage("MCP request failed; its outcome may be unknown. Do not automatically retry changing actions.".into())
}
struct Session {
    http: reqwest::Client,
    url: String,
    token: String,
    session: Option<String>,
    next: u64,
}
impl Session {
    fn request(&self) -> reqwest::RequestBuilder {
        let mut request = self
            .http
            .post(&self.url)
            .header("accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-03-26");
        if !self.token.is_empty() {
            request = request.bearer_auth(&self.token);
        }
        if let Some(id) = &self.session {
            request = request.header("Mcp-Session-Id", id);
        }
        request
    }
    fn new(url: &str, token: &str) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(10))
                .build()
                .map_err(upstream)?,
            url: url.into(),
            token: token.into(),
            session: None,
            next: 1,
        })
    }
    async fn initialize(&mut self) -> Result<()> {
        let initialized=self.rpc("initialize",json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"aster","version":env!("CARGO_PKG_VERSION")}})).await?;
        if initialized["protocolVersion"] != "2025-03-26"
            || !initialized["capabilities"]["tools"].is_object()
        {
            return Err(invalid(
                "MCP server must support tools and protocol 2025-03-26",
            ));
        }
        let response = self
            .request()
            .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .send()
            .await
            .map_err(upstream)?;
        if !response.status().is_success() {
            return Err(upstream("initialization notification refused"));
        }
        Ok(())
    }
    async fn close(&self) {
        if let Some(id) = &self.session {
            let mut request = self
                .http
                .delete(&self.url)
                .header("Mcp-Session-Id", id)
                .header("MCP-Protocol-Version", "2025-03-26")
                .timeout(Duration::from_millis(500));
            if !self.token.is_empty() {
                request = request.bearer_auth(&self.token);
            }
            let _ = request.send().await;
        }
    }
    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        let mut response = self
            .request()
            .json(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .send()
            .await
            .map_err(upstream)?;
        if !response.status().is_success() {
            return Err(upstream("server refused request"));
        }
        if let Some(value) = response.headers().get("Mcp-Session-Id") {
            let value = value.to_str().map_err(upstream)?;
            if value.is_empty()
                || value.len() > 512
                || !value.bytes().all(|b| (0x21..=0x7e).contains(&b))
            {
                return Err(invalid("invalid MCP session identifier"));
            }
            if self.session.as_deref().is_some_and(|old| old != value) {
                return Err(invalid("MCP session changed unexpectedly"));
            }
            self.session = Some(value.into());
        }
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let sse = mime.starts_with("text/event-stream");
        if !sse && !mime.starts_with("application/json") {
            return Err(invalid("unsupported MCP response format"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(upstream)? {
            if chunk.len() > LIMIT - bytes.len() {
                return Err(invalid("MCP response exceeds 256 KiB"));
            }
            bytes.extend_from_slice(&chunk);
            if sse {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    let normalized = text.replace("\r\n", "\n");
                    let events: Vec<_> = normalized.split("\n\n").collect();
                    for event in events.iter().take(events.len().saturating_sub(1)) {
                        let data = event
                            .lines()
                            .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                            .collect::<Vec<_>>()
                            .join("\n");
                        if data.is_empty() {
                            continue;
                        }
                        let value: Value = serde_json::from_str(&data)
                            .map_err(|_| invalid("malformed MCP event"))?;
                        if let Some(result) = response_result(value, id)? {
                            return Ok(result);
                        }
                    }
                }
            }
        }
        if sse {
            return Err(invalid("MCP event stream ended before its response"));
        }
        let value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("malformed MCP response"))?;
        response_result(value, id)?.ok_or_else(|| invalid("MCP response identifier mismatch"))
    }
    async fn tools(&mut self) -> Result<Vec<McpTool>> {
        let mut tools = Vec::new();
        let mut names = HashSet::new();
        let mut cursors = HashSet::new();
        let mut cursor = None;
        let mut bytes = 0;
        for _ in 0..8 {
            let result = self
                .rpc(
                    "tools/list",
                    cursor.as_ref().map_or(json!({}), |c| json!({"cursor":c})),
                )
                .await?;
            let page = result["tools"]
                .as_array()
                .ok_or_else(|| invalid("invalid MCP tools list"))?;
            for value in page {
                bytes += value.to_string().len();
                if tools.len() >= 100 || bytes > LIMIT {
                    return Err(invalid("MCP tool catalog exceeds limits"));
                }
                let tool: McpTool = serde_json::from_value(value.clone())
                    .map_err(|_| invalid("invalid MCP tool definition"))?;
                if tool.name.is_empty()
                    || tool.name.len() > 128
                    || !tool
                        .name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                    || !tool.input_schema.is_object()
                    || !names.insert(tool.name.clone())
                {
                    return Err(invalid("invalid or duplicate MCP tool"));
                }
                tools.push(tool);
            }
            match result.get("nextCursor").filter(|c| !c.is_null()) {
                None => return Ok(tools),
                Some(value) => {
                    let value = value
                        .as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 2048)
                        .ok_or_else(|| invalid("invalid MCP cursor"))?;
                    if !cursors.insert(value.to_owned()) {
                        return Err(invalid("MCP pagination repeated a cursor"));
                    }
                    cursor = Some(value.to_owned());
                }
            }
        }
        Err(invalid("MCP tool catalog exceeds eight pages"))
    }
}
fn response_result(value: Value, id: u64) -> Result<Option<Value>> {
    if value["jsonrpc"] != "2.0" {
        return Err(invalid("invalid MCP JSON-RPC version"));
    }
    if value.get("method").is_some() {
        if value.get("id").is_some() {
            return Err(invalid("MCP server requests are not supported"));
        }
        return Ok(None);
    }
    if value["id"] != id {
        return Ok(None);
    }
    if value.get("error").is_some() {
        return Err(upstream("MCP protocol error"));
    }
    value
        .get("result")
        .cloned()
        .map(Some)
        .ok_or_else(|| invalid("MCP response has no result"))
}
#[async_trait]
impl McpTransport for HttpMcp {
    async fn discover(&self, url: &str, token: &str) -> Result<Vec<McpTool>> {
        let mut session = Session::new(url, token)?;
        let result = tokio::time::timeout(Duration::from_secs(20), async {
            session.initialize().await?;
            session.tools().await
        })
        .await
        .map_err(|_| upstream("timeout"));
        session.close().await;
        result?
    }
    async fn call(
        &self,
        url: &str,
        token: &str,
        tool: &str,
        schema_hash: &str,
        arguments: Value,
        guard: &dyn aster_core::mcp::McpCallGuard,
    ) -> Result<Value> {
        let mut session = Session::new(url, token)?;
        let mut dispatched = false;
        let result = tokio::time::timeout(Duration::from_secs(20), async {
            session.initialize().await?;
            let tools = session.tools().await?;
            let current = tools
                .into_iter()
                .find(|t| t.name == tool)
                .ok_or_else(|| invalid("MCP tool disappeared"))?;
            if crate::mcp::fingerprint(&serde_json::to_value(current).map_err(upstream)?)
                != schema_hash
            {
                return Err(CoreError::Conflict(
                    "MCP tool changed; rediscover before calling".into(),
                ));
            }
            guard.check().await?;
            dispatched = true;
            session
                .rpc("tools/call", json!({"name":tool,"arguments":arguments}))
                .await
                .map_err(|_| unknown_outcome())
        })
        .await;
        session.close().await;
        result.unwrap_or_else(|_| {
            Err(if dispatched {
                unknown_outcome()
            } else {
                upstream("timeout")
            })
        })
    }
}
fn unknown_outcome() -> CoreError {
    CoreError::Conflict("MCP action outcome is unknown. Check the remote system before retrying; the action may already have completed.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        response::IntoResponse,
        routing::{delete, post},
        Json, Router,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Allowed;
    #[async_trait]
    impl aster_core::mcp::McpCallGuard for Allowed {
        async fn check(&self) -> Result<()> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn failed_initialization_releases_session_and_uncertain_calls_are_explicit() {
        let closed = Arc::new(AtomicUsize::new(0));
        let deletions = closed.clone();
        let router=Router::new().route("/invalid",post(||async { ([("mcp-session-id","fixture-session")],Json(json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"unsupported","capabilities":{}}}))) }).merge(delete(move || {let n=deletions.clone();async move {n.fetch_add(1,Ordering::SeqCst);axum::http::StatusCode::OK}})))
            .route("/mcp",post(|Json(body):Json<Value>|async move {
                let result=match body["method"].as_str().unwrap_or("") {
                    "initialize"=>json!({"protocolVersion":"2025-03-26","capabilities":{"tools":{}}}),
                    "tools/list"=>json!({"tools":[{"name":"read","description":"Read","inputSchema":{"type":"object"}}]}),
                    "tools/call"=>return ([("content-type","application/json")],"truncated result").into_response(),
                    _=>json!({})
                }; Json(json!({"jsonrpc":"2.0","id":body["id"],"result":result})).into_response()
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        assert!(HttpMcp
            .discover(&format!("{origin}/invalid"), "")
            .await
            .is_err());
        assert_eq!(
            closed.load(Ordering::SeqCst),
            1,
            "failed initialization must delete its session"
        );
        let tools = HttpMcp
            .discover(&format!("{origin}/mcp"), "")
            .await
            .unwrap();
        let hash = crate::mcp::fingerprint(&serde_json::to_value(&tools[0]).unwrap());
        let error = HttpMcp
            .call(
                &format!("{origin}/mcp"),
                "",
                "read",
                &hash,
                json!({}),
                &Allowed,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error,CoreError::Conflict(ref text) if text.contains("outcome is unknown") && text.contains("before retrying"))
        );
        server.abort();
    }
}
