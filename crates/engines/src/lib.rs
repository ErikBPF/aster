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
        let base = self.info.endpoint.trim_end_matches('/');
        let max_rows = request.max_rows.unwrap_or(1000);
        let headers = statement_headers(&self.info, &request);

        let mut payload: serde_json::Value = self
            .client
            .post(format!("{base}/v1/statement"))
            .headers(headers.clone())
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
                        .get(engine_endpoint(&self.info, next))
                        .headers(headers.clone())
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

        // A crawled page advertised more work; tell the coordinator to stop
        // instead of leaving the query running after we stop reading it.
        if truncated {
            if let Some(next) = payload.get("nextUri").and_then(|value| value.as_str()) {
                let _ = self
                    .client
                    .delete(engine_endpoint(&self.info, next))
                    .headers(headers)
                    .send()
                    .await;
            }
        }

        Ok(QueryResult {
            columns,
            rows,
            truncated,
        })
    }
}

/// Trino Gateway picks the backend pool from the routing-group header; without
/// it the gateway uses its own default. Catalog and schema are only sent when a
/// cell asked for them, so the coordinator keeps its own defaults otherwise.
fn statement_headers(info: &EngineInfo, request: &QueryRequest) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in [
        ("X-Trino-User", Some("aster".to_string())),
        ("X-Trino-Source", Some("aster".to_string())),
        ("X-Trino-Client-Tags", Some("aster".to_string())),
        ("X-Trino-Routing-Group", info.routing_group.clone()),
        ("X-Trino-Catalog", request.catalog.clone()),
        ("X-Trino-Schema", request.schema.clone()),
    ] {
        if let Some(value) = value {
            if let Ok(value) = reqwest::header::HeaderValue::from_str(&value) {
                headers.insert(name, value);
            }
        }
    }
    headers
}

/// Follow-up pages come back as absolute URIs on the coordinator's own address,
/// which is not reachable from (and would bypass) a gateway in front of it. Send
/// them back to the endpoint the engine was configured with.
fn engine_endpoint(info: &EngineInfo, advertised: &str) -> String {
    let base = info.endpoint.trim_end_matches('/');
    match reqwest::Url::parse(advertised) {
        Ok(advertised) => match reqwest::Url::parse(&format!("{base}/")) {
            Ok(mut url) => {
                url.set_path(advertised.path());
                url.set_query(advertised.query());
                url.to_string()
            }
            Err(_) => advertised.to_string(),
        },
        Err(_) => {
            let path = if advertised.starts_with('/') {
                advertised.to_string()
            } else {
                format!("/{advertised}")
            };
            format!("{base}{path}")
        }
    }
}

pub struct SparkEngine {
    info: EngineInfo,
    session: tokio::sync::OnceCell<spark_connect::SparkSession>,
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
            session: tokio::sync::OnceCell::new(),
        }
    }

    /// The client connects once per engine and reuses the session; the connect
    /// itself is blocking, so it runs on the blocking pool.
    async fn session(&self) -> Result<&spark_connect::SparkSession> {
        self.session
            .get_or_try_init(|| async {
                let endpoint = connect_url(&self.info.endpoint);
                let session = tokio::task::spawn_blocking(move || {
                    spark_connect::SparkSession::builder()
                        .remote(&endpoint)
                        .get_or_create()
                        .map_err(spark_error)
                })
                .await
                .map_err(|error| CoreError::Engine(error.to_string()))?;
                session
            })
            .await
    }
}

#[async_trait]
impl QueryEngine for SparkEngine {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    async fn health(&self) -> Health {
        match self.session().await {
            Ok(_) => Health::Healthy,
            Err(_) => Health::Unavailable,
        }
    }

    async fn execute(&self, request: QueryRequest) -> Result<QueryResult> {
        if let Some(catalog) = request.catalog.as_deref() {
            if !catalog.is_empty() {
                return Err(CoreError::Invalid(
                    "the spark engine has no catalog switching yet".into(),
                ));
            }
        }
        let session = self.session().await?.clone();
        let sql = request.sql;
        let max_rows = request.max_rows.unwrap_or(1000);
        tokio::task::spawn_blocking(move || run_spark_query(&session, &sql, max_rows))
            .await
            .map_err(|error| CoreError::Engine(error.to_string()))?
    }
}

