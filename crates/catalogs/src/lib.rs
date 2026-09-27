//! Catalog plugins and the data-permission surface. Polaris (Apache Polaris
//! Iceberg REST) is the first implementation. Nessie and Unity Catalog are
//! registered stubs; the trait is intentionally product-neutral so the
//! engine/catalog choice stays a team decision.

use std::{collections::HashSet, sync::Arc};

use aster_core::{
    Catalog, CatalogConfig, CatalogId, ColumnSchema, CoreError, Health, Namespace, Result,
    TableDescriptor, TableRef, TableSchema,
};
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize};

/// Polaris's realm context header. The dev stack runs the single default realm;
/// a multi-realm deployment would have to make this configurable.
const POLARIS_REALM: &str = "POLARIS";

pub struct PolarisCatalog {
    id: CatalogId,
    client: reqwest::Client,
    endpoint: String,
    prefix: String,
    token: Option<String>,
    credential: Option<(String, String)>,
    generic_tables_enabled: bool,
}

#[derive(Deserialize)]
struct GenericList {
    identifiers: Vec<GenericIdentifier>,
    #[serde(rename = "next-page-token")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct GenericIdentifier {
    namespace: Vec<String>,
    name: String,
}

#[derive(Deserialize)]
struct GenericLoad {
    table: GenericTable,
}

#[derive(Deserialize)]
struct GenericTable {
    name: String,
    format: String,
    #[serde(rename = "base-location")]
    base_location: Option<String>,
}

