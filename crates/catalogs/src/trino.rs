//! Native Trino metadata. All queries target the exact configured backend alias.
use crate::transport::{self, invalid, Budget};
use aster_core::{
    Catalog, CatalogConfig, CatalogId, ColumnSchema, CoreError, Health, Namespace, Result,
    TableDescriptor, TableRef, TableSchema,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) struct TrinoCatalog {
    id: CatalogId,
    endpoint: reqwest::Url,
    catalog: String,
    token: Option<String>,
    client: reqwest::Client,
}

fn name(value: &str) -> Result<&str> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(invalid("Trino identifier"));
    }
    Ok(value)
}

fn namespace(segments: &[String]) -> Result<&str> {
    match segments {
        [part] => name(part),
        _ => Err(invalid("Trino namespace")),
    }
}

fn literal(value: &str) -> Result<String> {
    Ok(format!("'{}'", name(value)?.replace('\'', "''")))
}

impl TrinoCatalog {
    pub(crate) fn from_config(config: &CatalogConfig) -> Result<Self> {
        let catalog = config
            .catalog
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("Trino catalog alias is required".into()))?;
        name(catalog)?;
        Ok(Self {
            id: CatalogId::new(&config.id),
            endpoint: reqwest::Url::parse(&format!(
                "{}/v1/statement",
                config.endpoint.trim_end_matches('/')
            ))
            .map_err(|_| invalid("Trino endpoint"))?,
            catalog: catalog.to_string(),
            token: config.token.clone(),
            client: transport::client(None),
        })
    }

    fn relation(&self, table: &str) -> String {
        format!(
            "\"{}\".information_schema.{table}",
            self.catalog.replace('"', "\"\"")
        )
    }

    fn headers(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let request = request
            .header("X-Trino-User", "aster")
            .header("X-Trino-Source", "aster");
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn query(&self, sql: String, fields: &[&str]) -> Result<Vec<Vec<String>>> {
        let mut budget = Budget::default();
        let mut request = self.client.post(self.endpoint.clone()).body(sql);
        let mut rows = Vec::new();
        let mut seen_uris = HashSet::new();
        let mut seen_names = HashSet::new();
        let mut query_id = None;
        let mut has_columns = false;
        loop {
            let page: Value = budget.json(self.headers(request), "Trino metadata").await?;
            if page.get("error").is_some() {
                return Err(CoreError::Catalog("Trino metadata query failed".into()));
            }
            let id = page["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid("Trino query id"))?;
            if query_id.as_deref().is_some_and(|previous| previous != id) {
                return Err(invalid("Trino query id changed"));
            }
            query_id = Some(id.to_string());
            let state = page
                .pointer("/stats/state")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("Trino query state"))?;
            if !matches!(
                state,
                "QUEUED"
                    | "WAITING_FOR_RESOURCES"
                    | "DISPATCHING"
                    | "PLANNING"
                    | "STARTING"
                    | "RUNNING"
                    | "FINISHING"
                    | "FINISHED"
            ) {
                return Err(invalid("Trino query state"));
            }
            if let Some(columns) = page.get("columns") {
                let columns = columns.as_array().ok_or_else(|| invalid("Trino columns"))?;
                if columns.len() != fields.len()
                    || columns.iter().zip(fields).any(|(c, f)| {
                        c["name"].as_str() != Some(*f)
                            || c["type"].as_str().is_none_or(str::is_empty)
                    })
                {
                    return Err(invalid("Trino columns"));
                }
                has_columns = true;
            }
            if let Some(data) = page.get("data") {
                if !has_columns {
                    return Err(invalid("Trino missing columns"));
                }
                let data = data.as_array().ok_or_else(|| invalid("Trino rows"))?;
                if rows.len() + data.len() > 10_000 {
                    return Err(CoreError::Catalog(
                        "incomplete catalog observation: row limit".into(),
                    ));
                }
                for row in data {
                    let row = row
                        .as_array()
                        .filter(|r| r.len() == fields.len())
                        .ok_or_else(|| invalid("Trino row"))?;
                    let row = row
                        .iter()
                        .map(|v| {
                            name(v.as_str().ok_or_else(|| invalid("Trino cell"))?)
                                .map(str::to_string)
                        })
                        .collect::<Result<Vec<_>>>()?;
                    if !seen_names.insert(row[0].clone()) {
                        return Err(invalid("duplicate Trino metadata name"));
                    }
                    rows.push(row);
                }
            }
            match page.get("nextUri") {
                None if state == "FINISHED" && has_columns => return Ok(rows),
                Some(Value::String(next)) if !next.is_empty() && next.len() <= 8192 => {
                    let next =
                        reqwest::Url::parse(next).map_err(|_| invalid("Trino continuation"))?;
                    // Validate before adding credentials. Never rewrite an untrusted origin.
                    if next.origin() != self.endpoint.origin()
                        || !next.username().is_empty()
                        || next.password().is_some()
                        || next.fragment().is_some()
                        || !seen_uris.insert(next.to_string())
                    {
                        return Err(invalid("Trino continuation"));
                    }
                    request = self.client.get(next);
                }
                _ => return Err(invalid("incomplete Trino result")),
            }
        }
    }
}

