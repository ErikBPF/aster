//! Catalog plugins and the data-permission surface. Polaris (Apache Polaris
//! Iceberg REST) is the first implementation. Nessie and Unity Catalog are
//! registered stubs; the trait is intentionally product-neutral so the
//! engine/catalog choice stays a team decision.

use std::sync::Arc;

use aster_core::{
    Catalog, CatalogConfig, CatalogId, ColumnSchema, CoreError, Health, Namespace, Result,
    TableRef, TableSchema,
};
use async_trait::async_trait;

pub struct PolarisCatalog {
    id: CatalogId,
    client: reqwest::Client,
    endpoint: String,
    prefix: String,
    token: Option<String>,
}

impl PolarisCatalog {
    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        prefix: impl Into<String>,
    ) -> Self {
        Self {
            id: CatalogId::new(id),
            client: reqwest::Client::new(),
            endpoint: endpoint.into(),
            prefix: prefix.into(),
            token: None,
        }
    }

    fn request(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!(
            "{}/api/catalog/v1/{}/{}",
            self.endpoint.trim_end_matches('/'),
            self.prefix,
            path
        );
        let builder = self.client.get(url).header("Accept", "application/json");
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }
}

#[async_trait]
impl Catalog for PolarisCatalog {
    fn id(&self) -> &CatalogId {
        &self.id
    }

    fn kind(&self) -> &str {
        "polaris"
    }

    async fn health(&self) -> Health {
        match self.request("namespaces").send().await {
            Ok(response) if response.status().is_success() => Health::Healthy,
            Ok(_) => Health::Degraded,
            Err(_) => Health::Unavailable,
        }
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let payload: serde_json::Value = self
            .request("namespaces")
            .send()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?
            .json()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?;

        let namespaces = payload
            .get("namespaces")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_array())
                    .map(|parts| Namespace {
                        name: parts
                            .iter()
                            .filter_map(|part| part.as_str())
                            .collect::<Vec<_>>()
                            .join("."),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(namespaces)
    }

    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        let path = format!("namespaces/{namespace}/tables");
        let payload: serde_json::Value = self
            .request(&path)
            .send()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?
            .json()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?;

        let tables = payload
            .get("identifiers")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|item| TableRef {
                        namespace: item
                            .get("namespace")
                            .and_then(|value| value.as_array())
                            .map(|parts| {
                                parts
                                    .iter()
                                    .filter_map(|part| part.as_str())
                                    .collect::<Vec<_>>()
                                    .join(".")
                            })
                            .unwrap_or_else(|| namespace.to_string()),
                        name: item["name"].as_str().unwrap_or_default().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(tables)
    }

    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        let path = format!("namespaces/{}/tables/{}", table.namespace, table.name);
        let payload: serde_json::Value = self
            .request(&path)
            .send()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?
            .json()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?;

        let fields = payload
            .pointer("/metadata/schemas/0/fields")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();

        let columns = fields
            .iter()
            .map(|field| ColumnSchema {
                name: field["name"].as_str().unwrap_or_default().to_string(),
                data_type: field["type"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| field["type"].to_string()),
                nullable: !field["required"].as_bool().unwrap_or(false),
            })
            .collect();

        Ok(TableSchema {
            table: TableRef {
                namespace: table.namespace.clone(),
                name: table.name.clone(),
            },
            columns,
        })
    }
}

/// Cube's semantic layer, browsed read-only through `/v1/meta`.
///
/// `endpoint` is the Cube base URL and the configured `catalog` value is the base
/// path (`cubejs-api` by default). A cube and a view are both queryable, so they
/// are browsed as two namespaces; each member (measure or dimension) becomes a
/// column, named exactly as Cube's own SQL API exposes it.
/// ponytail: no credential yet — Cube API tokens are JWTs signed with
/// `CUBEJS_API_SECRET`, and nothing in `CatalogConfig` can carry one. Decide where
/// the secret lives when a Cube instance is actually deployed (D6).
pub struct CubeCatalog {
    id: CatalogId,
    client: reqwest::Client,
    endpoint: String,
    base_path: String,
}

impl CubeCatalog {
    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        base_path: impl Into<String>,
    ) -> Self {
        Self {
            id: CatalogId::new(id),
            client: reqwest::Client::new(),
            endpoint: endpoint.into(),
            base_path: base_path.into(),
        }
    }

    /// The compiled model: cubes and views, each with its measures and dimensions.
    async fn meta(&self) -> Result<serde_json::Value> {
        let url = format!(
            "{}/{}/v1/meta",
            self.endpoint.trim_end_matches('/'),
            self.base_path.trim_matches('/')
        );
        self.client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?
            .json()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))
    }
}

/// One namespace's members, in the order Cube reports them.
fn cube_members<'a>(
    meta: &'a serde_json::Value,
    namespace: &str,
) -> Vec<(&'a str, &'a serde_json::Value)> {
    meta.get(namespace)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| Some((item.get("name")?.as_str()?, item)))
                .collect()
        })
        .unwrap_or_default()
}