async fn checked_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
    operation: &str,
) -> Result<T> {
    let response = request
        .send()
        .await
        .map_err(|error| CoreError::Catalog(error.to_string()))?;
    if !response.status().is_success() {
        return Err(CoreError::Catalog(format!(
            "{operation} returned HTTP {}",
            response.status().as_u16()
        )));
    }
    response
        .json()
        .await
        .map_err(|error| CoreError::Catalog(format!("invalid {operation} response: {error}")))
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
            credential: None,
            generic_tables_enabled: false,
        }
    }

    /// A ready bearer token, sent as-is.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// OAuth2 `client_id:client_secret`, exchanged for a token on each request.
    /// Preferred over `with_token`, whose token expires.
    pub fn with_credential(mut self, credential: Option<String>) -> Self {
        self.credential = credential.and_then(|value| {
            value
                .split_once(':')
                .map(|(id, secret)| (id.to_string(), secret.to_string()))
        });
        self
    }

    /// Generic metadata is a separate beta API. Runtime browsing stays off
    /// until a format-aware row-read and storage-policy gate validates it.
    pub fn with_generic_tables(mut self, enabled: bool) -> Self {
        self.generic_tables_enabled = enabled;
        self
    }

    /// The bearer token to attach: the configured one, or a fresh token from the
    /// OAuth2 client-credentials grant.
    async fn bearer(&self) -> Result<Option<String>> {
        if let Some(token) = &self.token {
            return Ok(Some(token.clone()));
        }
        let Some((client_id, client_secret)) = &self.credential else {
            return Ok(None);
        };
        let payload: serde_json::Value = self
            .client
            .post(format!(
                "{}/api/catalog/v1/oauth/tokens",
                self.endpoint.trim_end_matches('/')
            ))
            .basic_auth(client_id, Some(client_secret))
            .header("Polaris-Realm", POLARIS_REALM)
            .form(&[
                ("grant_type", "client_credentials"),
                ("scope", "PRINCIPAL_ROLE:ALL"),
            ])
            .send()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?
            .json()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?;
        payload
            .get("access_token")
            .and_then(|value| value.as_str())
            .map(|token| Some(token.to_string()))
            .ok_or_else(|| CoreError::Catalog("polaris token response had no access_token".into()))
    }

    async fn request(&self, path: &str) -> Result<reqwest::RequestBuilder> {
        let url = format!(
            "{}/api/catalog/v1/{}/{}",
            self.endpoint.trim_end_matches('/'),
            self.prefix,
            path
        );
        let builder = self.client.get(url).header("Accept", "application/json");
        Ok(match self.bearer().await? {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        })
    }

    async fn generic_request(&self, path: &[&str]) -> Result<reqwest::RequestBuilder> {
        let mut url = reqwest::Url::parse(&format!(
            "{}/api/catalog/polaris/v1/",
            self.endpoint.trim_end_matches('/')
        ))
        .map_err(|_| CoreError::Catalog("invalid Polaris endpoint".into()))?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| CoreError::Catalog("invalid Polaris endpoint path".into()))?;
            segments.pop_if_empty();
            segments.push(&self.prefix);
            for part in path {
                segments.push(part);
            }
        }
        let builder = self.client.get(url).header("Accept", "application/json");
        Ok(match self.bearer().await? {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        })
    }

    async fn list_iceberg_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        let path = format!("namespaces/{namespace}/tables");
        let payload: serde_json::Value =
            checked_json(self.request(&path).await?, "Iceberg list").await?;
        let identifiers = payload
            .get("identifiers")
            .and_then(|value| value.as_array())
            .ok_or_else(|| CoreError::Catalog("Iceberg list has no identifiers".into()))?;
        Ok(identifiers
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
            .collect())
    }

    async fn list_generic_refs(&self, namespace: &str) -> Result<Vec<TableRef>> {
        let mut tables = Vec::new();
        let mut seen_names = HashSet::new();
        let mut seen_tokens = HashSet::new();
        let mut page_token: Option<String> = None;
        loop {
            let request = self
                .generic_request(&["namespaces", namespace, "generic-tables"])
                .await?;
            let request = match &page_token {
                Some(token) => request.query(&[("pageToken", token)]),
                None => request,
            };
            let page: GenericList = checked_json(request, "Generic Table list").await?;
            for item in page.identifiers {
                if item.namespace.join(".") != namespace || item.name.trim().is_empty() {
                    return Err(CoreError::Catalog(
                        "Generic Table list contains an invalid identifier".into(),
                    ));
                }
                if !seen_names.insert(item.name.clone()) {
                    return Err(CoreError::Catalog(
                        "Generic Table list contains a duplicate name".into(),
                    ));
                }
                tables.push(TableRef {
                    namespace: namespace.to_string(),
                    name: item.name,
                });
            }
            match page.next_page_token {
                None => return Ok(tables),
                Some(token) if token.is_empty() || !seen_tokens.insert(token.clone()) => {
                    return Err(CoreError::Catalog(
                        "Generic Table list has an invalid page token".into(),
                    ));
                }
                Some(token) => page_token = Some(token),
            }
        }
    }

    pub async fn load_generic_table(&self, table: &TableRef) -> Result<TableDescriptor> {
        let request = self
            .generic_request(&[
                "namespaces",
                &table.namespace,
                "generic-tables",
                &table.name,
            ])
            .await?;
        let loaded: GenericLoad = checked_json(request, "Generic Table load").await?;
        if loaded.table.name != table.name || loaded.table.format.trim().is_empty() {
            return Err(CoreError::Catalog(
                "Generic Table load has invalid metadata".into(),
            ));
        }
        if let Some(location) = &loaded.table.base_location {
            let parsed = reqwest::Url::parse(location).map_err(|_| {
                CoreError::Catalog("Generic Table has malformed base location".into())
            })?;
            if parsed.cannot_be_a_base()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.query().is_some()
                || parsed.fragment().is_some()
            {
                return Err(CoreError::Catalog(
                    "Generic Table has malformed base location".into(),
                ));
            }
        }
        Ok(TableDescriptor {
            table: table.clone(),
            format: Some(loaded.table.format),
            base_location: loaded.table.base_location,
            schema_available: false,
        })
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
        let Ok(request) = self.request("namespaces").await else {
            return Health::Unavailable;
        };
        match request.send().await {
            Ok(response) if response.status().is_success() => Health::Healthy,
            Ok(_) => Health::Degraded,
            Err(_) => Health::Unavailable,
        }
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let payload: serde_json::Value =
            checked_json(self.request("namespaces").await?, "namespace list").await?;

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
        if self.generic_tables_enabled {
            Ok(self
                .list_table_descriptors(namespace)
                .await?
                .into_iter()
                .map(|descriptor| descriptor.table)
                .collect())
        } else {
            self.list_iceberg_tables(namespace).await
        }
    }

    async fn list_table_descriptors(&self, namespace: &str) -> Result<Vec<TableDescriptor>> {
        let iceberg = self.list_iceberg_tables(namespace).await?;
        let mut names: HashSet<_> = iceberg.iter().map(|table| table.name.clone()).collect();
        let mut descriptors: Vec<_> = iceberg
            .into_iter()
            .map(|table| TableDescriptor {
                table,
                format: Some("iceberg".into()),
                base_location: None,
                schema_available: true,
            })
            .collect();
        if self.generic_tables_enabled {
            for table in self.list_generic_refs(namespace).await? {
                if !names.insert(table.name.clone()) {
                    return Err(CoreError::Catalog(
                        "Iceberg and Generic Table lists contain the same name".into(),
                    ));
                }
                descriptors.push(self.load_generic_table(&table).await?);
            }
        }
        Ok(descriptors)
    }

    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        if self.generic_tables_enabled
            && self
                .list_generic_refs(&table.namespace)
                .await?
                .iter()
                .any(|generic| generic.name == table.name)
        {
            return Err(CoreError::Invalid(
                "schema unavailable for a Generic Table".into(),
            ));
        }
        let path = format!("namespaces/{}/tables/{}", table.namespace, table.name);
        let payload: serde_json::Value =
            checked_json(self.request(&path).await?, "Iceberg table load").await?;

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
/// `CUBEJS_API_SECRET`, which `CatalogConfig.credential` could carry once a Cube
/// instance is actually deployed (D6).
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

