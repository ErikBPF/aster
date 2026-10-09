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
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRef {
    pub namespace: String,
    #[serde(default)]
    pub namespace_segments: Vec<String>,
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
    /// A complete descriptor listing proves current metastore registration.
    /// Semantic/discovery indexes must not opt in; this never proves readable rows.
    fn authoritative_inventory(&self) -> bool {
        false
    }
    /// Upper bound on total source bytes per metadata method, including nested
    /// reads. None means legacy AI discovery must omit this optional adapter.
    fn metadata_read_bytes(&self) -> Option<usize> {
        None
    }
    fn id(&self) -> &CatalogId;
    /// Catalog product, e.g. "polaris", "nessie", "unity".
    fn kind(&self) -> &str;
    async fn health(&self) -> Health;
    async fn list_namespaces(&self) -> Result<Vec<Namespace>>;
    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>>;
    async fn list_tables_qualified(&self, segments: &[String]) -> Result<Vec<TableRef>> {
        self.list_tables(&legacy_namespace(segments)?).await
    }
    async fn list_descriptors_qualified(
        &self,
        segments: &[String],
    ) -> Result<Vec<TableDescriptor>> {
        self.list_table_descriptors(&legacy_namespace(segments)?)
            .await
    }
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
    /// Opt-in bounded observation. Implementations must cap ALL source response
    /// bytes and reads before buffering, including authentication and pagination.
    /// The default performs zero IO; an unbounded adapter is optional unavailable
    /// context, never an excuse to bypass the caller's resource boundary.
    async fn table_schema_bounded(
        &self,
        _table: &TableRef,
        _max_bytes: usize,
        _max_reads: usize,
    ) -> Result<TableSchema> {
        Err(crate::CoreError::Catalog(
            "bounded observation unavailable".into(),
        ))
    }
}

/// Old providers may accept only a single unambiguous namespace component.
pub fn legacy_namespace(segments: &[String]) -> Result<String> {
    match segments {
        [name] if !name.is_empty() && !name.contains('.') && !name.contains('\u{1f}') => {
            Ok(name.clone())
        }
        _ => Err(crate::CoreError::Invalid(
            "structured namespace required by provider".into(),
        )),
    }
}

pub fn namespace_segments(legacy: &str, segments: &[String]) -> Result<Vec<String>> {
    if !segments.is_empty() {
        if segments
            .iter()
            .any(|s| s.is_empty() || s.contains('\u{1f}'))
            || (!legacy.is_empty() && legacy != segments.join("."))
        {
            return Err(crate::CoreError::Invalid(
                "conflicting or invalid namespace identity".into(),
            ));
        }
        return Ok(segments.to_vec());
    }
    legacy_namespace(&[legacy.to_string()]).map(|name| vec![name])
}