fn run_spark_query(
    session: &spark_connect::SparkSession,
    sql: &str,
    max_rows: usize,
) -> Result<QueryResult> {
    let frame = session.sql(sql).map_err(spark_error)?;
    let schema = frame.schema().map_err(spark_error)?;
    let columns = match schema {
        spark_connect::types::DataType::Struct { fields } => fields
            .into_iter()
            .map(|field| Column {
                name: field.name,
                data_type: field.data_type.simple_string(),
            })
            .collect(),
        other => {
            return Err(CoreError::Engine(format!(
                "spark returned a non-tabular schema: {other}"
            )))
        }
    };

    // Ask for one row more than the cap so truncation is observable.
    let cap = i32::try_from(max_rows).unwrap_or(i32::MAX - 1);
    let rows = frame
        .limit(cap.saturating_add(1))
        .collect()
        .map_err(spark_error)?;
    let truncated = rows.len() > max_rows;
    let rows = rows
        .into_iter()
        .take(max_rows)
        .map(|row| row.into_values().iter().map(spark_value).collect())
        .collect();

    Ok(QueryResult {
        columns,
        rows,
        truncated,
    })
}

/// Spark values carry their own type; JSON is the wire shape the API already
/// speaks, so the wider types keep their text form rather than losing precision.
fn spark_value(value: &spark_connect::row::Value) -> serde_json::Value {
    use spark_connect::row::Value;
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(value) => (*value).into(),
        Value::Byte(value) => (*value).into(),
        Value::Short(value) => (*value).into(),
        Value::Integer(value) => (*value).into(),
        Value::Long(value) => (*value).into(),
        Value::Float(value) => serde_json::Number::from_f64(f64::from(*value))
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        Value::Double(value) => serde_json::Number::from_f64(*value)
            .map_or(serde_json::Value::Null, serde_json::Value::Number),
        Value::String(value) => value.clone().into(),
        Value::Binary(value) => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .encode(value)
                .into()
        }
        Value::Date(days) => format!("{days} days since epoch").into(),
        Value::Timestamp(micros) => format!("{micros} microseconds since epoch").into(),
        Value::Decimal { value, .. } => value.clone().into(),
        Value::List(values) => values.iter().map(spark_value).collect(),
        Value::Map(entries) => entries
            .iter()
            .map(|(key, value)| (key.clone(), spark_value(value)))
            .collect(),
        Value::Struct(fields) => fields
            .iter()
            .map(|(name, value)| (name.clone(), spark_value(value)))
            .collect(),
        Value::Variant { value, .. } => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD
                .encode(value)
                .into()
        }
    }
}

fn spark_error(error: spark_connect::SparkError) -> CoreError {
    CoreError::Engine(error.to_string())
}

