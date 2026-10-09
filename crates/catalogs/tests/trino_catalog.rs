use aster_catalogs::catalog_from_config;
use aster_core::{Catalog, CatalogConfig, TableRef};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    endpoint: String,
    requests: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new(pages: Vec<Value>, delay_ms: u64) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let base = endpoint.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let task = tokio::spawn(async move {
            for page in pages {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buf = [0; 4096];
                    let n = stream.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                        let length = headers
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|s| s.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if body.len() >= length {
                            break;
                        }
                    }
                }
                captured
                    .lock()
                    .unwrap()
                    .push(String::from_utf8(request).unwrap());
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                let body = page.to_string().replace("$BASE", &base);
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        Self {
            endpoint,
            requests,
            task,
        }
    }
    fn catalog(&self, alias: &str) -> Arc<dyn Catalog> {
        let result = catalog_from_config(&CatalogConfig {
            id: "browser-id".into(),
            kind: "trino".into(),
            endpoint: self.endpoint.clone(),
            catalog: Some(alias.into()),
            token: Some("fixture-token".into()),
            credential: None,
        });
        assert!(
            result.is_ok(),
            "native Trino catalog must register: {:?}",
            result.err()
        );
        result.unwrap()
    }
}
fn page(names: &[&str], rows: Value) -> Value {
    json!({"id":"q1", "stats":{"state":"FINISHED"}, "columns":names.iter().map(|n| json!({"name":n,"type":"varchar"})).collect::<Vec<_>>(), "data":rows})
}

#[tokio::test]
async fn paginated_native_metadata_uses_exact_alias_and_identity() {
    for alias in ["tpch", "tpcds"] {
        let f = Fixture::new(
            vec![
                json!({"id":"q1","stats":{"state":"QUEUED"},"nextUri":"$BASE/next"}),
                page(&["schema_name"], json!([["tiny"], ["sf1"]])),
                page(&["table_name"], json!([["orders"], ["lineitem"]])),
                page(
                    &["column_name", "data_type", "is_nullable"],
                    json!([
                        ["orderkey", "bigint", "NO"],
                        ["comment", "varchar(79)", "YES"]
                    ]),
                ),
            ],
            0,
        )
        .await;
        let catalog = f.catalog(alias);
        assert_eq!(catalog.id().0, "browser-id");
        assert!(catalog.authoritative_inventory());
        assert_eq!(
            catalog.metadata_read_bytes(),
            None,
            "AI bounded schema remains unavailable"
        );
        let namespaces = catalog.list_namespaces().await.unwrap();
        assert_eq!(namespaces[0].segments, ["tiny"]);
        let tables = catalog
            .list_descriptors_qualified(&["tiny".into()])
            .await
            .unwrap();
        assert_eq!(tables.len(), 2);
        assert!(tables[0].schema_available);
        assert_eq!(tables[0].table.namespace_segments, ["tiny"]);
        let schema = catalog.table_schema(&tables[0].table).await.unwrap();
        assert_eq!(schema.columns[0].data_type, "bigint");
        assert!(!schema.columns[0].nullable);
        assert!(schema.columns[1].nullable);
        let requests = f.requests.lock().unwrap();
        assert!(requests[0].contains(&format!("FROM \"{alias}\".information_schema.schemata")));
        assert!(requests[1].starts_with("GET /next "));
        assert!(requests[2].contains("table_schema = 'tiny'"));
        assert!(requests[3].contains("table_name = 'orders' ORDER BY ordinal_position"));
        assert!(requests
            .iter()
            .all(|r| r.contains("authorization: Bearer fixture-token")
                && r.contains("x-trino-user: aster")));
    }
}

#[tokio::test]
async fn quotes_hostile_identifiers_and_literals_without_splitting_namespaces() {
    let f = Fixture::new(
        vec![page(
            &["column_name", "data_type", "is_nullable"],
            json!([["x", "bigint", "YES"]]),
        )],
        0,
    )
    .await;
    let catalog = f.catalog("tp\"ch");
    let table = TableRef {
        namespace: "tiny.' OR 1=1 --".into(),
        namespace_segments: vec!["tiny.' OR 1=1 --".into()],
        name: "x'; DROP TABLE z; --".into(),
    };
    catalog.table_schema(&table).await.unwrap();
    let requests = f.requests.lock().unwrap();
    assert!(requests[0].contains("FROM \"tp\"\"ch\".information_schema.columns"));
    assert!(requests[0].contains("table_schema = 'tiny.'' OR 1=1 --'"));
    assert!(requests[0].contains("table_name = 'x''; DROP TABLE z; --'"));
}

#[tokio::test]
async fn refuses_malformed_errors_incomplete_and_duplicate_metadata() {
    for value in [
        json!({}),
        json!({"error":{"message":"fixture-secret"}}),
        json!({"id":"q1","stats":{"state":"RUNNING"}}),
        page(&["wrong"], json!([["tiny"]])),
        page(&["schema_name"], json!([[null]])),
        page(&["schema_name"], json!([["tiny", "extra"]])),
        page(&["schema_name"], json!([["tiny"], ["tiny"]])),
        json!({"id":"q1","stats":{"state":"FINISHED"},"nextUri":false}),
    ] {
        let f = Fixture::new(vec![value], 0).await;
        let error = f.catalog("tpch").list_namespaces().await.unwrap_err();
        assert!(!error.to_string().contains("fixture-secret"));
    }
}

