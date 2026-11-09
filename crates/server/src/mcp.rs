//! MCP registration boundaries. Remote execution is separately authorized.
use crate::{AppState, DestinationPolicy, Keyring};
use aster_core::{
    authorize,
    mcp::{McpConnection, McpRecord, McpStore},
    Action, AdminSharedModelSummary, CoreError, EncryptedSharedModel, Result,
};
use axum::http::HeaderMap;
use std::sync::Arc;

pub struct McpRegistry {
    store: Arc<dyn McpStore>,
    destinations: DestinationPolicy,
    keys: Keyring,
    development_endpoint: Option<String>,
    transport: Arc<dyn aster_core::mcp::McpTransport>,
}
fn summary(c: &McpConnection) -> AdminSharedModelSummary {
    AdminSharedModelSummary {
        id: format!("mcp/{}", c.id),
        base_url: c.url.clone(),
        model: c.auth.clone(),
    }
}
impl McpRegistry {
    pub fn new(
        store: Arc<dyn McpStore>,
        destinations: DestinationPolicy,
        keys: Keyring,
        transport: Arc<dyn aster_core::mcp::McpTransport>,
    ) -> Self {
        Self {
            store,
            destinations,
            keys,
            development_endpoint: None,
            transport,
        }
    }
    pub fn with_development_endpoint(mut self, url: String) -> Result<Self> {
        let parsed = reqwest::Url::parse(&url)
            .map_err(|_| CoreError::Invalid("invalid mock endpoint".into()))?;
        if parsed.scheme() != "http"
            || parsed.host_str() != Some("127.0.0.1")
            || parsed.path() != "/mcp"
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(CoreError::Invalid(
                "mock MCP must use loopback HTTP /mcp".into(),
            ));
        }
        self.development_endpoint = Some(url);
        Ok(self)
    }
    fn destination(&self, state: &AppState, url: &str) -> Result<()> {
        if state.dev_login
            && state.identity.is_none()
            && self.development_endpoint.as_deref() == Some(url)
        {
            return Ok(());
        }
        self.destinations
            .validate(url)
            .map_err(|_| CoreError::Invalid("MCP endpoint origin is not approved".into()))
    }
    fn token(&self, r: &McpRecord) -> Result<String> {
        self.keys.decrypt(&EncryptedSharedModel {
            summary: summary(&r.connection),
            key_version: r.key_version.clone(),
            nonce: r.nonce.clone(),
            ciphertext: r.ciphertext.clone(),
        })
    }
    pub async fn verify_keys(&self) -> Result<()> {
        for row in self.store.list().await? {
            self.token(&row)?;
        }
        Ok(())
    }
}

