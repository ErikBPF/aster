//! Rendering catalog metadata as the files a team commits.
//!
//! The `Catalog` trait navigates metadata; this module is the write direction.
//! One table's schema, plus the data contract that describes it when one is
//! loaded, becomes a Cube model for the semantic layer or an Open Data Contract
//! Standard document for the producer/consumer agreement.
//!
//! Emission is pure: no IO, no store, no clock, so the output is reproducible
//! and testable. A new target is one [`SemanticFormat`] implementation plus one
//! arm in [`format`].

use std::sync::Arc;

use crate::catalog::{TableRef, TableSchema};
use crate::contract::DataContract;
use crate::error::{CoreError, Result};

/// What one rendering reads: the catalog's view of a table, and the contract
/// that describes it when one with the same name is loaded.
pub struct TableModel<'a> {
    pub schema: &'a TableSchema,
    pub contract: Option<&'a DataContract>,
}

/// One field as the emitter sees it, taken from the contract when there is one
/// and from the catalog otherwise.
struct Field<'a> {
    name: &'a str,
    data_type: Option<&'a str>,
    required: bool,
    semantic: Option<&'a str>,
    transform: Option<&'a str>,
}

impl<'a> TableModel<'a> {
    /// The contract wins when there is one: it carries meaning the catalog does
    /// not — semantic types, required fields, an aggregation.
    fn fields(&self) -> Vec<Field<'a>> {
        match self.contract {
            Some(contract) => contract
                .fields
                .iter()
                .map(|field| Field {
                    name: &field.name,
                    data_type: field.data_type.as_deref(),
                    required: field.required,
                    semantic: field.semantic_type.as_deref(),
                    transform: field.transform_logic.as_deref(),
                })
                .collect(),
            None => self
                .schema
                .columns
                .iter()
                .map(|column| Field {
                    name: &column.name,
                    data_type: Some(&column.data_type),
                    required: !column.nullable,
                    semantic: None,
                    transform: None,
                })
                .collect(),
        }
    }

    fn name(&self) -> &'a str {
        self.contract
            .map(|contract| contract.name.as_str())
            .unwrap_or(&self.schema.table.name)
    }

    fn description(&self) -> Option<&'a str> {
        self.contract
            .and_then(|contract| contract.description.as_deref())
    }

    fn owner(&self) -> Option<&'a str> {
        self.contract.and_then(|contract| contract.owner.as_deref())
    }
}

/// A target format: Cube's semantic model or an ODCS data contract.
pub trait SemanticFormat: Send + Sync {
    /// Target name, used in the request and in the suggested path.
    fn target(&self) -> &'static str;
    /// Suggested repository path for the rendered file.
    fn path(&self, table: &TableRef) -> String;
    fn render(&self, model: &TableModel<'_>) -> Result<String>;
}

/// Resolves a target by name; an unknown one is refused by name.
pub fn format(target: &str) -> Result<Arc<dyn SemanticFormat>> {
    match target {
        "cube" => Ok(Arc::new(CubeFormat)),
        "odcs" => Ok(Arc::new(OdcsFormat)),
        other => Err(CoreError::Invalid(format!(
            "unknown semantic target {other}: expected cube or odcs"
        ))),
    }
}

pub struct CubeFormat;

pub struct OdcsFormat;

impl SemanticFormat for CubeFormat {
    fn target(&self) -> &'static str {
        "cube"
    }

    fn path(&self, table: &TableRef) -> String {
        format!("model/cubes/{}.yml", file_name(&table.name))
    }

    fn render(&self, model: &TableModel<'_>) -> Result<String> {
        let fields = model.fields();
        let dimensions: Vec<&Field<'_>> =
            fields.iter().filter(|field| !is_measure(field)).collect();
        let measures: Vec<&Field<'_>> = fields.iter().filter(|field| is_measure(field)).collect();

        let mut out = header();
        out.push_str("cubes:\n");
        out.push_str(&format!("  - name: {}\n", quoted(&file_name(model.name()))));
        out.push_str(&format!(
            "    sql_table: {}\n",
            quoted(&format!(
                "{}.{}",
                model.schema.table.namespace, model.schema.table.name
            ))
        ));
        if let Some(description) = model.description() {
            out.push_str(&format!("    description: {}\n", quoted(description)));
        }

        if !dimensions.is_empty() {
            out.push_str("    dimensions:\n");
            for field in dimensions {
                out.push_str(&format!(
                    "      - name: {}\n        sql: {}\n        type: {}\n",
                    quoted(field.name),
                    quoted(field.name),
                    quoted(cube_type(field.data_type))
                ));
            }
        }

        if !measures.is_empty() {
            if measures.iter().any(|field| field.transform.is_none()) {
                out.push_str("    # Aggregation is assumed to be sum; set transformLogic on the contract field.\n");
            }
            out.push_str("    measures:\n");
            for field in measures {
                out.push_str(&format!(
                    "      - name: {}\n        sql: {}\n        type: {}\n",
                    quoted(field.name),
                    quoted(field.transform.unwrap_or(field.name)),
                    quoted(aggregation(field.transform))
                ));
            }
        }

        Ok(out)
    }
}

