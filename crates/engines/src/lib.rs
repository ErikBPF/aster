//! Query-engine plugins. Each module adapts one engine's wire protocol to the
//! `QueryEngine` trait. Registering a new engine is one module plus one arm in
//! `engine_from_config`.

use std::sync::Arc;

use aster_core::{
    Column, CoreError, EngineConfig, EngineId, EngineInfo, Health, QueryEngine, QueryRequest,
    QueryResult, Result,
};
use async_trait::async_trait;

pub struct TrinoEngine {
    info: EngineInfo,
    client: reqwest::Client,
    user: String,
}

impl TrinoEngine {
    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        routing_group: Option<String>,
    ) -> Self {
        Self {
            info: EngineInfo {
                id: EngineId::new(id),
                kind: "trino".into(),
                endpoint: endpoint.into(),
                routing_group,
            },
            client: reqwest::Client::new(),
            user: "aster".into(),
        }
    }
}

#[async_trait]
impl QueryEngine for TrinoEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    async fn health(&self) -> Health {
        let url = format!("{}/v1/info", self.info.endpoint.trim_end_matches('/'));
        match self.client.get(url).send().await {
            Ok(response) if response.status().is_success() => Health::Healthy,
            Ok(_) => Health::Degraded,
            Err(_) => Health::Unavailable,
        }
    }

    async fn execute(&self, request: QueryRequest) -> Result<QueryResult> {
        let url = format!("{}/v1/statement", self.info.endpoint.trim_end_matches('/'));
        let max_rows = request.max_rows.unwrap_or(1000);

        let mut payload: serde_json::Value = self
            .client
            .post(url)
            .header("X-Trino-User", &self.user)
            .header("X-Trino-Source", "aster")
            .header("X-Trino-Client-Tags", "aster")
            .headers(routing_headers(&self.info))
            .body(request.sql)
            .send()
            .await
            .map_err(|error| CoreError::Engine(error.to_string()))?
            .json()
            .await
            .map_err(|error| CoreError::Engine(error.to_string()))?;

        let mut columns: Vec<Column> = Vec::new();
        let mut rows: Vec<Vec<serde_json::Value>> = Vec::new();
        let mut truncated = false;

        // ponytail: follows nextUri but never issues the closing DELETE, so the
        // engine may hold resources briefly. Add the DELETE when load matters.
        loop {
            if columns.is_empty() {
                if let Some(found) = payload.get("columns").and_then(|value| value.as_array()) {
                    columns = found
                        .iter()
                        .map(|column| Column {
                            name: column["name"].as_str().unwrap_or_default().to_string(),
                            data_type: column["type"].as_str().unwrap_or_default().to_string(),
                        })
                        .collect();
                }
            }

            if let Some(error) = payload.get("error") {
                return Err(CoreError::Engine(
                    error["message"]
                        .as_str()
                        .unwrap_or("trino error")
                        .to_string(),
                ));
            }

            if let Some(data) = payload.get("data").and_then(|value| value.as_array()) {
                for row in data {
                    if rows.len() >= max_rows {
                        truncated = true;
                        break;
                    }
                    rows.push(row.as_array().cloned().unwrap_or_default());
                }
            }

            if truncated {
                break;
            }

            match payload.get("nextUri").and_then(|value| value.as_str()) {
                Some(next) => {
                    payload = self
                        .client
                        .get(next)
                        .send()
                        .await
                        .map_err(|error| CoreError::Engine(error.to_string()))?
                        .json()
                        .await
                        .map_err(|error| CoreError::Engine(error.to_string()))?;
                }
                None => break,
            }
        }

        Ok(QueryResult {
            columns,
            rows,
            truncated,
        })
    }
}

/// Trino Gateway picks the backend pool from this header; without it the
/// gateway uses its own default routing group.
fn routing_headers(info: &EngineInfo) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(group) = &info.routing_group {
        if let Ok(value) = reqwest::header::HeaderValue::from_str(group) {
            headers.insert("X-Trino-Routing-Group", value);
        }
    }
    headers
}

pub struct SparkEngine {
    info: EngineInfo,
}

impl SparkEngine {
    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        routing_group: Option<String>,
    ) -> Self {
        Self {
            info: EngineInfo {
                id: EngineId::new(id),
                kind: "spark".into(),
                endpoint: endpoint.into(),
                routing_group,
            },
        }
    }
}

#[async_trait]
impl QueryEngine for SparkEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    async fn health(&self) -> Health {
        Health::Unavailable
    }

    async fn execute(&self, _request: QueryRequest) -> Result<QueryResult> {
        Err(CoreError::Engine(
            "spark engine plugin not implemented yet".into(),
        ))
    }
}

pub struct StarRocksEngine {
    info: EngineInfo,
}

impl StarRocksEngine {
    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        routing_group: Option<String>,
    ) -> Self {
        Self {
            info: EngineInfo {
                id: EngineId::new(id),
                kind: "starrocks".into(),
                endpoint: endpoint.into(),
                routing_group,
            },
        }
    }
}

#[async_trait]
impl QueryEngine for StarRocksEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    async fn health(&self) -> Health {
        Health::Unavailable
    }

    async fn execute(&self, _request: QueryRequest) -> Result<QueryResult> {
        Err(CoreError::Engine(
            "starrocks engine plugin not implemented yet".into(),
        ))
    }
}

/// Canned rows for local UI work and for the Keycloak/eval stack, so the shell
/// can be exercised without a cluster. Never register it in a deployment.
pub struct MockEngine {
    info: EngineInfo,
}

impl MockEngine {
    pub fn new(
        id: impl Into<String>,
        endpoint: impl Into<String>,
        routing_group: Option<String>,
    ) -> Self {
        Self {
            info: EngineInfo {
                id: EngineId::new(id),
                kind: "mock".into(),
                endpoint: endpoint.into(),
                routing_group,
            },
        }
    }
}

