//! Data-contract metadata used to describe tables to users and to the model.
//!
//! We read the parts we use — contract name, optional description and owner, and
//! the ordered field list — from a real Open Data Contract Standard document
//! (v3.2: `schema[].properties[]`, `team.name`, `semanticType`) as well as from
//! the earlier flat JSON shape. YAML and JSON both parse, because a real contract
//! is usually YAML and YAML is a superset of JSON.
//!
//! We deliberately do not validate against the published JSON Schema: it pays
//! off when we *emit* contracts, not when we tolerate them, and the emitter's
//! own check is that a rendered document parses back through this reader
//! (`crate::semantic`). Unknown sections are ignored, which is what
//! `additionalProperties: false` would forbid.

use serde::Serialize;
use serde_json::Value;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContractField {
    pub name: String,
    pub data_type: Option<String>,
    pub required: bool,
    /// ODCS v3.2 `semanticType`: `column`, `measure` or `dimension`. Carried
    /// through because a measure is what a Cube model will need.
    pub semantic_type: Option<String>,
    /// ODCS v3.2 `transformLogic`, e.g. `SUM(total)`. The aggregation an
    /// emitter has to reproduce when the field is a measure.
    pub transform_logic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DataContract {
    /// Local identifier: the file name it was loaded from.
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub fields: Vec<ContractField>,
}

impl DataContract {
    /// Parses YAML or JSON. A contract must name itself, through `name` or — as a
    /// real ODCS document does — through `id`.
    pub fn parse(id: &str, text: &str) -> Result<Self> {
        let value: Value = serde_norway::from_str(text).map_err(|error| {
            CoreError::Invalid(format!("{id}: not valid YAML or JSON: {error}"))
        })?;
        Self::from_value(id, &value)
    }

    pub fn from_value(id: &str, value: &Value) -> Result<Self> {
        let name = value
            .get("name")
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| CoreError::Invalid(format!("{id}: contract has no name or id")))?
            .to_string();

        Ok(Self {
            id: id.to_string(),
            name,
            description: string(value, "description"),
            owner: value
                .get("owner")
                .or_else(|| value.get("team").and_then(|team| team.get("name")))
                .and_then(Value::as_str)
                .filter(|owner| !owner.trim().is_empty())
                .map(str::to_string),
            fields: fields(value),
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
                if let Some(semantic_type) = &field.semantic_type {
                    field_line.push_str(&format!(" ({semantic_type})"));
                }
                field_line
            })
            .collect::<Vec<_>>()
            .join(", ");
        line.push_str(&fields);
        line
    }
}

fn string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

/// ODCS v3: `schema` is a list of objects, each with an ordered `properties`
/// list. The flat shape puts the fields directly in `schema`. Both are read.
fn fields(value: &Value) -> Vec<ContractField> {
    value
        .get("schema")
        .and_then(Value::as_array)
        .map(|objects| {
            objects
                .iter()
                .flat_map(
                    |object| match object.get("properties").and_then(Value::as_array) {
                        Some(properties) => properties
                            .iter()
                            .filter_map(field)
                            .collect::<Vec<ContractField>>(),
                        None => field(object).into_iter().collect(),
                    },
                )
                .collect()
        })
        .unwrap_or_default()
}

fn field(value: &Value) -> Option<ContractField> {
    Some(ContractField {
        name: value.get("name")?.as_str()?.to_string(),
        data_type: value
            .get("logicalType")
            .or_else(|| value.get("type"))
            .and_then(Value::as_str)
            .map(str::to_string),
        required: value
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        semantic_type: value
            .get("semanticType")
            .and_then(Value::as_str)
            .map(str::to_string),
        transform_logic: value
            .get("transformLogic")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// The contract describing a table, matched by name or id, case-insensitively.
pub fn for_table<'a>(contracts: &'a [DataContract], table: &str) -> Option<&'a DataContract> {
    contracts.iter().find(|contract| {
        contract.name.eq_ignore_ascii_case(table) || contract.id.eq_ignore_ascii_case(table)
    })
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
    fn parses_a_real_odcs_v3_yaml_contract() {
        let text = r#"version: 3.2.0
apiVersion: v3.2.0
kind: DataContract
id: orders
name: orders
status: active
team:
  name: data-team
schema:
  - name: orders
    logicalType: object
    properties:
      - name: order_id
        logicalType: bigint
        required: true
        primaryKey: true
      - name: total
        logicalType: decimal(12,2)
        semanticType: measure
      - name: placed_at
        logicalType: timestamp
        required: true
"#;

        let contract = DataContract::parse("orders.odcs.yaml", text).expect("parses");
        assert_eq!(contract.name, "orders");
        assert_eq!(contract.owner.as_deref(), Some("data-team"));
        assert_eq!(
            contract
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            vec!["order_id", "total", "placed_at"]
        );
        assert!(contract.fields[0].required);
        assert_eq!(contract.fields[1].semantic_type.as_deref(), Some("measure"));
        assert!(contract.summary().contains("total decimal(12,2) (measure)"));
    }

    #[test]
    fn a_document_named_only_by_id_still_parses() {
        let contract = DataContract::parse("id-only.yaml", "id: customers\n").expect("parses");
        assert_eq!(contract.name, "customers");
        assert!(contract.fields.is_empty());
    }

    #[test]
    fn a_measure_keeps_its_aggregation() {
        let text = r#"version: 3.2.0
apiVersion: v3.2.0
kind: DataContract
id: orders
schema:
  - name: orders
    properties:
      - name: total
        logicalType: decimal(12,2)
        semanticType: measure
        transformLogic: SUM(total)
"#;

        let contract = DataContract::parse("orders.odcs.yaml", text).expect("parses");
        assert_eq!(
            contract.fields[0].transform_logic.as_deref(),
            Some("SUM(total)")
        );
    }

    #[test]
    fn finds_the_contract_of_a_table_by_name_or_id() {
        let contracts = vec![
            DataContract::parse("people", r#"{"name":"people"}"#).expect("people"),
            DataContract::parse("orders", r#"{"name":"orders"}"#).expect("orders"),
        ];

        assert_eq!(
            for_table(&contracts, "ORDERS").map(|contract| contract.name.as_str()),
            Some("orders")
        );
        assert!(for_table(&contracts, "sales").is_none());
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