impl SemanticFormat for OdcsFormat {
    fn target(&self) -> &'static str {
        "odcs"
    }

    fn path(&self, table: &TableRef) -> String {
        format!("contracts/{}.yaml", file_name(&table.name))
    }

    fn render(&self, model: &TableModel<'_>) -> Result<String> {
        let mut out = header();
        out.push_str("version: 3.2.0\napiVersion: v3.2.0\nkind: DataContract\n");
        out.push_str(&format!(
            "id: {}\n",
            quoted(&format!(
                "{}.{}",
                model.schema.table.namespace,
                model.name()
            ))
        ));
        out.push_str(&format!("name: {}\n", quoted(model.name())));
        out.push_str("status: draft\n");
        if let Some(description) = model.description() {
            out.push_str(&format!("description: {}\n", quoted(description)));
        }
        if let Some(owner) = model.owner() {
            out.push_str(&format!("team:\n  name: {}\n", quoted(owner)));
        }

        out.push_str("schema:\n");
        out.push_str(&format!(
            "  - name: {}\n    logicalType: \"object\"\n    properties:\n",
            quoted(&model.schema.table.name)
        ));
        for field in model.fields() {
            out.push_str(&format!("      - name: {}\n", quoted(field.name)));
            if let Some(data_type) = field.data_type {
                out.push_str(&format!("        logicalType: {}\n", quoted(data_type)));
            }
            if field.required {
                out.push_str("        required: true\n");
            }
            out.push_str(&format!(
                "        semanticType: {}\n",
                quoted(field.semantic.unwrap_or("column"))
            ));
            if let Some(transform) = field.transform {
                out.push_str(&format!("        transformLogic: {}\n", quoted(transform)));
            }
        }

        Ok(out)
    }
}

fn header() -> String {
    String::from("# Generated by aster from catalog metadata; review before committing.\n")
}

/// Cube member names take letters, digits and underscores, and start with a
/// letter; the same shape keeps a file name safe.
fn file_name(table: &str) -> String {
    let mut name: String = table
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if !name.starts_with(|character: char| character.is_ascii_alphabetic()) {
        name.insert_str(0, "t_");
    }
    name
}

fn is_measure(field: &Field<'_>) -> bool {
    field
        .semantic
        .is_some_and(|semantic| semantic.eq_ignore_ascii_case("measure"))
}

/// Cube states the aggregation in the measure's `type`. An ODCS transform
/// spells it out; otherwise we assume `sum` and say so in the file.
fn aggregation(transform: Option<&str>) -> &'static str {
    let Some(transform) = transform else {
        return "sum";
    };
    let function = transform
        .split(['(', ' ', '\t'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match function.as_str() {
        "count" => "count",
        "count_distinct" => "count_distinct",
        "avg" | "average" => "avg",
        "min" => "min",
        "max" => "max",
        _ => "sum",
    }
}

fn cube_type(data_type: Option<&str>) -> &'static str {
    let data_type = data_type.unwrap_or_default().to_ascii_lowercase();
    if data_type.contains("time") || data_type.contains("date") {
        "time"
    } else if data_type.contains("bool") {
        "boolean"
    } else if [
        "int", "decimal", "numeric", "double", "real", "float", "number",
    ]
    .iter()
    .any(|needle| data_type.contains(needle))
    {
        "number"
    } else {
        "string"
    }
}