/// Cube puts measures and dimensions in separate lists; a browser wants one column list.
fn cube_columns(member: &serde_json::Value) -> Vec<ColumnSchema> {
    ["measures", "dimensions"]
        .iter()
        .filter_map(|section| member.get(section).and_then(|value| value.as_array()))
        .flatten()
        .filter_map(|column| {
            Some(ColumnSchema {
                name: column.get("name")?.as_str()?.to_string(),
                data_type: column
                    .get("type")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                // Cube's model carries no nullability.
                nullable: true,
            })
        })
        .collect()
}

#[async_trait]
impl Catalog for CubeCatalog {
    fn id(&self) -> &CatalogId {
        &self.id
    }

    fn kind(&self) -> &str {
        "cube"
    }

    async fn health(&self) -> Health {
        match self.meta().await {
            Ok(_) => Health::Healthy,
            Err(CoreError::Catalog(_)) => Health::Degraded,
            Err(_) => Health::Unavailable,
        }
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let meta = self.meta().await?;
        Ok(["cubes", "views"]
            .iter()
            .filter(|namespace| !cube_members(&meta, namespace).is_empty())
            .map(|namespace| Namespace {
                name: (*namespace).to_string(),
            })
            .collect())
    }

    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        let meta = self.meta().await?;
        Ok(cube_members(&meta, namespace)
            .iter()
            .map(|(name, _)| TableRef {
                namespace: namespace.to_string(),
                name: (*name).to_string(),
            })
            .collect())
    }

    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        let meta = self.meta().await?;
        let member = cube_members(&meta, &table.namespace)
            .into_iter()
            .find(|(name, _)| *name == table.name)
            .map(|(_, member)| member)
            .ok_or_else(|| CoreError::NotFound(format!("unknown cube: {}", table.name)))?;
        Ok(TableSchema {
            table: table.clone(),
            columns: cube_columns(member),
        })
    }
}

macro_rules! stub_catalog {
    ($name:ident, $kind:literal) => {
        pub struct $name {
            id: CatalogId,
            endpoint: String,
        }

        impl $name {
            pub fn new(id: impl Into<String>, endpoint: impl Into<String>) -> Self {
                Self {
                    id: CatalogId::new(id),
                    endpoint: endpoint.into(),
                }
            }
        }

        #[async_trait]
        impl Catalog for $name {
            fn id(&self) -> &CatalogId {
                &self.id
            }

            fn kind(&self) -> &str {
                $kind
            }

            async fn health(&self) -> Health {
                Health::Unavailable
            }

            async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
                Err(CoreError::Catalog(format!(
                    "{} catalog plugin not implemented yet ({})",
                    $kind, self.endpoint
                )))
            }

            async fn list_tables(&self, _namespace: &str) -> Result<Vec<TableRef>> {
                Err(CoreError::Catalog(format!(
                    "{} catalog plugin not implemented yet",
                    $kind
                )))
            }

            async fn table_schema(&self, _table: &TableRef) -> Result<TableSchema> {
                Err(CoreError::Catalog(format!(
                    "{} catalog plugin not implemented yet",
                    $kind
                )))
            }
        }
    };
}

stub_catalog!(NessieCatalog, "nessie");
stub_catalog!(UnityCatalog, "unity");

/// Canned namespaces and schemas for local UI work, so the catalog tab can be
/// exercised without a metastore. Never register it in a deployment.
pub struct MockCatalog {
    id: CatalogId,
}

impl MockCatalog {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: CatalogId::new(id),
        }
    }

    fn tables(namespace: &str) -> Vec<(&'static str, &'static [(&'static str, &'static str)])> {
        match namespace {
            "aster_demo" => vec![
                (
                    "orders",
                    &[
                        ("order_id", "bigint"),
                        ("customer", "varchar"),
                        ("region", "varchar"),
                        ("total", "decimal(12,2)"),
                        ("placed_at", "timestamp"),
                    ],
                ),
                (
                    "customers",
                    &[
                        ("customer_id", "bigint"),
                        ("name", "varchar"),
                        ("email", "varchar"),
                        ("created_at", "timestamp"),
                    ],
                ),
            ],
            "aster_demo.sales" => vec![(
                "daily_revenue",
                &[
                    ("day", "date"),
                    ("region", "varchar"),
                    ("revenue", "decimal(18,2)"),
                ],
            )],
            "aster_raw" => vec![
                (
                    "events",
                    &[
                        ("event_id", "varchar"),
                        ("kind", "varchar"),
                        ("payload", "json"),
                        ("seen_at", "timestamp"),
                    ],
                ),
                (
                    "clickstream",
                    &[
                        ("session_id", "varchar"),
                        ("path", "varchar"),
                        ("country", "varchar"),
                    ],
                ),
            ],
            _ => Vec::new(),
        }
    }
}

#[async_trait]
impl Catalog for MockCatalog {
    fn id(&self) -> &CatalogId {
        &self.id
    }

    fn kind(&self) -> &str {
        "mock"
    }

