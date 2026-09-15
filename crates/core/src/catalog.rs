use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CatalogId(pub String);

impl CatalogId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for CatalogId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRef {
    pub namespace: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub table: TableRef,
    pub columns: Vec<ColumnSchema>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogHealth {
    Healthy,
    Degraded,
    Unavailable,
}

/// A catalog and its authorization decisions. Polaris is the first
/// implementation; Nessie, Unity Catalog, and others plug in behind the same
/// trait. The trait exposes metadata navigation only; engines enforce grants.
#[async_trait]
pub trait Catalog: Send + Sync {
    fn id(&self) -> &CatalogId;
    /// Catalog product, e.g. "polaris", "nessie", "unity".
    fn kind(&self) -> &str;
    async fn health(&self) -> CatalogHealth;
    async fn list_namespaces(&self) -> Result<Vec<Namespace>>;
    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>>;
    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema>;
}
