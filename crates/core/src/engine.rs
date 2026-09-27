use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::health::Health;

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

/// A query engine instance. Implementations adapt a specific engine's wire
/// protocol; the rest of the application never branches on engine kind.
#[async_trait]
pub trait QueryEngine: Send + Sync {
    fn info(&self) -> &EngineInfo;
    /// Whether the engine has a session catalog a bare `schema.table` resolves
    /// against. Trino does; Spark Connect has none, so a default catalog must
    /// not be pushed onto it.
    fn uses_catalog(&self) -> bool {
        true
    }
    async fn health(&self) -> Health;
    async fn execute(&self, request: QueryRequest) -> Result<QueryResult>;
    /// Protected bindings may use only an adapter that explicitly accepts a
    /// verified session subject and enforces it at the backend. Shared adapters
    /// remain closed until that backend policy is implemented and tested.
    async fn execute_as_verified(
        &self,
        _request: QueryRequest,
        _subject: &str,
    ) -> Result<QueryResult> {
        Err(crate::CoreError::Unauthorized(
            "engine has no authenticated delegation".into(),
        ))
    }
}