/// YAML double-quoted scalar. Quoting everything keeps descriptions and type
/// names out of YAML's plain-scalar edge cases.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ColumnSchema;

    fn schema() -> TableSchema {
        TableSchema {
            table: TableRef {
                namespace_segments: Vec::new(),
                namespace: "sales".into(),
                name: "orders".into(),
            },
            columns: vec![
                ColumnSchema {
                    name: "order_id".into(),
                    data_type: "bigint".into(),
                    nullable: false,
                },
                ColumnSchema {
                    name: "customer".into(),
                    data_type: "varchar".into(),
                    nullable: true,
                },
                ColumnSchema {
                    name: "total".into(),
                    data_type: "decimal(12,2)".into(),
                    nullable: false,
                },
                ColumnSchema {
                    name: "placed_at".into(),
                    data_type: "timestamp".into(),
                    nullable: false,
                },
            ],
        }
    }

    fn contract() -> DataContract {
        DataContract::parse(
            "orders",
            r#"{
                "name": "orders",
                "description": "one row per order",
                "team": {"name": "data-team"},
                "schema": [
                    {"name": "order_id", "type": "bigint", "required": true},
                    {"name": "total", "type": "decimal(12,2)", "semanticType": "measure",
                     "transformLogic": "SUM(total)"}
                ]
            }"#,
        )
        .expect("parses")
    }

    #[test]
    fn renders_a_cube_model_from_catalog_columns() {
        let schema = schema();
        let text = CubeFormat
            .render(&TableModel {
                schema: &schema,
                contract: None,
            })
            .expect("renders");

        assert!(text.contains("  - name: \"orders\""));
        assert!(text.contains("    sql_table: \"sales.orders\""));
        assert!(text.contains(
            "      - name: \"placed_at\"\n        sql: \"placed_at\"\n        type: \"time\""
        ));
        assert!(text
            .contains("      - name: \"total\"\n        sql: \"total\"\n        type: \"number\""));
        assert!(!text.contains("measures:"));
    }

    #[test]
    fn a_contract_measure_becomes_a_cube_measure() {
        let schema = schema();
        let contract = contract();
        let text = CubeFormat
            .render(&TableModel {
                schema: &schema,
                contract: Some(&contract),
            })
            .expect("renders");

        assert!(text.contains("    description: \"one row per order\""));
        assert!(text.contains("    dimensions:\n      - name: \"order_id\""));
        assert!(text.contains(
            "    measures:\n      - name: \"total\"\n        sql: \"SUM(total)\"\n        type: \"sum\""
        ));
        assert!(!text.contains("assumed to be sum"));
    }

    #[test]
    fn a_measure_without_a_transform_is_summed_and_flagged() {
        let schema = schema();
        let contract = DataContract::parse(
            "orders",
            r#"{"name":"orders","schema":[{"name":"total","semanticType":"measure"}]}"#,
        )
        .expect("parses");
        let text = CubeFormat
            .render(&TableModel {
                schema: &schema,
                contract: Some(&contract),
            })
            .expect("renders");

        assert!(text.contains("assumed to be sum"));
        assert!(text.contains("        sql: \"total\"\n        type: \"sum\""));
    }

    #[test]
    fn renders_an_odcs_contract_that_parses_back() {
        let schema = schema();
        let text = OdcsFormat
            .render(&TableModel {
                schema: &schema,
                contract: None,
            })
            .expect("renders");

        let parsed = DataContract::parse("rendered.yaml", &text).expect("parses back");
        assert_eq!(parsed.name, "orders");
        assert_eq!(parsed.fields.len(), 4);
        assert!(parsed.fields[0].required);
        assert_eq!(parsed.fields[0].semantic_type.as_deref(), Some("column"));
    }

    #[test]
    fn the_contracts_semantics_reach_the_emitted_contract() {
        let schema = schema();
        let contract = contract();
        let text = OdcsFormat
            .render(&TableModel {
                schema: &schema,
                contract: Some(&contract),
            })
            .expect("renders");

        let parsed = DataContract::parse("rendered.yaml", &text).expect("parses back");
        assert_eq!(parsed.owner.as_deref(), Some("data-team"));
        assert_eq!(parsed.name, "orders");
        assert_eq!(parsed.fields[1].semantic_type.as_deref(), Some("measure"));
        assert_eq!(
            parsed.fields[1].transform_logic.as_deref(),
            Some("SUM(total)")
        );
    }

    #[test]
    fn an_unknown_target_is_refused_by_name() {
        let Err(error) = format("lookml") else {
            panic!("expected a refusal");
        };
        assert!(matches!(error, CoreError::Invalid(_)));
        assert!(error.to_string().contains("lookml"));
    }

    #[test]
    fn file_names_keep_dashes_and_digits_out_of_the_way() {
        let dashed = TableRef {
            namespace_segments: Vec::new(),
            namespace: "sales".into(),
            name: "order-facts".into(),
        };
        assert_eq!(CubeFormat.path(&dashed), "model/cubes/order_facts.yml");
        assert_eq!(OdcsFormat.path(&dashed), "contracts/order_facts.yaml");

        let digits = TableSchema {
            table: TableRef {
                namespace_segments: Vec::new(),
                namespace: "sales".into(),
                name: "2orders".into(),
            },
            columns: vec![],
        };
        let text = CubeFormat
            .render(&TableModel {
                schema: &digits,
                contract: None,
            })
            .expect("renders");
        assert!(text.contains("  - name: \"t_2orders\""));
    }

    #[test]
    fn an_unsafe_description_is_quoted_into_one_line() {
        let schema = schema();
        let contract = DataContract::parse("orders", "name: orders\ndescription: \"a: b\\nc\"\n")
            .expect("parses");
        let text = OdcsFormat
            .render(&TableModel {
                schema: &schema,
                contract: Some(&contract),
            })
            .expect("renders");
        assert!(text.contains("description: \"a: b\\nc\""));
    }
}