pub async fn list(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(bool, bool, Vec<McpConnection>)> {
    let principal = crate::personal_models::current(state, headers).await?;
    authorize(&principal, Action::ReadNotebook)?;
    let admin = authorize(&principal, Action::Administer).is_ok();
    let Some(registry) = &state.mcp else {
        return Ok((false, admin, vec![]));
    };
    let mut visible = Vec::new();
    for row in registry.store.list().await? {
        let mut c = row.connection;
        if admin
            || (c.enabled
                && (c.subjects.contains(&principal.subject)
                    || c.teams
                        .iter()
                        .any(|team| crate::personal_models::team_member(state, &principal, team))))
        {
            if !admin {
                c.url.clear();
                c.subjects.clear();
                c.teams.clear();
                c.auth.clear();
            }
            visible.push(c);
        }
    }
    Ok((true, admin, visible))
}

pub async fn put(
    state: &AppState,
    headers: &HeaderMap,
    mut c: McpConnection,
    expected: i64,
    token: String,
) -> Result<McpConnection> {
    let principal = crate::personal_models::current(state, headers).await?;
    authorize(&principal, Action::Administer)?;
    let registry = state
        .mcp
        .as_ref()
        .ok_or_else(|| CoreError::Invalid("MCP registration is not configured".into()))?;
    c.revision = expected
        .checked_add(1)
        .ok_or_else(|| CoreError::Invalid("invalid MCP revision".into()))?;
    c.subjects.sort();
    c.subjects.dedup();
    c.teams.sort();
    c.teams.dedup();
    c.validate()?;
    registry.destination(state, &c.url)?;
    if let Some(old) = registry.store.get(&c.id).await? {
        if old.connection.url == c.url && old.connection.auth == c.auth && token.is_empty() {
            c.approved_reads = old.connection.approved_reads;
        }
    }
    for team in &c.teams {
        let exists = state.team_git_targets.as_ref().map_or_else(
            || {
                state
                    .team_workspaces
                    .as_ref()
                    .is_some_and(|r| r.contains_team(team))
            },
            |r| r.contains_team(team),
        );
        if !exists {
            return Err(CoreError::Invalid("unknown MCP team".into()));
        }
    }
    let token = match c.auth.as_str() {
        "none" if token.is_empty() => String::new(),
        "bearer"
            if !token.is_empty() && token.len() <= 8192 && !token.chars().any(char::is_control) =>
        {
            token
        }
        "bearer" if token.is_empty() && expected > 0 => {
            let old = registry
                .store
                .get(&c.id)
                .await?
                .ok_or_else(|| CoreError::Conflict("MCP connection unavailable".into()))?;
            if old.connection.url != c.url || old.connection.auth != c.auth {
                return Err(CoreError::Invalid(
                    "enter a new token when changing the endpoint".into(),
                ));
            }
            registry.token(&old)?
        }
        _ => return Err(CoreError::Invalid("invalid MCP authentication".into())),
    };
    let encrypted = registry.keys.encrypt(summary(&c), &token)?;
    registry
        .store
        .put(
            McpRecord {
                connection: c.clone(),
                key_version: encrypted.key_version,
                nonce: encrypted.nonce,
                ciphertext: encrypted.ciphertext,
            },
            expected,
            &principal.subject,
        )
        .await?;
    Ok(c)
}

pub async fn discover(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
) -> Result<Vec<(aster_core::mcp::McpTool, String, bool)>> {
    let (_principal, registry, record) = admitted(state, headers, id).await?;
    let tools = registry
        .transport
        .discover(&record.connection.url, &registry.token(&record)?)
        .await?;
    Ok(tools
        .into_iter()
        .map(|tool| {
            let hash = fingerprint(&serde_json::to_value(&tool).expect("serializable tool"));
            let approved = record.connection.approved_reads.get(&tool.name) == Some(&hash);
            (tool, hash, approved)
        })
        .collect())
}
pub(crate) fn fingerprint(value: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.to_string()))
}
async fn admitted<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    id: &str,
) -> Result<(aster_core::Principal, &'a McpRegistry, McpRecord)> {
    let p = crate::personal_models::current(state, headers).await?;
    authorize(&p, Action::ReadNotebook)?;
    let registry = state
        .mcp
        .as_deref()
        .ok_or_else(|| CoreError::Unauthorized("MCP unavailable".into()))?;
    let r = registry
        .store
        .get(id)
        .await?
        .ok_or_else(|| CoreError::Unauthorized("MCP connection unavailable".into()))?;
    if !r.connection.enabled
        || !(authorize(&p, Action::Administer).is_ok()
            || r.connection.subjects.contains(&p.subject)
            || r.connection
                .teams
                .iter()
                .any(|team| crate::personal_models::team_member(state, &p, team)))
    {
        return Err(CoreError::Unauthorized("MCP connection unavailable".into()));
    }
    registry.destination(state, &r.connection.url)?;
    Ok((p, registry, r))
}
pub async fn approve_read(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    expected: i64,
    tool: &str,
    hash: &str,
    approved: bool,
) -> Result<McpConnection> {
    let (p, registry, mut record) = admitted(state, headers, id).await?;
    authorize(&p, Action::Administer)?;
    let tools = discover(state, headers, id).await?;
    if !tools.iter().any(|(t, h, _)| t.name == tool && h == hash) {
        return Err(CoreError::Conflict(
            "tool changed; refresh before approving".into(),
        ));
    }
    if approved {
        record
            .connection
            .approved_reads
            .insert(tool.into(), hash.into());
    } else {
        record.connection.approved_reads.remove(tool);
    }
    record.connection.revision = expected
        .checked_add(1)
        .ok_or_else(|| CoreError::Invalid("invalid MCP revision".into()))?;
    let connection = record.connection.clone();
    registry.store.put(record, expected, &p.subject).await?;
    Ok(connection)
}
pub struct CallResult {
    pub confirmation: String,
    pub result: String,
}
pub async fn call(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    tool: &str,
    arguments: &str,
    confirmation: &str,
) -> Result<CallResult> {
    call_scoped(
        state,
        headers,
        id,
        tool,
        arguments,
        confirmation,
        "explorer",
    )
    .await
}

