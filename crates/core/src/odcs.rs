//! Preserved ODCS input, separate from legacy response projections and access grants.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CoreError, Result};

#[derive(Clone)]
pub struct OdcsDocument {
    pub source: Vec<u8>,
    pub raw: Value,
    pub projection: DocumentProjection,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentProjection {
    pub id: String,
    pub version: String,
    pub api_version: String,
    pub kind: String,
    pub name: Option<String>,
    pub description: Option<Value>,
    pub team: Option<Value>,
    #[serde(default)]
    pub servers: Vec<Value>,
    #[serde(default)]
    pub schema: Vec<SchemaObject>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaObject {
    pub id: Option<String>,
    pub name: String,
    pub physical_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<Property>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub id: Option<String>,
    pub name: String,
    pub physical_name: Option<String>,
    pub logical_type: Option<String>,
    pub physical_type: Option<String>,
    pub description: Option<String>,
    pub semantic_type: Option<String>,
    pub transform_logic: Option<String>,
    #[serde(default)]
    pub properties: Vec<Property>,
}

impl OdcsDocument {
    /// Syntax and projection only. The server separately applies the pinned schema.
    pub fn parse(source: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(source)
            .map_err(|_| CoreError::Invalid("ODCS input is not UTF-8".into()))?;
        // Norway's Value mapping rejects duplicate keys, unlike a JSON Value visitor.
        let yaml: serde_norway::Value = serde_norway::from_str(text)
            .map_err(|_| CoreError::Invalid("invalid or ambiguous ODCS YAML".into()))?;
        let raw = serde_json::to_value(yaml)
            .map_err(|_| CoreError::Invalid("ODCS input is not a JSON-compatible tree".into()))?;
        if raw.get("apiVersion").is_none() {
            return Err(CoreError::Invalid(
                "legacy contract format is not supported".into(),
            ));
        }
        if raw["apiVersion"] != "v3.2.0" {
            return Err(CoreError::Invalid("unsupported ODCS apiVersion".into()));
        }
        let projection: DocumentProjection = serde_json::from_value(raw.clone())
            .map_err(|_| CoreError::Invalid("invalid ODCS document projection".into()))?;
        if projection.kind != "DataContract"
            || projection.id.is_empty()
            || projection.version.is_empty()
        {
            return Err(CoreError::Invalid("invalid ODCS identity".into()));
        }
        unique(
            projection
                .schema
                .iter()
                .map(|o| (o.id.as_deref(), o.name.as_str())),
        )?;
        for object in &projection.schema {
            properties_unique(&object.properties)?;
        }
        Ok(Self {
            source: source.to_vec(),
            raw,
            projection,
        })
    }
}

fn unique<'a>(items: impl Iterator<Item = (Option<&'a str>, &'a str)>) -> Result<()> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for (id, name) in items {
        if name.is_empty()
            || !names.insert(name)
            || id.is_some_and(|id| id.is_empty() || !ids.insert(id))
        {
            return Err(CoreError::Invalid("ambiguous ODCS identity".into()));
        }
    }
    Ok(())
}

fn properties_unique(properties: &[Property]) -> Result<()> {
    unique(
        properties
            .iter()
            .map(|p| (p.id.as_deref(), p.name.as_str())),
    )?;
    for property in properties {
        properties_unique(&property.properties)?;
    }
    Ok(())
}