    async fn health(&self) -> Health {
        Health::Healthy
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        Ok(["aster_demo", "aster_demo.sales", "aster_raw"]
            .iter()
            .map(|name| Namespace {
                name: (*name).into(),
            })
            .collect())
    }

    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        Ok(Self::tables(namespace)
            .iter()
            .map(|(name, _)| TableRef {
                namespace: namespace.into(),
                name: (*name).into(),
            })
            .collect())
    }

    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        let columns = Self::tables(&table.namespace)
            .iter()
            .find(|(name, _)| *name == table.name)
            .map(|(_, columns)| {
                columns
                    .iter()
                    .map(|(name, data_type)| ColumnSchema {
                        name: (*name).into(),
                        data_type: (*data_type).into(),
                        nullable: true,
                    })
                    .collect()
            })
            .ok_or_else(|| CoreError::NotFound(format!("unknown table: {}", table.name)))?;
        Ok(TableSchema {
            table: table.clone(),
            columns,
        })
    }
}

pub fn catalog_from_config(config: &CatalogConfig) -> Result<Arc<dyn Catalog>> {
    let catalog: Arc<dyn Catalog> = match config.kind.as_str() {
        "polaris" => Arc::new(PolarisCatalog::new(
            &config.id,
            &config.endpoint,
            config.catalog.clone().unwrap_or_else(|| "default".into()),
        )),
        "nessie" => Arc::new(NessieCatalog::new(&config.id, &config.endpoint)),
        "unity" => Arc::new(UnityCatalog::new(&config.id, &config.endpoint)),
        // A semantic layer browsed read-only; `catalog` carries the base path.
        "cube" => Arc::new(CubeCatalog::new(
            &config.id,
            &config.endpoint,
            config
                .catalog
                .clone()
                .unwrap_or_else(|| "cubejs-api".into()),
        )),
        // Demo provider: canned metadata, for local UI work only.
        "mock" => Arc::new(MockCatalog::new(&config.id)),
        other => return Err(CoreError::Invalid(format!("unknown catalog kind: {other}"))),
    };
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_config(kind: &str) -> CatalogConfig {
        CatalogConfig {
            id: "catalog-1".into(),
            kind: kind.into(),
            endpoint: "http://catalog.invalid:8181".into(),
            catalog: None,
        }
    }

    #[test]
    fn an_unknown_kind_is_refused_at_startup() {
        let Err(error) = catalog_from_config(&catalog_config("glue")) else {
            panic!("expected a refusal");
        };
        assert!(error.to_string().contains("unknown catalog kind"));
    }

    #[test]
    fn every_declared_kind_resolves_to_a_provider() {
        for kind in ["polaris", "nessie", "unity", "cube", "mock"] {
            let catalog = catalog_from_config(&catalog_config(kind)).expect(kind);
            assert_eq!(catalog.kind(), kind);
        }
    }

    /// The compiled-model document as Cube serves it, trimmed to the fields we read.
    fn cube_meta() -> serde_json::Value {
        serde_json::json!({
            "cubes": [
                {
                    "name": "Orders",
                    "measures": [
                        {"name": "Orders.count", "type": "number"},
                        {"name": "Orders.total", "type": "number"}
                    ],
                    "dimensions": [
                        {"name": "Orders.status", "type": "string"},
                        {"name": "Orders.placed_at", "type": "time"}
                    ]
                }
            ],
            "views": [{"name": "Sales", "measures": [], "dimensions": [{"name": "Sales.region", "type": "string"}]}],
            "compilerId": "abc"
        })
    }

    #[test]
    fn a_cube_model_becomes_namespaces_tables_and_columns() {
        let meta = cube_meta();

        let namespaces: Vec<_> = ["cubes", "views"]
            .iter()
            .filter(|namespace| !cube_members(&meta, namespace).is_empty())
            .collect();
        assert_eq!(namespaces.len(), 2);

        let cubes = cube_members(&meta, "cubes");
        assert_eq!(cubes.len(), 1);
        assert_eq!(cubes[0].0, "Orders");

        let columns = cube_columns(cubes[0].1);
        assert_eq!(
            columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Orders.count",
                "Orders.total",
                "Orders.status",
                "Orders.placed_at"
            ]
        );
        assert_eq!(columns[2].data_type, "string");
    }

    #[test]
    fn a_namespace_cube_does_not_have_is_empty() {
        assert!(cube_members(&cube_meta(), "models").is_empty());
    }

    #[tokio::test]
    async fn the_mock_catalog_walks_namespaces_tables_and_schemas() {
        let catalog = catalog_from_config(&catalog_config("mock")).expect("mock");

        let namespaces = catalog.list_namespaces().await.expect("namespaces");
        assert!(namespaces.iter().any(|ns| ns.name == "aster_demo"));

        let tables = catalog.list_tables("aster_demo").await.expect("tables");
        assert!(tables.iter().any(|table| table.name == "orders"));

        let schema = catalog
            .table_schema(&TableRef {
                namespace: "aster_demo".into(),
                name: "orders".into(),
            })
            .await
            .expect("schema");
        assert_eq!(schema.columns[0].name, "order_id");

        let miss = catalog
            .table_schema(&TableRef {
                namespace: "aster_demo".into(),
                name: "nope".into(),
            })
            .await;
        assert!(miss.is_err());
    }
}
