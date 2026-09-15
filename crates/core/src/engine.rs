use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineId(pub String);

impl EngineId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for EngineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stable description of a registered engine instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    pub id: EngineId,
    /// Engine family, e.g. "trino", "spark", "starrocks".
    pub kind: String,
    pub endpoint: String,
    /// Routing group this instance belongs to when it sits behind a gateway.
    /// Selecting a pool is then a choice of group, not of host.
    #[serde(default)]
    pub routing_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    pub sql: String,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub max_rows: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineHealth {
    Healthy,
    Degraded,
    Unavailable,
}

/// A query engine instance. Implementations adapt a specific engine's wire
/// protocol; the rest of the application never branches on engine kind.
#[async_trait]
pub trait QueryEngine: Send + Sync {
    fn info(&self) -> &EngineInfo;
    async fn health(&self) -> EngineHealth;
    async fn execute(&self, request: QueryRequest) -> Result<QueryResult>;
}
