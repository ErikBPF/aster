use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::health::Health;

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

/// Browse metadata. A generic table may have no catalog-provided schema or
/// base location; neither absence implies an empty table or readable data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDescriptor {
    #[serde(flatten)]
    pub table: TableRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_location: Option<String>,
    pub schema_available: bool,
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

/// A catalog and its authorization decisions. Polaris is the first
/// implementation; Nessie, Unity Catalog, and others plug in behind the same
/// trait. The trait exposes metadata navigation only; engines enforce grants.
#[async_trait]
pub trait Catalog: Send + Sync {
    fn id(&self) -> &CatalogId;
    /// Catalog product, e.g. "polaris", "nessie", "unity".
    fn kind(&self) -> &str;
    async fn health(&self) -> Health;
    async fn list_namespaces(&self) -> Result<Vec<Namespace>>;
    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>>;
    /// Defaults preserve metadata navigation for catalogs without a format
    /// descriptor. Polaris overrides this for Iceberg and generic tables.
    async fn list_table_descriptors(&self, namespace: &str) -> Result<Vec<TableDescriptor>> {
        Ok(self
            .list_tables(namespace)
            .await?
            .into_iter()
            .map(|table| TableDescriptor {
                table,
                format: None,
                base_location: None,
                schema_available: true,
            })
            .collect())
    }
    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema>;
}
