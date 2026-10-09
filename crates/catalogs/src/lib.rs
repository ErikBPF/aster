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
use serde::Deserialize;
mod transport;
mod trino;
use transport::{invalid, next_token, Budget};

/// Polaris's realm context header. The dev stack runs the single default realm;
/// a multi-realm deployment would have to make this configurable.
const POLARIS_REALM: &str = "POLARIS";

pub struct PolarisCatalog {
    id: CatalogId,
    client: reqwest::Client,
    endpoint: String,
    prefix: String,
    token: Option<String>,
    credential: Option<String>,
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

impl PolarisCatalog {
    /// Supply a client with an explicitly trusted private catalog CA.
    pub fn with_root_certificate(mut self, certificate: reqwest::Certificate) -> Self {
        self.client = transport::client(Some(certificate));
        self
    }

    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        prefix: impl Into<String>,
    ) -> Self {
        Self {
            id: CatalogId::new(id),
            client: transport::client(None),
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
        self.credential = credential;
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
    async fn bearer(&self, budget: &mut Budget) -> Result<Option<String>> {
        if let Some(token) = &self.token {
            return Ok(Some(token.clone()));
        }
        let Some(credential) = &self.credential else {
            return Ok(None);
        };
        let (client_id, client_secret) = credential
            .split_once(':')
            .filter(|(id, secret)| !id.is_empty() && !secret.is_empty())
            .ok_or_else(|| CoreError::Invalid("invalid Polaris credential".into()))?;
        let request = self
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
            ]);
        let payload: serde_json::Value = budget.json(request, "Polaris token exchange").await?;
        payload
            .get("access_token")
            .and_then(|value| value.as_str())
            .filter(|token| !token.trim().is_empty())
            .map(|token| Some(token.to_string()))
            .ok_or_else(|| CoreError::Catalog("polaris token response had no access_token".into()))
    }

    async fn request(&self, path: &str, budget: &mut Budget) -> Result<reqwest::RequestBuilder> {
        let url = format!(
            "{}/api/catalog/v1/{}/{}",
            self.endpoint.trim_end_matches('/'),
            encode_segment(&self.prefix),
            path
        );
        let builder = self.client.get(url).header("Accept", "application/json");
        Ok(match self.bearer(budget).await? {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        })
    }

    async fn generic_request(
        &self,
        path: &[&str],
        budget: &mut Budget,
    ) -> Result<reqwest::RequestBuilder> {
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
        Ok(match self.bearer(budget).await? {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        })
    }

    async fn list_iceberg_tables(
        &self,
        namespace: &str,
        budget: &mut Budget,
    ) -> Result<Vec<TableRef>> {
        let path = format!("namespaces/{}/tables", encode_segment(namespace));
        let mut tables = Vec::new();
        let mut names = HashSet::new();
        for item in self.pages(&path, "identifiers", budget).await? {
            let item: GenericIdentifier =
                serde_json::from_value(item).map_err(|_| invalid("Iceberg identifier"))?;
            aster_core::catalog::namespace_segments("", &item.namespace)?;
            if item.namespace.join("\u{1f}") != namespace
                || item.name.is_empty()
                || !names.insert(item.name.clone())
            {
                return Err(invalid("Iceberg identifier"));
            }
            tables.push(TableRef {
                namespace: item.namespace.join("."),
                namespace_segments: item.namespace,
                name: item.name,
            });
        }
        Ok(tables)
    }

