use aster_catalogs::{catalog_from_config, CubeCatalog, OpenMetadataCatalog, PolarisCatalog};
use aster_core::{Catalog, CatalogConfig, TableRef};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{
    rustls::{
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
        ServerConfig,
    },
    TlsAcceptor,
};

type Reply = (u16, String, String, u64);

pub async fn polaris_health_shares_oauth_deadline() {
    use aster_core::Health;
    use std::time::Instant;
    let f = Fixture::start(true).await;
    let oauth_path = "/api/catalog/v1/oauth/tokens";
    let health_path = "/api/catalog/v1/lake/namespaces";
    let catalog = PolarisCatalog::new("p", &f.endpoint, "lake")
        .with_root_certificate(f.certificate.clone())
        .with_credential(Some("fixture:health-secret".into()));
    f.raw(
        oauth_path,
        200,
        json!({"access_token":"fixture-health-token"}).to_string(),
        "",
        150,
    );
    // Health is status-only: no JSON shape or body consumption is required.
    f.raw(health_path, 200, "not JSON".into(), "", 150);
    let start = Instant::now();
    assert_eq!(catalog.health().await, Health::Healthy);
    assert!(start.elapsed() >= Duration::from_millis(300));
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(f.requests.lock().unwrap().len(), 2);
    f.respond(health_path, 503, json!({"error":"fixture-health-token"}));
    assert_eq!(catalog.health().await, Health::Degraded);
    f.respond(
        oauth_path,
        401,
        json!({"access_token":"fixture-health-token"}),
    );
    let before = f.requests.lock().unwrap().len();
    assert_eq!(catalog.health().await, Health::Unavailable);
    assert_eq!(f.requests.lock().unwrap().len(), before + 1);

    // Each response individually fits ten seconds; their combined latency does not.
    f.raw(
        oauth_path,
        200,
        json!({"access_token":"fixture-health-token"}).to_string(),
        "",
        6000,
    );
    f.raw(health_path, 200, "not JSON".into(), "", 6000);
    let before = f.requests.lock().unwrap().len();
    let start = Instant::now();
    let health = tokio::time::timeout(Duration::from_secs(15), catalog.health())
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(f.requests.lock().unwrap().len(), before + 2);
    assert_eq!(
        health,
        Health::Unavailable,
        "OAuth plus health must share one deadline; elapsed {elapsed:?}"
    );
    assert!(
        elapsed >= Duration::from_secs(9) && elapsed < Duration::from_millis(11500),
        "shared deadline elapsed {elapsed:?}"
    );
}

