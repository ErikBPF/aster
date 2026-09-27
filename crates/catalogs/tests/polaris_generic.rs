use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use aster_catalogs::PolarisCatalog;
use aster_core::{Catalog, TableRef};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

struct Fixture {
    endpoint: String,
    requests: Arc<Mutex<Vec<String>>>,
    routes: Arc<Mutex<HashMap<String, (u16, String)>>>,
    task: JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fixture {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listen");
        let endpoint = format!("http://{}", listener.local_addr().expect("address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let routes: Arc<Mutex<HashMap<String, (u16, String)>>> = Arc::new(Mutex::new(HashMap::from([
            ("/api/catalog/v1/lake/namespaces/sales/tables".into(), (200, r#"{"identifiers":[{"namespace":["sales"],"name":"iceberg_orders"}]}"#.into())),
            ("/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables".into(), (200, r#"{"identifiers":[{"namespace":["sales"],"name":"delta_orders"}],"next-page-token":null}"#.into())),
            ("/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables/delta_orders".into(), (200, r#"{"table":{"name":"delta_orders","format":"delta","base-location":"s3://lake/sales/delta_orders"}}"#.into())),
        ])));
        let served = Arc::clone(&routes);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let Ok(read) = stream.read(&mut chunk).await else {
                        break;
                    };
                    if read == 0 || request.len() > 8192 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..read]);
                }
                let request = String::from_utf8_lossy(&request).into_owned();
                captured.lock().expect("request log").push(request.clone());
                let path = request.split_whitespace().nth(1).unwrap_or("");
                let (status, body) = served
                    .lock()
                    .expect("routes")
                    .get(path)
                    .cloned()
                    .unwrap_or((404, r#"{"error":"not found"}"#.into()));
                let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        Self {
            endpoint,
            requests,
            routes,
            task,
        }
    }

    fn respond(&self, path: &str, status: u16, body: &str) {
        self.routes
            .lock()
            .expect("routes")
            .insert(path.into(), (status, body.into()));
    }
}

#[tokio::test]
async fn polaris_generic_lists_delta_beside_iceberg() {
    let fixture = Fixture::start().await;
    let catalog = PolarisCatalog::new("polaris", &fixture.endpoint, "lake")
        .with_token(Some("fixture-token".into()))
        .with_generic_tables(true);

    let tables = catalog.list_tables("sales").await.unwrap_or_else(|error| {
        panic!(
            "{error}; requests: {:?}",
            fixture.requests.lock().expect("requests")
        )
    });
    assert_eq!(
        tables.len(),
        2,
        "generic tables must be included in the browse"
    );
    assert!(tables.iter().any(|table| table.name == "delta_orders"));
    assert!(fixture
        .requests
        .lock()
        .expect("requests")
        .iter()
        .any(|request| {
            request.starts_with("GET /api/catalog/polaris/v1/lake/namespaces/sales/generic-tables ")
                && request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer fixture-token")
        }));
}

#[tokio::test]
async fn polaris_generic_paginates_loads_and_marks_schema_unavailable() {
    let fixture = Fixture::start().await;
    fixture.respond(
        "/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables",
        200,
        r#"{"identifiers":[{"namespace":["sales"],"name":"delta_orders"}],"next-page-token":"second"}"#,
    );
    fixture.respond(
        "/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables?pageToken=second",
        200,
        r#"{"identifiers":[{"namespace":["sales"],"name":"csv_exports"}],"next-page-token":null}"#,
    );
    fixture.respond(
        "/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables/csv_exports",
        200,
        r#"{"table":{"name":"csv_exports","format":"csv"}}"#,
    );
    let catalog = PolarisCatalog::new("polaris", &fixture.endpoint, "lake")
        .with_token(Some("fixture-token".into()))
        .with_generic_tables(true);

    let descriptors = catalog
        .list_table_descriptors("sales")
        .await
        .expect("browse");
    assert_eq!(descriptors.len(), 3);
    assert_eq!(descriptors[0].format.as_deref(), Some("iceberg"));
    assert!(descriptors[0].schema_available);
    let delta = descriptors
        .iter()
        .find(|item| item.table.name == "delta_orders")
        .unwrap();
    assert_eq!(delta.format.as_deref(), Some("delta"));
    assert_eq!(
        delta.base_location.as_deref(),
        Some("s3://lake/sales/delta_orders")
    );
    assert!(!delta.schema_available);
    let csv = descriptors
        .iter()
        .find(|item| item.table.name == "csv_exports")
        .unwrap();
    assert_eq!(csv.format.as_deref(), Some("csv"));
    assert_eq!(
        csv.base_location, None,
        "Polaris allows an absent base location"
    );
    assert!(!csv.schema_available);

    let refusal = catalog
        .table_schema(&TableRef {
            namespace: "sales".into(),
            name: "delta_orders".into(),
        })
        .await
        .expect_err("generic schema must be unavailable");
    assert!(refusal.to_string().contains("schema unavailable"));
    let requests = fixture.requests.lock().expect("requests");
    assert!(requests.iter().any(|request| request.starts_with(
        "GET /api/catalog/polaris/v1/lake/namespaces/sales/generic-tables?pageToken=second "
    ) && request
        .to_ascii_lowercase()
        .contains("authorization: bearer fixture-token")));
    assert!(
        !requests
            .iter()
            .any(|request| request.contains("/tables/delta_orders ")),
        "Delta must never be loaded through the Iceberg table endpoint"
    );
}

#[tokio::test]
async fn polaris_generic_denied_list_and_load_are_errors() {
    let list_denied = Fixture::start().await;
    list_denied.respond(
        "/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables",
        403,
        r#"{"message":"denied"}"#,
    );
    let catalog =
        PolarisCatalog::new("polaris", &list_denied.endpoint, "lake").with_generic_tables(true);
    let error = catalog
        .list_table_descriptors("sales")
        .await
        .expect_err("denied list");
    assert!(error.to_string().contains("HTTP 403"), "{error}");

    let load_missing = Fixture::start().await;
    load_missing.respond(
        "/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables/delta_orders",
        404,
        r#"{"message":"missing"}"#,
    );
    let catalog =
        PolarisCatalog::new("polaris", &load_missing.endpoint, "lake").with_generic_tables(true);
    let error = catalog
        .list_table_descriptors("sales")
        .await
        .expect_err("missing load");
    assert!(error.to_string().contains("HTTP 404"), "{error}");
}

#[tokio::test]
async fn polaris_generic_malformed_location_and_duplicate_name_are_errors() {
    let malformed = Fixture::start().await;
    malformed.respond(
        "/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables/delta_orders",
        200,
        r#"{"table":{"name":"delta_orders","format":"delta","base-location":"not a URI"}}"#,
    );
    let catalog =
        PolarisCatalog::new("polaris", &malformed.endpoint, "lake").with_generic_tables(true);
    let error = catalog
        .list_table_descriptors("sales")
        .await
        .expect_err("malformed location");
    assert!(
        error.to_string().contains("malformed base location"),
        "{error}"
    );

    let duplicate = Fixture::start().await;
    duplicate.respond(
        "/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables",
        200,
        r#"{"identifiers":[{"namespace":["sales"],"name":"iceberg_orders"}],"next-page-token":null}"#,
    );
    let catalog =
        PolarisCatalog::new("polaris", &duplicate.endpoint, "lake").with_generic_tables(true);
    let error = catalog
        .list_table_descriptors("sales")
        .await
        .expect_err("duplicate name");
    assert!(error.to_string().contains("same name"), "{error}");
}

#[tokio::test]
async fn polaris_generic_is_runtime_disabled_by_default() {
    let fixture = Fixture::start().await;
    let catalog = PolarisCatalog::new("polaris", &fixture.endpoint, "lake");
    let descriptors = catalog
        .list_table_descriptors("sales")
        .await
        .expect("Iceberg browse");
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].format.as_deref(), Some("iceberg"));
    assert!(!fixture
        .requests
        .lock()
        .expect("requests")
        .iter()
        .any(|request| { request.contains("/generic-tables") }));
}

#[tokio::test]
async fn polaris_generic_rejects_credential_bearing_location() {
    for location in [
        "s3://user:secret@lake/sales/delta_orders",
        "s3://lake/sales/delta_orders?token=secret",
        "s3://lake/sales/delta_orders#secret",
    ] {
        let fixture = Fixture::start().await;
        fixture.respond(
            "/api/catalog/polaris/v1/lake/namespaces/sales/generic-tables/delta_orders",
            200,
            &format!(r#"{{"table":{{"name":"delta_orders","format":"delta","base-location":"{location}"}}}}"#),
        );
        let catalog =
            PolarisCatalog::new("polaris", &fixture.endpoint, "lake").with_generic_tables(true);
        let error = catalog
            .list_table_descriptors("sales")
            .await
            .expect_err("unsafe location");
        assert!(
            error.to_string().contains("malformed base location"),
            "{error}"
        );
        assert!(
            !error.to_string().contains("secret"),
            "no location credentials in error"
        );
    }
}