/// OpenMetadata as a metadata *source* we read from: schemas become namespaces and
/// tables become tables, with their columns and nullability. Descriptions, owners,
/// glossary terms and lineage are richer than our `TableSchema`, so they are not
/// read here — enriching the UI from this same API is a separate server-side step.
///
/// `endpoint` is the OpenMetadata base URL; the optional `catalog` is a database
/// fully-qualified name to restrict the schemas to.
/// ponytail: the bot token is a parameter rather than config because where the
/// credential lives (secret store key, per-catalog name) is a decision to make when
/// a real instance exists.
pub struct OpenMetadataCatalog {
    id: CatalogId,
    client: reqwest::Client,
    endpoint: String,
    database: Option<String>,
    token: Option<String>,
}

impl OpenMetadataCatalog {
    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        database: Option<String>,
        token: Option<String>,
    ) -> Self {
        Self {
            id: CatalogId::new(id),
            client: reqwest::Client::new(),
            endpoint: endpoint.into(),
            database,
            token,
        }
    }

    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}/api/v1/{}", self.endpoint.trim_end_matches('/'), path);
        let builder = self.client.get(url).header("Accept", "application/json");
        let builder = match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        };
        builder
            .send()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))?
            .json()
            .await
            .map_err(|error| CoreError::Catalog(error.to_string()))
    }

    /// Schemas, optionally restricted to the configured database.
    async fn schemas(&self) -> Result<serde_json::Value> {
        let path = match &self.database {
            Some(database) => format!("databaseSchemas?limit=200&database={database}"),
            None => "databaseSchemas?limit=200".to_string(),
        };
        self.get(&path).await
    }

    /// The tables of a schema, with their columns.
    async fn tables(&self, namespace: &str) -> Result<serde_json::Value> {
        self.get(&format!(
            "tables?limit=200&fields=columns&databaseSchema={namespace}"
        ))
        .await
    }
}