pub async fn polaris_pages_share_bytes_and_elapsed_budget() {
    use std::time::Instant;
    let f = Fixture::start(true).await;
    let path = "/api/catalog/v1/lake/namespaces";
    let second = format!("{path}?pageToken=next");
    let page = |name: &str, padding: usize, next: Value| {
        json!({"namespaces":[[name]], "padding":"x".repeat(padding), "next-page-token":next})
            .to_string()
    };
    f.raw(
        path,
        200,
        page("first", 1024 * 1024, json!("next")),
        "",
        150,
    );
    f.raw(
        &second,
        200,
        page("second", 1024 * 1024, Value::Null),
        "",
        150,
    );
    let start = Instant::now();
    let namespaces = f.polaris().list_namespaces().await.unwrap();
    assert_eq!(
        namespaces
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert!(start.elapsed() >= Duration::from_millis(300));
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(f.requests.lock().unwrap().len(), 2);

    // Each body fits individually; the combined six MiB must not pass a four MiB budget.
    f.raw(
        path,
        200,
        page("first", 3 * 1024 * 1024, json!("next")),
        "",
        0,
    );
    f.raw(
        &second,
        200,
        page("second", 3 * 1024 * 1024, Value::Null),
        "",
        0,
    );
    let error = f.polaris().list_namespaces().await.unwrap_err().to_string();
    assert!(error.contains("byte limit"));
    assert_eq!(f.requests.lock().unwrap().len(), 4);
}
pub struct Fixture {
    pub endpoint: String,
    pub certificate: reqwest::Certificate,
    pub requests: Arc<Mutex<Vec<String>>>,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
    task: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    pub async fn start(tls: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let command = |args: &[&str]| {
            let out = std::process::Command::new("openssl")
                .current_dir(dir.path())
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "openssl fixture setup failed");
        };
        command(&[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=fixture CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
        ]);
        command(&[
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-keyout",
            "leaf.key",
            "-out",
            "leaf.csr",
        ]);
        std::fs::write(dir.path().join("ext"), "basicConstraints=critical,CA:FALSE\nsubjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n").unwrap();
        command(&[
            "x509",
            "-req",
            "-in",
            "leaf.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-days",
            "1",
            "-extfile",
            "ext",
            "-outform",
            "DER",
            "-out",
            "leaf.der",
        ]);
        command(&[
            "pkcs8", "-topk8", "-nocrypt", "-in", "leaf.key", "-outform", "DER", "-out", "key.der",
        ]);
        let config = ServerConfig::builder_with_provider(Arc::new(
            tokio_rustls::rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(
                std::fs::read(dir.path().join("leaf.der")).unwrap(),
            )],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                std::fs::read(dir.path().join("key.der")).unwrap(),
            )),
        )
        .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let certificate =
            reqwest::Certificate::from_pem(&std::fs::read(dir.path().join("ca.pem")).unwrap())
                .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!(
            "{}://{}",
            if tls { "https" } else { "http" },
            listener.local_addr().unwrap()
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let routes = Arc::new(Mutex::new(HashMap::<String, Reply>::new()));
        let (seen, served) = (requests.clone(), routes.clone());
        let task = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let (seen, served, acceptor) = (seen.clone(), served.clone(), acceptor.clone());
                tokio::spawn(async move {
                    if tls {
                        if let Ok(stream) = acceptor.accept(socket).await {
                            serve(stream, seen, served).await;
                        }
                    } else {
                        serve(socket, seen, served).await;
                    }
                });
            }
        });
        Self {
            endpoint,
            certificate,
            requests,
            routes,
            task,
            _dir: dir,
        }
    }
    pub fn respond(&self, path: &str, status: u16, body: Value) {
        self.raw(path, status, body.to_string(), "", 0);
    }
    pub fn raw(&self, path: &str, status: u16, body: String, extra: &str, delay: u64) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.into(), (status, body, extra.into(), delay));
    }
    fn polaris(&self) -> PolarisCatalog {
        PolarisCatalog::new("p", &self.endpoint, "lake")
            .with_root_certificate(self.certificate.clone())
            .with_token(Some("FIXTURE_TOKEN".into()))
    }
    fn om(&self) -> OpenMetadataCatalog {
        OpenMetadataCatalog::new("om", &self.endpoint, None, Some("FIXTURE_TOKEN".into()))
            .with_root_certificate(self.certificate.clone())
    }
}
async fn serve<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    mut stream: S,
    seen: Arc<Mutex<Vec<String>>>,
    routes: Arc<Mutex<HashMap<String, Reply>>>,
) {
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
        let Ok(n) = stream.read(&mut buf).await else {
            return;
        };
        if n == 0 || bytes.len() > 16384 {
            return;
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    let request = String::from_utf8(bytes).unwrap();
    let path = request.split_whitespace().nth(1).unwrap();
    let reply =
        routes
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or((404, "{}".into(), String::new(), 0));
    seen.lock().unwrap().push(request);
    tokio::time::sleep(Duration::from_millis(reply.3)).await;
    let (length, body) = if reply.2.contains("Transfer-Encoding: chunked") {
        (
            String::new(),
            format!("{:x}\r\n{}\r\n0\r\n\r\n", reply.1.len(), reply.1),
        )
    } else {
        (format!("Content-Length: {}\r\n", reply.1.len()), reply.1)
    };
    let response = format!("HTTP/1.1 {} Test\r\nContent-Type: application/json\r\n{length}Connection: close\r\n{}\r\n{body}", reply.0, reply.2);
    let _ = stream.write_all(response.as_bytes()).await;
}
fn table() -> TableRef {
    TableRef {
        namespace: "sales.eu.raw".into(),
        namespace_segments: vec!["sales.eu".into(), "raw".into()],
        name: "orders/part".into(),
    }
}
const LOAD: &str = "/api/catalog/v1/lake/namespaces/sales.eu%1Fraw/tables/orders%2Fpart";
fn schema(id: u32, name: &str) -> Value {
    json!({"schema-id":id,"type":"struct","fields":[{"id":1,"name":name,"type":"long","required":true}]})
}

pub async fn iceberg_current_schema_and_pages() {
    let f = Fixture::start(true).await;
    f.respond(LOAD, 200, json!({"metadata":{"format-version":2,"current-schema-id":7,"schemas":[schema(1,"old"),schema(7,"current")]}}));
    assert_eq!(
        f.polaris().table_schema(&table()).await.unwrap().columns[0].name,
        "current"
    );
    for metadata in [
        json!({"format-version":2,"schemas":[schema(1,"old")]}),
        json!({"format-version":2,"current-schema-id":8,"schemas":[schema(1,"old")]}),
        json!({"format-version":2,"current-schema-id":1,"schemas":[schema(1,"one"),schema(1,"duplicate")]}),
        json!({"format-version":4,"schema":schema(0,"future")}),
        json!({"format-version":2,"current-schema-id":7,"schemas":[{"schema-id":7,"fields":[]}]}),
    ] {
        f.respond(LOAD, 200, json!({"metadata":metadata}));
        assert!(
            f.polaris().table_schema(&table()).await.is_err(),
            "missing/ambiguous current schema must fail"
        );
    }
    f.respond(
        LOAD,
        200,
        json!({"metadata":{"format-version":1,"schema":schema(0,"legacy")}}),
    );
    assert_eq!(
        f.polaris().table_schema(&table()).await.unwrap().columns[0].name,
        "legacy"
    );
    let ns = "/api/catalog/v1/lake/namespaces";
    f.respond(
        ns,
        200,
        json!({"namespaces":[["sales.eu","raw"]],"next-page-token":"a&b"}),
    );
    f.respond(
        &format!("{ns}?pageToken=a%26b"),
        200,
        json!({"namespaces":[["sales","eu.raw"]]}),
    );
    let all = f.polaris().list_namespaces().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_ne!(all[0].segments, all[1].segments);
    let path = "/api/catalog/v1/lake/namespaces/sales.eu%1Fraw/tables";
    f.respond(path,200,json!({"identifiers":[{"namespace":["sales.eu","raw"],"name":"one"}],"next-page-token":"next"}));
    f.respond(
        &format!("{path}?pageToken=next"),
        200,
        json!({"identifiers":[{"namespace":["sales.eu","raw"],"name":"two"}]}),
    );
    let all = f
        .polaris()
        .list_tables_qualified(&table().namespace_segments)
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].namespace_segments, table().namespace_segments);
    f.respond(
        &format!("{path}?pageToken=next"),
        500,
        json!({"identifiers":[]}),
    );
    assert!(
        f.polaris()
            .list_tables_qualified(&table().namespace_segments)
            .await
            .is_err(),
        "a failed second page must discard partial results"
    );
    for bad in [
        json!({"identifiers":[] ,"next-page-token":""}),
        json!({"identifiers":[{"namespace":["other"],"name":"wrong"}]}),
        json!({"identifiers":[{"namespace":["sales.eu","raw"]}]}),
    ] {
        f.respond(path, 200, bad);
        assert!(f
            .polaris()
            .list_tables_qualified(&table().namespace_segments)
            .await
            .is_err());
    }
    for invalid in [
        json!({}),
        json!({"namespaces":[[1]]}),
        json!({"namespaces":[],"next-page-token":""}),
        json!({"namespaces":[],"next-page-token":5}),
    ] {
        f.respond(ns, 200, invalid);
        assert!(f.polaris().list_namespaces().await.is_err());
    }
    f.respond(ns, 200, json!({"namespaces":[],"next-page-token":"repeat"}));
    f.respond(
        &format!("{ns}?pageToken=repeat"),
        200,
        json!({"namespaces":[],"next-page-token":"repeat"}),
    );
    assert!(f.polaris().list_namespaces().await.is_err());
    for i in 0..=64 {
        f.respond(
            &if i == 0 {
                ns.into()
            } else {
                format!("{ns}?pageToken={i}")
            },
            200,
            json!({"namespaces":[],"next-page-token":format!("{}",i+1)}),
        );
    }
    assert!(
        f.polaris().list_namespaces().await.is_err(),
        "page limit is incomplete, never success"
    );
}