/// The Spark client speaks its own `sc://host[:port][/;k=v]` connection string,
/// while engine endpoints stay in the http(s) form every other engine uses.
fn connect_url(endpoint: &str) -> String {
    let (secure, authority) = match endpoint.split_once("://") {
        Some(("sc", _)) => return endpoint.to_string(),
        Some(("https", rest)) => (true, rest),
        Some((_, rest)) => (false, rest),
        None => (false, endpoint),
    };
    let authority = authority.trim_end_matches('/');
    if secure {
        format!("sc://{authority}/;use_ssl=true")
    } else {
        format!("sc://{authority}")
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
        let plain_request = QueryRequest {
            sql: "select 1".into(),
            catalog: None,
            schema: None,
            max_rows: None,
        };
        assert_eq!(
            statement_headers(&scoped, &plain_request)["x-trino-routing-group"],
            "adhoc"
        );

        let plain = EngineInfo {
            routing_group: None,
            ..scoped.clone()
        };
        let headers = statement_headers(&plain, &plain_request);
        assert!(headers.get("x-trino-routing-group").is_none());
        assert_eq!(headers["x-trino-user"], "aster");
        assert_eq!(headers["x-trino-source"], "aster");
    }

    #[test]
    fn catalog_and_schema_are_sent_only_when_asked_for() {
        let info = EngineInfo {
            id: EngineId::new("engine-1"),
            kind: "trino".into(),
            endpoint: "http://engine.invalid:8080".into(),
            routing_group: None,
        };
        let scoped = QueryRequest {
            sql: "select 1".into(),
            catalog: Some("tpch".into()),
            schema: Some("tiny".into()),
            max_rows: None,
        };
        let headers = statement_headers(&info, &scoped);
        assert_eq!(headers["x-trino-catalog"], "tpch");
        assert_eq!(headers["x-trino-schema"], "tiny");

        let unscoped = QueryRequest {
            catalog: None,
            schema: None,
            ..scoped
        };
        let headers = statement_headers(&info, &unscoped);
        assert!(headers.get("x-trino-catalog").is_none());
        assert!(headers.get("x-trino-schema").is_none());
    }

    #[test]
    fn spark_values_keep_their_type_on_the_wire() {
        use spark_connect::row::Value;

        assert_eq!(spark_value(&Value::Null), serde_json::Value::Null);
        assert_eq!(spark_value(&Value::Bool(true)), serde_json::json!(true));
        assert_eq!(spark_value(&Value::Long(42)), serde_json::json!(42));
        assert_eq!(spark_value(&Value::Double(1.5)), serde_json::json!(1.5));
        assert_eq!(spark_value(&Value::String("ada".into())), "ada");
        assert_eq!(
            spark_value(&Value::Decimal {
                value: "12.30".into(),
                precision: Some(12),
                scale: Some(2),
            }),
            "12.30"
        );
        assert_eq!(
            spark_value(&Value::Binary(vec![1, 2, 3])),
            // Standard base64, as Spark's own JSON encoding uses.
            "AQID"
        );
        assert_eq!(
            spark_value(&Value::List(vec![Value::Long(1), Value::Null])),
            serde_json::json!([1, null])
        );
        assert_eq!(
            spark_value(&Value::Struct(vec![("n".into(), Value::Long(2))])),
            serde_json::json!({"n": 2})
        );
    }

    #[test]
    fn spark_endpoints_become_connect_connection_strings() {
        assert_eq!(
            connect_url("http://spark-connect-aster:15002"),
            "sc://spark-connect-aster:15002"
        );
        assert_eq!(
            connect_url("https://spark.example.com:443/"),
            "sc://spark.example.com:443/;use_ssl=true"
        );
        assert_eq!(connect_url("127.0.0.1:15002"), "sc://127.0.0.1:15002");
        assert_eq!(connect_url("sc://host:15002"), "sc://host:15002");
    }

    #[tokio::test]
    async fn a_spark_catalog_request_is_refused_before_connecting() {
        let engine = SparkEngine::new("spark-live", "http://spark.invalid:15002", None);
        let Err(error) = engine
            .execute(QueryRequest {
                sql: "select 1".into(),
                catalog: Some("hive".into()),
                schema: None,
                max_rows: None,
            })
            .await
        else {
            panic!("expected a refusal");
        };
        assert!(error.to_string().contains("catalog"));
    }

    #[test]
    fn a_foreign_next_uri_is_rerouted_to_the_configured_endpoint() {
        let gateway = EngineInfo {
            id: EngineId::new("engine-1"),
            kind: "trino".into(),
            endpoint: "http://gateway.invalid:8080/".into(),
            routing_group: Some("lab".into()),
        };
        assert_eq!(
            engine_endpoint(
                &gateway,
                "http://trino.trino.svc:8080/v1/statement/queued/2026_1/abc/1?slug=x",
            ),
            "http://gateway.invalid:8080/v1/statement/queued/2026_1/abc/1?slug=x"
        );
        assert_eq!(
            engine_endpoint(&gateway, "/v1/statement/queued/2026_1/abc/1"),
            "http://gateway.invalid:8080/v1/statement/queued/2026_1/abc/1"
        );
    }
}
