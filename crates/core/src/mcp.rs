//! Admin-owned remote tool connections. Skill content never grants access.
use crate::{CoreError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::RwLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpConnection {
    pub id: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub auth: String,
    pub enabled: bool,
    pub revision: i64,
    pub subjects: Vec<String>,
    pub teams: Vec<String>,
    #[serde(default)]
    pub approved_reads: BTreeMap<String, String>,
}
impl McpConnection {
    pub fn validate(&self) -> Result<()> {
        if !crate::llm::valid_id(&self.id)
            || self.name.trim().is_empty()
            || self.name.len() > 160
            || self.description.len() > 512
            || self.url.len() > 2048
            || !matches!(self.auth.as_str(), "none" | "bearer")
            || self.revision < 1
            || self.subjects.len() + self.teams.len() > 200
            || self
                .subjects
                .iter()
                .any(|s| s.is_empty() || s.len() > 512 || s.chars().any(char::is_control))
            || self.teams.iter().any(|s| !crate::llm::valid_id(s))
            || self.name.chars().any(char::is_control)
            || self.description.contains('\0')
        {
            return Err(CoreError::Invalid("invalid MCP connection".into()));
        }
        Ok(())
    }
}

// Ciphertext is stored separately from the public metadata and never returned by an API.
#[derive(Clone, Serialize, Deserialize)]
pub struct McpRecord {
    pub connection: McpConnection,
    pub key_version: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}
#[async_trait]
pub trait McpStore: Send + Sync {
    async fn list(&self) -> Result<Vec<McpRecord>>;
    async fn get(&self, id: &str) -> Result<Option<McpRecord>>;
    async fn put(&self, record: McpRecord, expected: i64, actor: &str) -> Result<()>;
    async fn remember(
        &self,
        id: &str,
        subject: &str,
        fingerprint: &str,
        expires: i64,
    ) -> Result<()>;
    async fn consume(&self, id: &str, subject: &str, fingerprint: &str, now: i64) -> Result<bool>;
}
#[derive(Default)]
pub struct InMemoryMcp {
    rows: RwLock<BTreeMap<String, McpRecord>>,
    confirmations: RwLock<BTreeMap<String, (String, String, i64)>>,
}
fn lock_error() -> CoreError {
    CoreError::Storage("MCP store unavailable".into())
}
#[async_trait]
impl McpStore for InMemoryMcp {
    async fn list(&self) -> Result<Vec<McpRecord>> {
        Ok(self
            .rows
            .read()
            .map_err(|_| lock_error())?
            .values()
            .cloned()
            .collect())
    }
    async fn get(&self, id: &str) -> Result<Option<McpRecord>> {
        Ok(self.rows.read().map_err(|_| lock_error())?.get(id).cloned())
    }
    async fn remember(
        &self,
        id: &str,
        subject: &str,
        fingerprint: &str,
        expires: i64,
    ) -> Result<()> {
        let mut entries = self.confirmations.write().map_err(|_| lock_error())?;
        entries.retain(|_, value| value.2 > expires - 300);
        if entries.len() >= 1000 {
            return Err(CoreError::Conflict(
                "too many pending tool confirmations".into(),
            ));
        }
        entries.insert(id.into(), (subject.into(), fingerprint.into(), expires));
        Ok(())
    }
    async fn consume(&self, id: &str, subject: &str, fingerprint: &str, now: i64) -> Result<bool> {
        let mut entries = self.confirmations.write().map_err(|_| lock_error())?;
        if entries
            .get(id)
            .is_some_and(|v| v.0 == subject && v.1 == fingerprint && v.2 > now)
        {
            entries.remove(id);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    async fn put(&self, record: McpRecord, expected: i64, _actor: &str) -> Result<()> {
        record.connection.validate()?;
        let mut rows = self.rows.write().map_err(|_| lock_error())?;
        if rows
            .get(&record.connection.id)
            .map_or(0, |r| r.connection.revision)
            != expected
            || expected.checked_add(1) != Some(record.connection.revision)
        {
            return Err(CoreError::Conflict(
                "MCP connection changed; reload before saving".into(),
            ));
        }
        rows.insert(record.connection.id.clone(), record);
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn discover(&self, url: &str, token: &str) -> Result<Vec<McpTool>>;
    async fn call(
        &self,
        url: &str,
        token: &str,
        tool: &str,
        schema_hash: &str,
        arguments: serde_json::Value,
        guard: &dyn McpCallGuard,
    ) -> Result<serde_json::Value>;
}

/// Current authorization checked after transport preparation, before dispatch.
#[async_trait]
pub trait McpCallGuard: Send + Sync {
    async fn check(&self) -> Result<()>;
}