pub async fn provider_http_errors_are_not_empty_success() {
    let f = Fixture::start(true).await;
    f.respond(
        "/api/catalog/v1/lake/namespaces",
        200,
        json!({"namespaces":[]}),
    );
    let malformed = PolarisCatalog::new("p", &f.endpoint, "lake")
        .with_root_certificate(f.certificate.clone())
        .with_credential(Some("broken".into()));
    assert!(
        malformed.list_namespaces().await.is_err(),
        "malformed configured OAuth credential must not fall through to anonymous"
    );
    let cube = CubeCatalog::new("c", &f.endpoint, "cubejs-api")
        .with_root_certificate(f.certificate.clone());
    let oauth = PolarisCatalog::new("p", &f.endpoint, "lake")
        .with_root_certificate(f.certificate.clone())
        .with_credential(Some("fixture:oauth-secret".into()));
    for code in [401, 403, 429, 500] {
        f.respond(
            "/api/catalog/v1/oauth/tokens",
            code,
            json!({"access_token":"FIXTURE_TOKEN"}),
        );
        let error = oauth.list_namespaces().await.unwrap_err().to_string();
        assert!(error.contains(&code.to_string()));
        assert!(!error.contains("FIXTURE_TOKEN") && !error.contains("oauth-secret"));
        for path in ["/cubejs-api/v1/meta", "/api/v1/databaseSchemas?limit=200"] {
            f.respond(path, code, json!({"error":"FIXTURE_TOKEN"}));
        }
        for result in [cube.list_namespaces().await, f.om().list_namespaces().await] {
            let error = result
                .expect_err("HTTP failure must not be empty success")
                .to_string();
            assert!(!error.contains("FIXTURE_TOKEN"));
            assert!(error.contains(&code.to_string()));
        }
    }
    f.respond(
        "/api/catalog/v1/oauth/tokens",
        200,
        json!({"access_token":"FIXTURE_TOKEN"}),
    );
    oauth.list_namespaces().await.unwrap();
    let requests = f.requests.lock().unwrap().clone();
    assert!(requests
        .iter()
        .any(|r| r.starts_with("POST /api/catalog/v1/oauth/tokens ")
            && r.to_lowercase().contains("authorization: basic ")));
    assert!(requests
        .last()
        .unwrap()
        .to_lowercase()
        .contains("authorization: bearer fixture_token"));
    for malformed in [
        json!({"access_token":""}),
        json!({}),
        json!({"access_token":17}),
    ] {
        f.respond("/api/catalog/v1/oauth/tokens", 200, malformed);
        let before = f.requests.lock().unwrap().len();
        assert!(
            oauth.list_namespaces().await.is_err(),
            "missing or empty OAuth token must not produce anonymous metadata"
        );
        assert_eq!(
            f.requests.lock().unwrap().len(),
            before + 1,
            "invalid token must stop before metadata IO"
        );
    }
    f.respond("/cubejs-api/v1/meta", 200, json!({}));
    assert!(
        cube.list_namespaces().await.is_err(),
        "malformed metadata is not empty success"
    );
    f.raw(
        "/cubejs-api/v1/meta",
        302,
        "{}".into(),
        &format!("Location: {}/redirect?token=FIXTURE_TOKEN\r\n", f.endpoint),
        0,
    );
    assert!(cube.list_namespaces().await.is_err());
    assert!(!f
        .requests
        .lock()
        .unwrap()
        .iter()
        .any(|r| r.contains("GET /redirect")));
    f.raw(
        "/cubejs-api/v1/meta",
        200,
        format!(
            "{{\"cubes\":[],\"padding\":\"{}\"}}",
            "x".repeat(4 * 1024 * 1024)
        ),
        "",
        0,
    );
    assert!(
        cube.list_namespaces().await.is_err(),
        "body budget must be enforced before buffering"
    );
    f.raw(
        "/cubejs-api/v1/meta",
        200,
        format!(
            "{{\"cubes\":[],\"padding\":\"{}\"}}",
            "x".repeat(4 * 1024 * 1024)
        ),
        "Transfer-Encoding: chunked\r\n",
        0,
    );
    assert!(
        cube.list_namespaces().await.is_err(),
        "unknown-length streamed bodies must obey the same byte budget"
    );
    f.raw(
        "/cubejs-api/v1/meta",
        200,
        "{\"cubes\":[]}".into(),
        "",
        11_000,
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(12), cube.list_namespaces())
            .await
            .unwrap()
            .is_err(),
        "deadline must return an explicit error"
    );
}