/// OpenMetadata answers with `{"data": [...], "paging": {...}}`; tolerate a bare array too.
fn om_list(payload: &serde_json::Value) -> &[serde_json::Value] {
    payload
        .get("data")
        .or(Some(payload))
        .and_then(|value| value.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn om_name(entity: &serde_json::Value) -> Option<String> {
    entity
        .get("fullyQualifiedName")
        .or_else(|| entity.get("name"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

/// One entity's columns, in ordinal order as OpenMetadata reports them.
fn om_columns(table: &serde_json::Value) -> Vec<ColumnSchema> {
    table
        .get("columns")
        .and_then(|value| value.as_array())
        .map(|columns| {
            columns
                .iter()
                .filter_map(|column| {
                    let constraint = column.get("constraint").and_then(|value| value.as_str());
                    Some(ColumnSchema {
                        name: column.get("name")?.as_str()?.to_string(),
                        data_type: column
                            .get("dataTypeDisplay")
                            .or_else(|| column.get("dataType"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        nullable: !matches!(constraint, Some("NOT_NULL" | "PRIMARY_KEY")),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[async_trait]
impl Catalog for OpenMetadataCatalog {
    fn id(&self) -> &CatalogId {
        &self.id
    }

    fn kind(&self) -> &str {
        "openmetadata"
    }

    async fn health(&self) -> Health {
        match self.get("databases?limit=1").await {
            Ok(_) => Health::Healthy,
            Err(CoreError::Catalog(_)) => Health::Degraded,
            Err(_) => Health::Unavailable,
        }
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let payload = self.schemas().await?;
        Ok(om_list(&payload)
            .iter()
            .filter_map(|schema| {
                Some(Namespace {
                    name: om_name(schema)?,
                })
            })
            .collect())
    }

    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        let payload = self.tables(namespace).await?;
        Ok(om_list(&payload)
            .iter()
            .filter_map(|table| {
                Some(TableRef {
                    namespace: namespace.to_string(),
                    name: om_name(table)?.rsplit('.').next()?.to_string(),
                })
            })
            .collect())
    }

    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        let payload = self.tables(&table.namespace).await?;
        let entity = om_list(&payload)
            .iter()
            .find(|entity| {
                om_name(entity)
                    .map(|name| name == table.name || name.ends_with(&format!(".{}", table.name)))
                    .unwrap_or(false)
            })
            .ok_or_else(|| CoreError::NotFound(format!("unknown table: {}", table.name)))?;
        Ok(TableSchema {
            table: table.clone(),
            columns: om_columns(entity),
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
        "polaris" => Arc::new(
            PolarisCatalog::new(
                &config.id,
                &config.endpoint,
                config.catalog.clone().unwrap_or_else(|| "default".into()),
            )
            .with_token(config.token.clone())
            .with_credential(config.credential.clone()),
        ),
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
        // A metadata source we read from; `catalog` optionally narrows to one database.
        "openmetadata" => Arc::new(OpenMetadataCatalog::new(
            &config.id,
            &config.endpoint,
            config.catalog.clone(),
            None,
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
            token: None,
            credential: None,
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
        for kind in ["polaris", "cube", "openmetadata", "nessie", "unity", "mock"] {
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

    /// A list response as OpenMetadata serves it, trimmed to the fields we read.
    fn om_tables_response() -> serde_json::Value {
        serde_json::json!({
            "data": [{
                "name": "orders",
                "fullyQualifiedName": "trino.platform.sales.orders",
                "columns": [
                    {"name": "order_id", "dataType": "BIGINT", "constraint": "PRIMARY_KEY"},
                    {"name": "customer_id", "dataType": "BIGINT", "constraint": "NOT_NULL"},
                    {"name": "total", "dataType": "DECIMAL", "dataTypeDisplay": "DECIMAL(12,2)"},
                    {"name": "placed_at", "dataType": "TIMESTAMP", "constraint": "NULL"}
                ]
            }],
            "paging": {"total": 1}
        })
    }

    #[test]
    fn openmetadata_schemas_and_tables_become_namespaces_and_table_refs() {
        let schemas = serde_json::json!({
            "data": [
                {"name": "sales", "fullyQualifiedName": "trino.platform.sales"},
                {"name": "raw", "fullyQualifiedName": "trino.platform.raw"}
            ]
        });
        let names: Vec<_> = om_list(&schemas).iter().filter_map(om_name).collect();
        assert_eq!(names, vec!["trino.platform.sales", "trino.platform.raw"]);

        // A bare array is tolerated as well as the data envelope.
        let bare = serde_json::json!([{"name": "raw"}]);
        assert_eq!(om_list(&bare).len(), 1);
    }

    #[test]
    fn openmetadata_columns_carry_types_and_nullability() {
        let response = om_tables_response();
        let table = &om_list(&response)[0];
        let columns = om_columns(table);

        assert_eq!(
            columns
                .iter()
                .map(|column| column.name.as_str())
                .collect::<Vec<_>>(),
            vec!["order_id", "customer_id", "total", "placed_at"]
        );
        assert_eq!(columns[2].data_type, "DECIMAL(12,2)");
        assert!(!columns[0].nullable, "a primary key is not nullable");
        assert!(!columns[1].nullable, "NOT_NULL is not nullable");
        assert!(columns[2].nullable, "no constraint means nullable");
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

    #[tokio::test]
    async fn a_configured_bearer_token_is_attached_to_requests() {
        // No credential and no server: only the configured token is observable.
        let catalog =
            PolarisCatalog::new("c", "http://catalog.invalid", "p").with_token(Some("tok".into()));
        assert_eq!(
            catalog.bearer().await.expect("token").as_deref(),
            Some("tok")
        );

        let anonymous = PolarisCatalog::new("c", "http://catalog.invalid", "p");
        assert!(anonymous.bearer().await.expect("anonymous").is_none());
    }

    #[test]
    fn a_credential_is_split_into_client_id_and_secret() {
        let catalog = PolarisCatalog::new("c", "http://catalog.invalid", "p")
            .with_credential(Some("root:s3cr3t".into()));
        assert_eq!(
            catalog.credential,
            Some(("root".to_string(), "s3cr3t".to_string()))
        );

        // A malformed pair is refused rather than sent as a basic-auth header.
        let malformed = PolarisCatalog::new("c", "http://catalog.invalid", "p")
            .with_credential(Some("no-colon".into()));
        assert_eq!(malformed.credential, None);
    }
}