#[tokio::test]
async fn refuses_off_origin_userinfo_fragment_and_repeated_continuations() {
    for next in [
        "http://127.0.0.1:1/escape",
        "https://example.org/escape",
        "http://user:pass@example.org/",
        "$BASE/next#fragment",
        "/relative",
    ] {
        let f = Fixture::new(
            vec![json!({"id":"q1","stats":{"state":"RUNNING"},"nextUri":next})],
            0,
        )
        .await;
        assert!(f.catalog("tpch").list_namespaces().await.is_err());
        assert_eq!(f.requests.lock().unwrap().len(), 1);
    }
    let p = json!({"id":"q1","stats":{"state":"RUNNING"},"nextUri":"$BASE/next"});
    let f = Fixture::new(vec![p.clone(), p], 0).await;
    assert!(f.catalog("tpch").list_namespaces().await.is_err());
    assert_eq!(f.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn bounds_pages_rows_and_total_bytes() {
    let pages = (0..65)
        .map(|n| json!({"id":"q1","stats":{"state":"RUNNING"},"nextUri":format!("$BASE/next/{n}")}))
        .collect();
    let f = Fixture::new(pages, 0).await;
    assert!(f
        .catalog("tpch")
        .list_namespaces()
        .await
        .unwrap_err()
        .to_string()
        .contains("limit"));
    assert_eq!(f.requests.lock().unwrap().len(), 64);
    let rows: Vec<_> = (0..10001).map(|n| json!([format!("s{n}")])).collect();
    let f = Fixture::new(vec![page(&["schema_name"], json!(rows))], 0).await;
    assert!(f
        .catalog("tpch")
        .list_namespaces()
        .await
        .unwrap_err()
        .to_string()
        .contains("limit"));
    let mut first = page(&["schema_name"], json!([["tiny"]]));
    first["padding"] = json!("x".repeat(3 * 1024 * 1024));
    first["nextUri"] = json!("$BASE/next");
    first["stats"]["state"] = json!("RUNNING");
    let mut second = page(&["schema_name"], json!([["sf1"]]));
    second["padding"] = json!("x".repeat(2 * 1024 * 1024));
    let f = Fixture::new(vec![first, second], 0).await;
    assert!(f
        .catalog("tpch")
        .list_namespaces()
        .await
        .unwrap_err()
        .to_string()
        .contains("byte limit"));
}

#[tokio::test]
async fn pages_share_one_deadline() {
    let f = Fixture::new(
        vec![
            json!({"id":"q1","stats":{"state":"RUNNING"},"nextUri":"$BASE/next"}),
            page(&["schema_name"], json!([["tiny"]])),
        ],
        6000,
    )
    .await;
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        f.catalog("tpch").list_namespaces(),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    assert!(start.elapsed() < std::time::Duration::from_millis(11500));
    assert_eq!(f.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn refuses_schema_identity_conflicts_missing_tables_and_invalid_nullability() {
    for rows in [json!([]), json!([["x", "bigint", "MAYBE"]])] {
        let f = Fixture::new(
            vec![page(&["column_name", "data_type", "is_nullable"], rows)],
            0,
        )
        .await;
        let catalog = f.catalog("tpch");
        let mut table = TableRef {
            namespace: "tiny".into(),
            namespace_segments: vec!["other".into()],
            name: "orders".into(),
        };
        assert!(catalog.table_schema(&table).await.is_err());
        assert!(catalog
            .list_tables_qualified(&["tiny".into(), "nested".into()])
            .await
            .is_err());
        assert!(f.requests.lock().unwrap().is_empty());
        table.namespace_segments = vec!["tiny".into()];
        assert!(catalog.table_schema(&table).await.is_err());
    }
}

#[test]
fn registry_requires_explicit_alias_and_valid_endpoint_and_auth() {
    let config = CatalogConfig {
        id: "tpch".into(),
        kind: "trino".into(),
        endpoint: "http://localhost:8080".into(),
        catalog: Some("tpch".into()),
        token: None,
        credential: None,
    };
    assert!(catalog_from_config(&config).is_ok());
    for alias in [None, Some("".into()), Some("bad\nname".into())] {
        assert!(catalog_from_config(&CatalogConfig {
            catalog: alias,
            ..config.clone()
        })
        .is_err());
    }
    for endpoint in [
        "file:///tmp/trino",
        "http://user:secret@localhost",
        "http://localhost/?query=1",
        "http://localhost/#fragment",
    ] {
        assert!(catalog_from_config(&CatalogConfig {
            endpoint: endpoint.into(),
            ..config.clone()
        })
        .is_err());
    }
    assert!(catalog_from_config(&CatalogConfig {
        credential: Some("user:secret".into()),
        ..config.clone()
    })
    .is_err());
    assert!(catalog_from_config(&CatalogConfig {
        token: Some("bad\ntoken".into()),
        ..config
    })
    .is_err());
}

#[tokio::test]
async fn finished_execution_still_drains_remaining_result_pages() {
    let mut first = page(&["schema_name"], json!([["tiny"]]));
    first["nextUri"] = json!("$BASE/next");
    let f = Fixture::new(vec![first, page(&["schema_name"], json!([["sf1"]]))], 0).await;
    let namespaces = f.catalog("tpch").list_namespaces().await.unwrap();
    assert_eq!(namespaces.len(), 2);
}

#[tokio::test]
async fn pending_resource_and_dispatch_states_continue_to_complete_metadata() {
    let f = Fixture::new(
        vec![
            json!({"id":"q1","stats":{"state":"WAITING_FOR_RESOURCES"},"nextUri":"$BASE/wait"}),
            json!({"id":"q1","stats":{"state":"DISPATCHING"},"nextUri":"$BASE/dispatch"}),
            page(&["schema_name"], json!([["tiny"]])),
        ],
        0,
    )
    .await;
    assert_eq!(f.catalog("tpch").list_namespaces().await.unwrap().len(), 1);
}