pub async fn openmetadata_pages_and_registered_auth() {
    let f = Fixture::start(true).await;
    let path = "/api/v1/databaseSchemas?limit=200";
    f.respond(
        path,
        200,
        json!({"data":[{"fullyQualifiedName":"service.db.a&b"}],"paging":{"after":"a&b"}}),
    );
    f.respond(
        &format!("{path}&after=a%26b"),
        200,
        json!({"data":[{"fullyQualifiedName":"service.db.second"}],"paging":{}}),
    );
    assert_eq!(f.om().list_namespaces().await.unwrap().len(), 2);
    let path = "/api/v1/tables?limit=200&fields=columns&databaseSchema=service.db.a%26b";
    let rows: Vec<_> = (0..200)
        .map(|i| json!({"name":format!("table.{i}"),"columns":[]}))
        .collect();
    f.respond(path, 200, json!({"data":rows,"paging":{"after":"next"}}));
    f.respond(&format!("{path}&after=next"),200,json!({"data":[{"name":"last.part","columns":[{"name":"key","dataType":"BIGINT","constraint":"NOT_NULL"}]}],"paging":{}}));
    let tables = f
        .om()
        .list_tables_qualified(&["service.db.a&b".into()])
        .await
        .unwrap();
    assert_eq!(tables.len(), 201);
    assert_eq!(tables[200].name, "last.part");
    assert_eq!(tables[200].namespace_segments, vec!["service.db.a&b"]);
    assert!(!f.om().table_schema(&tables[200]).await.unwrap().columns[0].nullable);
    f.respond(&format!("{path}&after=next"), 500, json!({"data":[]}));
    assert!(f.om().list_tables("service.db.a&b").await.is_err());
    assert!(f.om().table_schema(&tables[200]).await.is_err());
    for bad in [
        json!({}),
        json!({"data":[{"name":"sales"}]}),
        json!({"data":[],"paging":{"after":""}}),
        json!({"data":[],"paging":{"after":5}}),
    ] {
        f.respond("/api/v1/databaseSchemas?limit=200", 200, bad);
        assert!(f.om().list_namespaces().await.is_err());
    }
    f.respond(
        "/api/v1/databaseSchemas?limit=200",
        200,
        json!({"data":[],"paging":{"after":"repeat"}}),
    );
    f.respond(
        "/api/v1/databaseSchemas?limit=200&after=repeat",
        200,
        json!({"data":[],"paging":{"after":"repeat"}}),
    );
    assert!(f.om().list_namespaces().await.is_err());
    // Neither selected AI observation nor legacy AI discovery may turn a
    // per-operation browse bound into an invented caller-budget guarantee.
    let count = f.requests.lock().unwrap().len();
    let cube = CubeCatalog::new("c", &f.endpoint, "cubejs-api")
        .with_root_certificate(f.certificate.clone());
    for catalog in [&f.polaris() as &dyn Catalog, &f.om(), &cube] {
        assert!(catalog.metadata_read_bytes().is_none());
        assert!(catalog
            .table_schema_bounded(&table(), 1024, 1)
            .await
            .is_err());
    }
    assert_eq!(
        f.requests.lock().unwrap().len(),
        count,
        "unavailable bounded AI reads must perform zero IO"
    );
    // Registration uses its ordinary production client against a loopback fixture.
    let wire = Fixture::start(false).await;
    wire.respond("/api/v1/databaseSchemas?limit=200", 200, json!({"data":[]}));
    wire.respond("/cubejs-api/v1/meta", 200, json!({"cubes":[]}));
    for kind in ["openmetadata", "cube"] {
        let config = CatalogConfig {
            id: kind.into(),
            kind: kind.into(),
            endpoint: wire.endpoint.clone(),
            catalog: None,
            token: Some(format!("fixture-{kind}")),
            credential: None,
        };
        catalog_from_config(&config)
            .unwrap()
            .list_namespaces()
            .await
            .unwrap();
        assert!(
            wire.requests
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .to_lowercase()
                .contains(&format!("authorization: bearer fixture-{kind}")),
            "registered token must reach selected provider"
        );
    }
}