    async fn pages(
        &self,
        path: &str,
        field: &str,
        budget: &mut Budget,
    ) -> Result<Vec<serde_json::Value>> {
        let mut all = Vec::new();
        let mut seen = HashSet::new();
        let mut token: Option<String> = None;
        loop {
            let mut request = self.request(path, budget).await?;
            if let Some(token) = &token {
                request = request.query(&[("pageToken", token)]);
            }
            let page: serde_json::Value = budget.json(request, "Iceberg list").await?;
            all.extend(
                page.get(field)
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| invalid("Iceberg list"))?
                    .iter()
                    .cloned(),
            );
            token = next_token(page.get("next-page-token"), &mut seen)?;
            if token.is_none() {
                return Ok(all);
            }
        }
    }

    async fn list_generic_refs(
        &self,
        namespace: &str,
        budget: &mut Budget,
    ) -> Result<Vec<TableRef>> {
        let mut tables = Vec::new();
        let mut seen_names = HashSet::new();
        let mut seen_tokens = HashSet::new();
        let mut page_token: Option<String> = None;
        loop {
            let request = self
                .generic_request(&["namespaces", namespace, "generic-tables"], budget)
                .await?;
            let request = match &page_token {
                Some(token) => request.query(&[("pageToken", token)]),
                None => request,
            };
            let page: GenericList = budget.json(request, "Generic Table list").await?;
            for item in page.identifiers {
                if item.namespace.join("\u{1f}") != namespace || item.name.trim().is_empty() {
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
                    namespace_segments: namespace.split('\u{1f}').map(str::to_string).collect(),
                    namespace: namespace.replace('\u{1f}', "."),
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

    async fn descriptors(&self, namespace: &str) -> Result<Vec<TableDescriptor>> {
        let budget = &mut Budget::default();
        let iceberg = self.list_iceberg_tables(namespace, budget).await?;
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
            for table in self.list_generic_refs(namespace, budget).await? {
                if !names.insert(table.name.clone()) {
                    return Err(CoreError::Catalog(
                        "Iceberg and Generic Table lists contain the same name".into(),
                    ));
                }
                descriptors.push(self.load_generic(&table, budget).await?);
            }
        }
        Ok(descriptors)
    }

    pub async fn load_generic_table(&self, table: &TableRef) -> Result<TableDescriptor> {
        self.load_generic(table, &mut Budget::default()).await
    }

    async fn load_generic(&self, table: &TableRef, budget: &mut Budget) -> Result<TableDescriptor> {
        let request = self
            .generic_request(
                &[
                    "namespaces",
                    &aster_core::catalog::namespace_segments(
                        &table.namespace,
                        &table.namespace_segments,
                    )?
                    .join("\u{1f}"),
                    "generic-tables",
                    &table.name,
                ],
                budget,
            )
            .await?;
        let loaded: GenericLoad = budget.json(request, "Generic Table load").await?;
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
    fn authoritative_inventory(&self) -> bool {
        true
    }
    fn id(&self) -> &CatalogId {
        &self.id
    }

    fn kind(&self) -> &str {
        "polaris"
    }

    async fn health(&self) -> Health {
        let budget = &mut Budget::default();
        let Ok(request) = self.request("namespaces", budget).await else {
            return Health::Unavailable;
        };
        match budget.status(request, "Polaris health").await {
            Ok(status) if status.is_success() => Health::Healthy,
            Ok(_) => Health::Degraded,
            Err(_) => Health::Unavailable,
        }
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let mut seen = HashSet::new();
        self.pages("namespaces", "namespaces", &mut Budget::default())
            .await?
            .into_iter()
            .map(|item| {
                let segments: Vec<String> =
                    serde_json::from_value(item).map_err(|_| invalid("namespace"))?;
                aster_core::catalog::namespace_segments("", &segments)?;
                if !seen.insert(segments.clone()) {
                    return Err(invalid("duplicate namespace"));
                }
                Ok(Namespace {
                    name: segments.join("."),
                    segments,
                })
            })
            .collect()
    }

    async fn list_tables_qualified(&self, segments: &[String]) -> Result<Vec<TableRef>> {
        let segments = aster_core::catalog::namespace_segments("", segments)?;
        if self.generic_tables_enabled {
            Ok(self
                .list_descriptors_qualified(&segments)
                .await?
                .into_iter()
                .map(|d| d.table)
                .collect())
        } else {
            self.list_iceberg_tables(&segments.join("\u{1f}"), &mut Budget::default())
                .await
        }
    }

    async fn list_descriptors_qualified(
        &self,
        segments: &[String],
    ) -> Result<Vec<TableDescriptor>> {
        let segments = aster_core::catalog::namespace_segments("", segments)?;
        self.descriptors(&segments.join("\u{1f}")).await
    }

    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        aster_core::catalog::namespace_segments(namespace, &[])?;
        if self.generic_tables_enabled {
            Ok(self
                .list_table_descriptors(namespace)
                .await?
                .into_iter()
                .map(|descriptor| descriptor.table)
                .collect())
        } else {
            self.list_iceberg_tables(namespace, &mut Budget::default())
                .await
        }
    }

    async fn list_table_descriptors(&self, namespace: &str) -> Result<Vec<TableDescriptor>> {
        let segments = aster_core::catalog::namespace_segments(namespace, &[])?;
        self.list_descriptors_qualified(&segments).await
    }

    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        let budget = &mut Budget::default();
        if self.generic_tables_enabled
            && self
                .list_generic_refs(
                    &aster_core::catalog::namespace_segments(
                        &table.namespace,
                        &table.namespace_segments,
                    )?
                    .join("\u{1f}"),
                    budget,
                )
                .await?
                .iter()
                .any(|generic| generic.name == table.name)
        {
            return Err(CoreError::Invalid(
                "schema unavailable for a Generic Table".into(),
            ));
        }
        let namespace =
            aster_core::catalog::namespace_segments(&table.namespace, &table.namespace_segments)?
                .join("\u{1f}");
        let path = format!(
            "namespaces/{}/tables/{}",
            encode_segment(&namespace),
            encode_segment(&table.name)
        );
        let request = self.request(&path, budget).await?;
        let payload: serde_json::Value = budget.json(request, "Iceberg table load").await?;
        let metadata = &payload["metadata"];
        let version = metadata["format-version"]
            .as_u64()
            .ok_or_else(|| invalid("Iceberg format version"))?;
        if !(1..=3).contains(&version) {
            return Err(invalid("unsupported Iceberg format version"));
        }
        // Iceberg v1's deprecated schema is the current schema. Never guess from
        // array order when either modern field is present (including malformed).
        let schema = if version == 1
            && metadata.get("schemas").is_none()
            && metadata.get("current-schema-id").is_none()
        {
            &metadata["schema"]
        } else {
            let id = metadata["current-schema-id"]
                .as_u64()
                .ok_or_else(|| invalid("Iceberg current schema id"))?;
            let schemas = metadata["schemas"]
                .as_array()
                .ok_or_else(|| invalid("Iceberg schemas"))?;
            let mut ids = HashSet::new();
            for schema in schemas {
                if !ids.insert(
                    schema["schema-id"]
                        .as_u64()
                        .ok_or_else(|| invalid("Iceberg schema id"))?,
                ) {
                    return Err(invalid("duplicate Iceberg schema id"));
                }
            }
            schemas
                .iter()
                .find(|s| s["schema-id"].as_u64() == Some(id))
                .ok_or_else(|| invalid("missing Iceberg current schema"))?
        };
        if schema["type"].as_str() != Some("struct") {
            return Err(invalid("Iceberg schema type"));
        }
        let fields = schema["fields"]
            .as_array()
            .ok_or_else(|| invalid("Iceberg fields"))?;

        let columns = fields
            .iter()
            .map(|field| {
                Ok(ColumnSchema {
                    name: field["name"]
                        .as_str()
                        .filter(|n| !n.is_empty())
                        .ok_or_else(|| invalid("Iceberg field name"))?
                        .to_string(),
                    data_type: field["type"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| field["type"].as_object().map(|_| field["type"].to_string()))
                        .ok_or_else(|| invalid("Iceberg field type"))?,
                    nullable: !field["required"]
                        .as_bool()
                        .ok_or_else(|| invalid("Iceberg field required"))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(TableSchema {
            table: TableRef {
                namespace_segments: table.namespace_segments.clone(),
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
/// Authentication uses an already-issued API token, never the signing secret.
pub struct CubeCatalog {
    id: CatalogId,
    client: reqwest::Client,
    endpoint: String,
    base_path: String,
    token: Option<String>,
}

impl CubeCatalog {
    /// Supply a client with an explicitly trusted private catalog CA.
    pub fn with_root_certificate(mut self, certificate: reqwest::Certificate) -> Self {
        self.client = transport::client(Some(certificate));
        self
    }

    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        base_path: impl Into<String>,
    ) -> Self {
        Self {
            id: CatalogId::new(id),
            client: transport::client(None),
            endpoint: endpoint.into(),
            base_path: base_path.into(),
            token: None,
        }
    }

    /// The compiled model: cubes and views, each with its measures and dimensions.
    async fn meta(&self) -> Result<serde_json::Value> {
        let url = format!(
            "{}/{}/v1/meta",
            self.endpoint.trim_end_matches('/'),
            self.base_path.trim_matches('/')
        );
        let mut request = self.client.get(url).header("Accept", "application/json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let meta: serde_json::Value = Budget::default().json(request, "Cube meta").await?;
        let cubes = meta
            .get("cubes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| invalid("Cube cubes"))?;
        let views = match meta.get("views") {
            None => &[][..],
            Some(v) => v
                .as_array()
                .ok_or_else(|| invalid("Cube views"))?
                .as_slice(),
        };
        for members in [cubes.as_slice(), views] {
            let mut names = HashSet::new();
            for member in members {
                let name = member["name"]
                    .as_str()
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| invalid("Cube member"))?;
                if !names.insert(name) {
                    return Err(invalid("duplicate Cube member"));
                }
                for kind in ["measures", "dimensions"] {
                    let columns = member[kind]
                        .as_array()
                        .ok_or_else(|| invalid("Cube columns"))?;
                    for column in columns {
                        if column["name"].as_str().is_none_or(str::is_empty)
                            || column["type"].as_str().is_none()
                        {
                            return Err(invalid("Cube column"));
                        }
                    }
                }
            }
        }
        Ok(meta)
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
                segments: vec![(*namespace).to_string()],
                name: (*namespace).to_string(),
            })
            .collect())
    }

    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        let meta = self.meta().await?;
        Ok(cube_members(&meta, namespace)
            .iter()
            .map(|(name, _)| TableRef {
                namespace_segments: namespace.split('\u{1f}').map(str::to_string).collect(),
                namespace: namespace.replace('\u{1f}', "."),
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
/// Registration supplies the configured target's bot token.
pub struct OpenMetadataCatalog {
    id: CatalogId,
    client: reqwest::Client,
    endpoint: String,
    database: Option<String>,
    token: Option<String>,
}

impl OpenMetadataCatalog {
    /// Supply a client with an explicitly trusted private catalog CA.
    pub fn with_root_certificate(mut self, certificate: reqwest::Certificate) -> Self {
        self.client = transport::client(Some(certificate));
        self
    }

    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        database: Option<String>,
        token: Option<String>,
    ) -> Self {
        Self {
            id: CatalogId::new(id),
            client: transport::client(None),
            endpoint: endpoint.into(),
            database,
            token,
        }
    }

    fn request(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/api/v1/{}", self.endpoint.trim_end_matches('/'), path);
        let builder = self.client.get(url).header("Accept", "application/json");
        match &self.token {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    async fn pages(&self, path: &str, params: &[(&str, &str)]) -> Result<Vec<serde_json::Value>> {
        let mut all = Vec::new();
        let mut seen = HashSet::new();
        let mut token: Option<String> = None;
        let budget = &mut Budget::default();
        loop {
            let mut request = self.request(path).query(&[("limit", "200")]).query(params);
            if let Some(token) = &token {
                request = request.query(&[("after", token)]);
            }
            let page: serde_json::Value = budget.json(request, "OpenMetadata list").await?;
            let data = page
                .get("data")
                .or_else(|| page.as_array().map(|_| &page))
                .and_then(|v| v.as_array())
                .ok_or_else(|| invalid("OpenMetadata list"))?;
            if page.get("paging").is_some_and(|p| !p.is_object()) {
                return Err(invalid("OpenMetadata paging"));
            }
            all.extend(data.iter().cloned());
            token = next_token(page.pointer("/paging/after"), &mut seen)?;
            if token.is_none() {
                return Ok(all);
            }
        }
    }

    /// Schemas, optionally restricted to the configured database.
    async fn schemas(&self) -> Result<Vec<serde_json::Value>> {
        let params: Vec<_> = self
            .database
            .as_deref()
            .map(|database| ("database", database))
            .into_iter()
            .collect();
        self.pages("databaseSchemas", &params).await
    }

    /// The tables of a schema, with their columns.
    async fn tables(&self, namespace: &str) -> Result<Vec<serde_json::Value>> {
        self.pages(
            "tables",
            &[("fields", "columns"), ("databaseSchema", namespace)],
        )
        .await
    }
}

/// OpenMetadata answers with `{"data": [...], "paging": {...}}`; tolerate a bare array too.
#[cfg(test)]
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
    async fn list_tables_qualified(&self, segments: &[String]) -> Result<Vec<TableRef>> {
        self.list_tables(opaque_namespace(segments)?).await
    }
    async fn list_descriptors_qualified(
        &self,
        segments: &[String],
    ) -> Result<Vec<TableDescriptor>> {
        self.list_table_descriptors(opaque_namespace(segments)?)
            .await
    }
    fn id(&self) -> &CatalogId {
        &self.id
    }

    fn kind(&self) -> &str {
        "openmetadata"
    }

    async fn health(&self) -> Health {
        match Budget::default()
            .json::<serde_json::Value>(
                self.request("databases").query(&[("limit", "1")]),
                "OpenMetadata health",
            )
            .await
        {
            Ok(_) => Health::Healthy,
            Err(CoreError::Catalog(_)) => Health::Degraded,
            Err(_) => Health::Unavailable,
        }
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let payload = self.schemas().await?;
        let mut seen = HashSet::new();
        payload
            .iter()
            .map(|schema| {
                let name = om_name(schema).ok_or_else(|| invalid("OpenMetadata namespace"))?;
                opaque_namespace(std::slice::from_ref(&name))?;
                if !seen.insert(name.clone()) {
                    return Err(invalid("duplicate OpenMetadata namespace"));
                }
                Ok(Namespace {
                    segments: vec![name.clone()],
                    name,
                })
            })
            .collect()
    }

    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        opaque_namespace(&[namespace.to_string()])?;
        let payload = self.tables(namespace).await?;
        let mut seen = HashSet::new();
        payload
            .iter()
            .map(|table| {
                let name = table["name"]
                    .as_str()
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| invalid("OpenMetadata table"))?;
                if !seen.insert(name) {
                    return Err(invalid("duplicate OpenMetadata table"));
                }
                Ok(TableRef {
                    namespace_segments: vec![namespace.into()],
                    namespace: namespace.into(),
                    name: name.into(),
                })
            })
            .collect()
    }

    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        let namespace = if table.namespace_segments.is_empty() {
            &table.namespace
        } else {
            let name = opaque_namespace(&table.namespace_segments)?;
            if !table.namespace.is_empty() && table.namespace != name {
                return Err(invalid("OpenMetadata identity"));
            }
            name
        };
        opaque_namespace(&[namespace.to_string()])?;
        let payload = self.tables(namespace).await?;
        let mut entities = payload.iter().filter(|entity| {
            entity
                .get("name")
                .and_then(|v| v.as_str())
                .map(|name| name == table.name)
                .unwrap_or(false)
        });
        let entity = entities
            .next()
            .ok_or_else(|| CoreError::NotFound(format!("unknown table: {}", table.name)))?;
        if entities.next().is_some() {
            return Err(invalid("duplicate OpenMetadata table"));
        }
        let columns = entity["columns"]
            .as_array()
            .ok_or_else(|| invalid("OpenMetadata columns"))?;
        for column in columns {
            if column["name"].as_str().is_none_or(str::is_empty)
                || column
                    .get("dataTypeDisplay")
                    .or_else(|| column.get("dataType"))
                    .and_then(|v| v.as_str())
                    .is_none()
            {
                return Err(invalid("OpenMetadata column"));
            }
        }
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
    // Fixture registration only; not evidence about an external physical backend.
    fn authoritative_inventory(&self) -> bool {
        true
    }
    fn metadata_read_bytes(&self) -> Option<usize> {
        Some(0)
    }
    async fn table_schema_bounded(
        &self,
        table: &TableRef,
        _: usize,
        _: usize,
    ) -> Result<TableSchema> {
        self.table_schema(table).await
    }
    async fn list_tables_qualified(&self, segments: &[String]) -> Result<Vec<TableRef>> {
        self.list_tables(opaque_namespace(segments)?).await
    }
    async fn list_descriptors_qualified(
        &self,
        segments: &[String],
    ) -> Result<Vec<TableDescriptor>> {
        self.list_table_descriptors(opaque_namespace(segments)?)
            .await
    }
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
                segments: vec![(*name).into()],
                name: (*name).into(),
            })
            .collect())
    }

    async fn list_tables(&self, namespace: &str) -> Result<Vec<TableRef>> {
        Ok(Self::tables(namespace)
            .iter()
            .map(|(name, _)| TableRef {
                namespace_segments: vec![namespace.into()],
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

/// Resolve explicit secret://KEY references through the caller's selected store.
/// The original configuration retains references; resolved values only reach the adapter.
pub async fn catalog_from_config_with_secrets(
    config: &CatalogConfig,
    secrets: &dyn aster_core::SecretStore,
) -> Result<Arc<dyn Catalog>> {
    let mut resolved = config.clone();
    for value in [&mut resolved.token, &mut resolved.credential] {
        if let Some(key) = value.as_deref().and_then(|v| v.strip_prefix("secret://")) {
            if key.is_empty() || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
                return Err(CoreError::Invalid(
                    "invalid catalog secret reference".into(),
                ));
            }
            *value = Some(
                secrets
                    .get(key)
                    .await
                    .map_err(|_| CoreError::Invalid("catalog secret resolution failed".into()))?
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        CoreError::Invalid("configured catalog secret is unavailable".into())
                    })?,
            );
        }
    }
    catalog_from_config(&resolved)
}

pub fn catalog_from_config(config: &CatalogConfig) -> Result<Arc<dyn Catalog>> {
    if [&config.token, &config.credential]
        .into_iter()
        .flatten()
        .any(|s| s.starts_with("secret://"))
    {
        return Err(CoreError::Invalid(
            "catalog secret reference requires resolution".into(),
        ));
    }
    if matches!(
        config.kind.as_str(),
        "polaris" | "cube" | "openmetadata" | "trino"
    ) {
        let url = reqwest::Url::parse(&config.endpoint)
            .map_err(|_| CoreError::Invalid("invalid catalog endpoint".into()))?;
        if !matches!(url.scheme(), "https" | "http")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(CoreError::Invalid("invalid catalog endpoint".into()));
        }
        if config.token.as_ref().is_some_and(|v| {
            v.is_empty() || reqwest::header::HeaderValue::from_str(&format!("Bearer {v}")).is_err()
        }) {
            return Err(CoreError::Invalid("invalid catalog token".into()));
        }
        if let Some(credential) = &config.credential {
            if config.kind != "polaris"
                || !credential
                    .split_once(':')
                    .is_some_and(|(id, secret)| !id.is_empty() && !secret.is_empty())
            {
                return Err(CoreError::Invalid("invalid catalog credential".into()));
            }
        }
    }
    let catalog: Arc<dyn Catalog> = match config.kind.as_str() {
        "trino" => Arc::new(trino::TrinoCatalog::from_config(config)?),
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
        "cube" => Arc::new(
            CubeCatalog::new(
                &config.id,
                &config.endpoint,
                config
                    .catalog
                    .clone()
                    .unwrap_or_else(|| "cubejs-api".into()),
            )
            .with_token(config.token.clone()),
        ),
        // A metadata source we read from; `catalog` optionally narrows to one database.
        "openmetadata" => Arc::new(OpenMetadataCatalog::new(
            &config.id,
            &config.endpoint,
            config.catalog.clone(),
            config.token.clone(),
        )),
        // Demo provider: canned metadata, for local UI work only.
        "mock" => Arc::new(MockCatalog::new(&config.id)),
        other => return Err(CoreError::Invalid(format!("unknown catalog kind: {other}"))),
    };
    Ok(catalog)
}

fn encode_segment(value: &str) -> String {
    let mut url = reqwest::Url::parse("http://local/").expect("constant URL");
    url.path_segments_mut()
        .expect("hierarchical URL")
        .push(value);
    url.path().trim_start_matches('/').to_string()
}

fn opaque_namespace(segments: &[String]) -> Result<&str> {
    match segments {
        [name] if !name.is_empty() && !name.chars().any(char::is_control) => Ok(name),
        _ => Err(CoreError::Invalid(
            "provider requires one opaque namespace component".into(),
        )),
    }
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
                namespace_segments: vec!["aster_demo".into()],
                namespace: "aster_demo".into(),
                name: "orders".into(),
            })
            .await
            .expect("schema");
        assert_eq!(schema.columns[0].name, "order_id");

        let miss = catalog
            .table_schema(&TableRef {
                namespace_segments: vec!["aster_demo".into()],
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
            catalog
                .bearer(&mut Budget::default())
                .await
                .expect("token")
                .as_deref(),
            Some("tok")
        );

        let anonymous = PolarisCatalog::new("c", "http://catalog.invalid", "p");
        assert!(anonymous
            .bearer(&mut Budget::default())
            .await
            .expect("anonymous")
            .is_none());
    }

    #[test]
    fn a_credential_is_retained_for_validation_before_io() {
        let catalog = PolarisCatalog::new("c", "http://catalog.invalid", "p")
            .with_credential(Some("root:s3cr3t".into()));
        assert_eq!(catalog.credential, Some("root:s3cr3t".to_string()));

        // Retain malformed input so request validation refuses it, never anonymous.
        let malformed = PolarisCatalog::new("c", "http://catalog.invalid", "p")
            .with_credential(Some("no-colon".into()));
        assert_eq!(malformed.credential.as_deref(), Some("no-colon"));
    }
}