#[async_trait]
impl Catalog for TrinoCatalog {
    fn id(&self) -> &CatalogId {
        &self.id
    }
    fn kind(&self) -> &str {
        "trino"
    }
    fn authoritative_inventory(&self) -> bool {
        true
    }
    // AI's caller-supplied byte/read ceilings are not implemented: retain opt-out.
    async fn health(&self) -> Health {
        match self.list_namespaces().await {
            Ok(_) => Health::Healthy,
            Err(_) => Health::Unavailable,
        }
    }
    async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        Ok(self
            .query(
                format!(
                    "SELECT schema_name FROM {} ORDER BY schema_name",
                    self.relation("schemata")
                ),
                &["schema_name"],
            )
            .await?
            .into_iter()
            .map(|row| Namespace {
                name: row[0].clone(),
                segments: vec![row[0].clone()],
            })
            .collect())
    }
    async fn list_tables(&self, ns: &str) -> Result<Vec<TableRef>> {
        let segments = aster_core::catalog::namespace_segments(ns, &[])?;
        self.list_tables_qualified(&segments).await
    }
    async fn list_tables_qualified(&self, segments: &[String]) -> Result<Vec<TableRef>> {
        let ns = namespace(segments)?;
        Ok(self
            .query(
                format!(
                    "SELECT table_name FROM {} WHERE table_schema = {} ORDER BY table_name",
                    self.relation("tables"),
                    literal(ns)?
                ),
                &["table_name"],
            )
            .await?
            .into_iter()
            .map(|row| TableRef {
                namespace: ns.into(),
                namespace_segments: segments.to_vec(),
                name: row[0].clone(),
            })
            .collect())
    }
    async fn list_descriptors_qualified(
        &self,
        segments: &[String],
    ) -> Result<Vec<TableDescriptor>> {
        Ok(self
            .list_tables_qualified(segments)
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
    async fn table_schema(&self, table: &TableRef) -> Result<TableSchema> {
        let segments =
            aster_core::catalog::namespace_segments(&table.namespace, &table.namespace_segments)?;
        let ns = namespace(&segments)?;
        let rows = self.query(format!("SELECT column_name, data_type, is_nullable FROM {} WHERE table_schema = {} AND table_name = {} ORDER BY ordinal_position", self.relation("columns"), literal(ns)?, literal(&table.name)?), &["column_name", "data_type", "is_nullable"]).await?;
        if rows.is_empty() {
            return Err(CoreError::NotFound("Trino table schema unavailable".into()));
        }
        let columns = rows
            .into_iter()
            .map(|row| {
                Ok(ColumnSchema {
                    name: row[0].clone(),
                    data_type: row[1].clone(),
                    nullable: match row[2].as_str() {
                        "YES" => true,
                        "NO" => false,
                        _ => return Err(invalid("Trino nullability")),
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(TableSchema {
            table: TableRef {
                namespace: ns.into(),
                namespace_segments: segments,
                name: table.name.clone(),
            },
            columns,
        })
    }
}
