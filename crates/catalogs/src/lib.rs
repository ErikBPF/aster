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

pub fn catalog_from_config(config: &CatalogConfig) -> Result<Arc<dyn Catalog>> {
    let catalog: Arc<dyn Catalog> = match config.kind.as_str() {
        "polaris" => Arc::new(PolarisCatalog::new(
            &config.id,
            &config.endpoint,
            config.catalog.clone().unwrap_or_else(|| "default".into()),
        )),
        "nessie" => Arc::new(NessieCatalog::new(&config.id, &config.endpoint)),
        "unity" => Arc::new(UnityCatalog::new(&config.id, &config.endpoint)),
        other => return Err(CoreError::Invalid(format!("unknown catalog kind: {other}"))),
    };
    Ok(catalog)
}