#[async_trait]
impl QueryEngine for MockEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    async fn health(&self) -> Health {
        Health::Healthy
    }

    async fn execute(&self, request: QueryRequest) -> Result<QueryResult> {
        let columns = ["order_id", "customer", "region", "total", "placed_at"];
        let types = ["bigint", "varchar", "varchar", "decimal(12,2)", "timestamp"];
        let rows = [
            [
                "1",
                "Ada Lovelace",
                "eu-west",
                "120.50",
                "2026-09-17 10:00:00",
            ],
            [
                "2",
                "Grace Hopper",
                "us-east",
                "88.00",
                "2026-09-17 11:30:00",
            ],
            ["3", "Alan Turing", "eu-west", "", "2026-09-17 12:05:00"],
            [
                "4",
                "Katherine Johnson",
                "us-east",
                "340.25",
                "2026-09-17 14:10:00",
            ],
            [
                "5",
                "Barbara Liskov",
                "apac",
                "15.75",
                "2026-09-17 15:45:00",
            ],
            [
                "6",
                "Margaret Hamilton",
                "eu-west",
                "999.99",
                "2026-09-17 16:20:00",
            ],
        ];

        let columns = columns
            .iter()
            .zip(types)
            .map(|(name, data_type)| Column {
                name: (*name).into(),
                data_type: data_type.into(),
            })
            .collect();

        // Plain `SELECT 1` (the default cell of a new notebook) answers with the
        // value itself, so the first run of a fresh notebook is not nonsense.
        let sql = request.sql.trim().to_ascii_lowercase();
        if sql.starts_with("select 1") {
            return Ok(QueryResult {
                columns: vec![Column {
                    name: "result".into(),
                    data_type: "integer".into(),
                }],
                rows: vec![vec![serde_json::json!(1)]],
                truncated: false,
            });
        }

        let cap = request.max_rows.unwrap_or(1000);
        let mut result = Vec::new();
        for row in rows {
            if result.len() >= cap {
                break;
            }
            let mut values: Vec<serde_json::Value> = row
                .iter()
                .map(|value| serde_json::Value::String((*value).into()))
                .collect();
            // One null so the renderer's null handling is visible.
            if values[3].as_str() == Some("") {
                values[3] = serde_json::Value::Null;
            }
            result.push(values);
        }

        Ok(QueryResult {
            columns,
            truncated: result.len() < rows.len(),
            rows: result,
        })
    }
}

pub fn engine_from_config(config: &EngineConfig) -> Result<Arc<dyn QueryEngine>> {
    let routing_group = config.routing_group.clone();
    let engine: Arc<dyn QueryEngine> = match config.kind.as_str() {
        "trino" => Arc::new(TrinoEngine::new(
            &config.id,
            &config.endpoint,
            routing_group,
        )),
        "spark" => Arc::new(SparkEngine::new(
            &config.id,
            &config.endpoint,
            routing_group,
        )),
        "starrocks" => Arc::new(StarRocksEngine::new(
            &config.id,
            &config.endpoint,
            routing_group,
        )),
        // Demo provider: canned rows, for local UI work only.
        "mock" => Arc::new(MockEngine::new(&config.id, &config.endpoint, routing_group)),
        other => return Err(CoreError::Invalid(format!("unknown engine kind: {other}"))),
    };
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_config(kind: &str) -> EngineConfig {
        EngineConfig {
            id: "engine-1".into(),
            kind: kind.into(),
            endpoint: "http://engine.invalid:8080".into(),
            routing_group: None,
        }
    }

    #[test]
    fn an_unknown_kind_is_refused_at_startup() {
        let Err(error) = engine_from_config(&engine_config("clickhouse")) else {
            panic!("expected a refusal");
        };
        assert!(error.to_string().contains("unknown engine kind"));
    }

    #[test]
    fn every_declared_kind_resolves_to_a_provider() {
        for kind in ["trino", "spark", "starrocks", "mock"] {
            let engine = engine_from_config(&engine_config(kind)).expect(kind);
            assert_eq!(engine.info().kind, kind);
        }
    }

    #[tokio::test]
    async fn the_mock_engine_returns_rows_and_honours_the_row_cap() {
        let engine = engine_from_config(&engine_config("mock")).expect("mock");
        let request = |max_rows| QueryRequest {
            sql: "select * from orders".into(),
            catalog: None,
            schema: None,
            max_rows,
        };

        let full = engine.execute(request(None)).await.expect("rows");
        assert_eq!(full.columns.len(), 5);
        assert_eq!(full.rows.len(), 6);
        assert_eq!(full.rows[2][3], serde_json::Value::Null);
        assert!(!full.truncated);

        let capped = engine.execute(request(Some(2))).await.expect("rows");
        assert_eq!(capped.rows.len(), 2);
        assert!(capped.truncated);

        let scalar = engine
            .execute(QueryRequest {
                sql: "SELECT 1".into(),
                ..request(None)
            })
            .await
            .expect("row");
        assert_eq!(scalar.columns[0].name, "result");
        assert_eq!(scalar.rows, vec![vec![serde_json::json!(1)]]);
    }

    #[test]
    fn routing_group_becomes_a_gateway_header() {
        let scoped = EngineInfo {
            id: EngineId::new("adhoc"),
            kind: "trino".into(),
            endpoint: "http://gw:8080".into(),
            routing_group: Some("adhoc".into()),
        };
        assert_eq!(routing_headers(&scoped)["x-trino-routing-group"], "adhoc");

        let plain = EngineInfo {
            routing_group: None,
            ..scoped.clone()
        };
        assert!(routing_headers(&plain).is_empty());
    }
}
