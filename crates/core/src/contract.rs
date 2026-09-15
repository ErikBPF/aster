//! Data-contract metadata (ODCS-shaped) used to describe tables to users and to the model.
//!
//! We read the subset we actually use: contract name, optional description and owner, and the
//! ordered schema field list. Everything else in a full ODCS document is ignored.
//! ponytail: JSON only for now; YAML contracts need a yaml crate, add it when a team commits to
//! storing contracts as YAML rather than JSON.

use serde::Serialize;
use serde_json::Value;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContractField {
    pub name: String,
    pub data_type: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DataContract {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub fields: Vec<ContractField>,
}

impl DataContract {
    pub fn parse(id: &str, text: &str) -> Result<Self> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| CoreError::Invalid(format!("{id}: {e}")))?;
        Self::from_json(id, &value)
    }

    pub fn from_json(id: &str, value: &Value) -> Result<Self> {
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Invalid(format!("{id}: contract has no name")))?
            .to_string();

        let fields = value
            .get("schema")
            .and_then(Value::as_array)
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|field| {
                        Some(ContractField {
                            name: field.get("name")?.as_str()?.to_string(),
                            data_type: field
                                .get("type")
                                .or_else(|| field.get("logicalType"))
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            required: field
                                .get("required")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            id: id.to_string(),
            name,
            description: value
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            owner: value
                .get("owner")
                .or_else(|| value.get("team").and_then(|team| team.get("name")))
                .and_then(Value::as_str)
                .map(str::to_string),
            fields,
        })
    }

    /// One-line rendering handed to the model as context.
    pub fn summary(&self) -> String {
        let mut line = format!("{} ({}): ", self.name, self.id);
        if let Some(owner) = &self.owner {
            line.push_str(&format!("owner {owner}; "));
        }
        if let Some(description) = &self.description {
            line.push_str(&format!("{description}; "));
        }
        let fields = self
            .fields
            .iter()
            .map(|field| {
                let mut field_line = field.name.clone();
                if let Some(data_type) = &field.data_type {
                    field_line.push_str(&format!(" {data_type}"));
                }
                if field.required {
                    field_line.push_str(" not null");
                }
                field_line
            })
            .collect::<Vec<_>>()
            .join(", ");
        line.push_str(&fields);
        line
    }
}

/// Contracts whose name appears in the statement, so we only spend tokens on relevant ones.
pub fn relevant<'a>(contracts: &'a [DataContract], sql: &str) -> Vec<&'a DataContract> {
    let sql = sql.to_lowercase();
    contracts
        .iter()
        .filter(|contract| sql.contains(&contract.name.to_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_odcs_shaped_document() {
        let text = r#"{
            "name": "orders",
            "description": "one row per order",
            "owner": "data-team",
            "schema": [
                {"name": "id", "type": "bigint", "required": true},
                {"name": "total", "type": "decimal"}
            ]
        }"#;

        let contract = DataContract::parse("orders.odcs.json", text).expect("parses");
        assert_eq!(contract.name, "orders");
        assert_eq!(contract.owner.as_deref(), Some("data-team"));
        assert_eq!(contract.fields.len(), 2);
        assert!(contract.fields[0].required);
        assert!(!contract.fields[1].required);
        assert!(contract.summary().contains("total decimal"));
    }

    #[test]
    fn rejects_a_contract_without_a_name() {
        let error = DataContract::parse("broken.json", r#"{"schema":[]}"#).expect_err("no name");
        assert!(matches!(error, CoreError::Invalid(_)));
    }

    #[test]
    fn selects_only_contracts_named_in_the_sql() {
        let orders = DataContract::parse("orders", r#"{"name":"orders"}"#).expect("orders");
        let people = DataContract::parse("people", r#"{"name":"people"}"#).expect("people");
        let contracts = vec![orders, people];

        let selected = relevant(&contracts, "SELECT * FROM Orders WHERE id = 1");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "orders");
    }
}