pub(crate) async fn call_scoped(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    tool: &str,
    arguments: &str,
    confirmation: &str,
    scope: &str,
) -> Result<CallResult> {
    let (p, registry, record) = admitted(state, headers, id).await?;
    if arguments.len() > 32768 {
        return Err(CoreError::Invalid("tool arguments exceed 32 KiB".into()));
    }
    let arguments: serde_json::Value = serde_json::from_str(arguments)
        .map_err(|_| CoreError::Invalid("invalid tool arguments JSON".into()))?;
    if !arguments.is_object() {
        return Err(CoreError::Invalid(
            "tool arguments must be an object".into(),
        ));
    }
    let tools = discover(state, headers, id).await?;
    let (definition, hash, approved) = tools
        .into_iter()
        .find(|(t, _, _)| t.name == tool)
        .ok_or_else(|| CoreError::Invalid("tool unavailable".into()))?;
    let schema = definition.input_schema;
    let input = arguments.clone();
    tokio::task::spawn_blocking(move || {
        let validator = jsonschema::validator_for(&schema)
            .map_err(|_| CoreError::Invalid("unsupported tool schema".into()))?;
        if !validator.is_valid(&input) {
            return Err(CoreError::Invalid(
                "arguments do not match tool schema".into(),
            ));
        }
        Ok(())
    })
    .await
    .map_err(|_| CoreError::Storage("tool validation unavailable".into()))??;
    let binding = fingerprint(&serde_json::json!([
        p.subject,
        scope,
        id,
        record.connection.revision,
        tool,
        hash,
        arguments
    ]));
    if !approved {
        if confirmation.is_empty() {
            let nonce = aster_core::new_sid()?;
            registry
                .store
                .remember(&nonce, &p.subject, &binding, crate::now() + 300)
                .await?;
            return Ok(CallResult {
                confirmation: nonce,
                result: String::new(),
            });
        }
        if !registry
            .store
            .consume(confirmation, &p.subject, &binding, crate::now())
            .await?
        {
            return Err(CoreError::Unauthorized(
                "tool confirmation expired, changed or already used".into(),
            ));
        }
    } else if !confirmation.is_empty() {
        return Err(CoreError::Conflict(
            "tool policy changed; request the call again".into(),
        ));
    }
    // Recheck current authority and connection generation immediately before execution.
    let (_, _, current) = admitted(state, headers, id).await?;
    if current.connection.revision != record.connection.revision {
        return Err(CoreError::Conflict(
            "MCP connection changed; request the call again".into(),
        ));
    }
    let result = registry
        .transport
        .call(
            &record.connection.url,
            &registry.token(&record)?,
            tool,
            &hash,
            arguments,
            &CurrentCall {
                state,
                headers,
                id,
                revision: record.connection.revision,
            },
        )
        .await?;
    Ok(CallResult {
        confirmation: String::new(),
        result: result.to_string(),
    })
}

struct CurrentCall<'a> {
    state: &'a AppState,
    headers: &'a HeaderMap,
    id: &'a str,
    revision: i64,
}
#[async_trait::async_trait]
impl aster_core::mcp::McpCallGuard for CurrentCall<'_> {
    async fn check(&self) -> Result<()> {
        let (_, _, current) = admitted(self.state, self.headers, self.id).await?;
        if current.connection.revision != self.revision {
            return Err(CoreError::Conflict(
                "MCP connection changed; request the call again".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) async fn check_access(state: &AppState, headers: &HeaderMap, id: &str) -> Result<()> {
    admitted(state, headers, id).await.map(|_| ())
}
