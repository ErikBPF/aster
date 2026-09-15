//! Query-engine plugins. Each module adapts one engine's wire protocol to the
//! `QueryEngine` trait. Registering a new engine is one module plus one arm in
//! `engine_from_config`.

use std::sync::Arc;

use aster_core::{
    Column, CoreError, EngineConfig, EngineHealth, EngineId, EngineInfo, QueryEngine, QueryRequest,
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

    async fn health(&self) -> EngineHealth {
        let url = format!("{}/v1/info", self.info.endpoint.trim_end_matches('/'));
        match self.client.get(url).send().await {
            Ok(response) if response.status().is_success() => EngineHealth::Healthy,
            Ok(_) => EngineHealth::Degraded,
            Err(_) => EngineHealth::Unavailable,
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

    async fn health(&self) -> EngineHealth {
        EngineHealth::Unavailable
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

    async fn health(&self) -> EngineHealth {
        EngineHealth::Unavailable
    }

    async fn execute(&self, _request: QueryRequest) -> Result<QueryResult> {
        Err(CoreError::Engine(
            "starrocks engine plugin not implemented yet".into(),
        ))
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
        other => return Err(CoreError::Invalid(format!("unknown engine kind: {other}"))),
    };
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

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
