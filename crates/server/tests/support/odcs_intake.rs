use aster_server::odcs_intake as intake;

pub async fn namespace_segments_survive_adapter_boundaries() {
    namespace_checks("transport").await;
}

pub async fn namespace_checks(mode: &str) {
    use aster_core::Catalog;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let service = axum::Router::new().route(
        "/api/catalog/v1/test/namespaces",
        axum::routing::get(|| async {
            axum::Json(serde_json::json!({"namespaces":[["sales.eu"],["sales","eu"]]}))
        }),
    ).route("/api/catalog/v1/test/namespaces/{namespace}/tables", axum::routing::get(|axum::extract::Path(namespace): axum::extract::Path<String>| async move {
        axum::Json(serde_json::json!({"identifiers":[{"namespace":namespace.split('\u{1f}').collect::<Vec<_>>(),"name":"orders.part"}]}))
    })).route("/api/catalog/v1/test/namespaces/{namespace}/tables/{table}", axum::routing::get(|| async {
        axum::Json(serde_json::json!({"metadata":{"format-version":2,"current-schema-id":0,"schemas":[{"schema-id":0,"type":"struct","fields":[]}]}}))
    }));
    let server = tokio::spawn(async move {
        axum::serve(listener, service).await.unwrap();
    });
    let catalog =
        aster_catalogs::PolarisCatalog::new("fixture", format!("http://{address}"), "test");
    let namespaces = catalog.list_namespaces().await.unwrap();
    let raw = serde_json::to_value(namespaces).unwrap();
    assert_eq!(raw[0]["segments"], serde_json::json!(["sales.eu"]));
    assert_eq!(raw[1]["segments"], serde_json::json!(["sales", "eu"]));
    for segments in [
        vec!["sales.eu".to_string()],
        vec!["sales".to_string(), "eu".to_string()],
    ] {
        let tables = catalog.list_tables_qualified(&segments).await.unwrap();
        assert_eq!(tables[0].namespace_segments, segments);
        assert_eq!(tables[0].name, "orders.part");
    }
    assert!(catalog.list_tables("sales.eu").await.is_err());
    use aster_core::*;
    use std::sync::Arc;
    use tower::ServiceExt;
    let mut catalogs = CatalogRegistry::new();
    catalogs.register(Arc::new(catalog));
    let app = aster_server::app(Arc::new(aster_server::AppState {
        config: AppConfig {
            bind: String::new(),
            engines: vec![EngineConfig {
                id: "fixture".into(),
                kind: "mock".into(),
                endpoint: "local".into(),
                routing_group: None,
                delegation: None,
            }],
            catalogs: vec![CatalogConfig {
                id: "fixture".into(),
                kind: "fixture".into(),
                endpoint: "local".into(),
                catalog: None,
                token: None,
                credential: None,
            }],
            catalog_bindings: vec![CatalogBindingConfig {
                catalog: "fixture".into(),
                engine: "fixture".into(),
                native_catalog: "fixture".into(),
                policy: BindingPolicy::Unprotected,
            }],
            default_engine: None,
            default_catalog: None,
        },
        engines: EngineRegistry::new(),
        catalogs,
        grants: Arc::new(InMemoryGrants::new()),
        audit: Arc::new(InMemoryAudit::new()),
        notebooks: Arc::new(NoNotebooks),
        notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
        notebook_write: Arc::new(tokio::sync::Mutex::new(())),
        team_workspaces: None,
        team_git_targets: None,
        llm: Arc::new(InMemoryLlm::new()),
        shared_models: None,
        current_identity: None,
        shared_model_use_enabled: false,
        conversations: Arc::new(InMemoryConversations::default()),
        exchanges: Arc::new(InMemoryExchanges::default()),
        compiled_contracts: None,
        contracts: Arc::new(vec![]),
        http: reqwest::Client::new(),
        sessions: Arc::new(InMemorySessions::new(3600)),
        handshakes: Arc::new(InMemoryHandshakes::new(300)),
        session_ttl_seconds: 3600,
        user_state: Arc::new(InMemoryUserState::new()),
        secrets: Arc::new(InMemorySecrets::new()),
        identity: None,
        dev_login: true,
        metrics: Arc::new(aster_server::Metrics::new()),
    }));

    for segments in [
        serde_json::json!(["sales.eu"]),
        serde_json::json!(["sales", "eu"]),
    ] {
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/aster.v1.Aster/ListTables")
            .header("content-type", "application/json")
            .header("connect-protocol-version", "1")
            .header("x-aster-subject", "alice")
            .header("x-aster-roles", "viewer")
            .body(axum::body::Body::from(
                serde_json::json!({"catalog":"fixture","namespaceSegments":segments}).to_string(),
            ))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["tables"][0]["namespaceSegments"], segments);
        assert_eq!(value["tables"][0]["name"], "orders.part");
    }
    if mode == "browser" {
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/catalog/physical")
                    .header("x-aster-subject", "alice")
                    .header("x-aster-roles", "viewer")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let html = String::from_utf8(
            axum::body::to_bytes(response.into_body(), 100000)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        let links: Vec<_> = html
            .split("href=\"")
            .skip(1)
            .filter_map(|part| part.split('"').next())
            .filter(|url| url.starts_with("/catalog/fixture/"))
            .collect();
        assert_eq!(links.len(), 2);
        assert_ne!(
            links[0], links[1],
            "namespace links collapse distinct arrays"
        );
        for link in links {
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(link)
                        .header("x-aster-subject", "alice")
                        .header("x-aster-roles", "viewer")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            let html = String::from_utf8(
                axum::body::to_bytes(response.into_body(), 100000)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            let table_link = html
                .split("href=\"")
                .skip(1)
                .filter_map(|p| p.split('"').next())
                .find(|url| url.ends_with("/orders.part"))
                .unwrap();
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(table_link)
                        .header("x-aster-subject", "alice")
                        .header("x-aster-roles", "viewer")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
        }
    }
    if mode == "semantic" {
        for (segments, expected) in [
            (serde_json::json!(["sales"]), 200),
            (serde_json::json!(["sales.eu"]), 400),
            (serde_json::json!(["sales", "eu"]), 400),
        ] {
            let response = app.clone().oneshot(axum::http::Request::builder().method("POST").uri("/aster.v1.Aster/RenderSemantic")
                .header("content-type", "application/json").header("connect-protocol-version", "1")
                .header("x-aster-subject", "alice").header("x-aster-roles", "viewer")
                .body(axum::body::Body::from(serde_json::json!({"catalog":"fixture","namespaceSegments":segments,"table":"orders","target":"odcs"}).to_string())).unwrap()).await.unwrap();
            assert_eq!(response.status().as_u16(), expected);
            let bytes = axum::body::to_bytes(response.into_body(), 100000)
                .await
                .unwrap();
            if expected == 200 {
                assert!(
                    String::from_utf8_lossy(&bytes).contains("sales.orders"),
                    "segments-only render lost namespace: {:?}",
                    bytes
                );
            }
        }
    }
    server.abort();
}

pub fn intake_preservation_does_not_expand_legacy_disclosure() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let directory = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aster-server"))
        .env("ASTER_CONTRACT_BUNDLE", directory.path())
        .env("ASTER_CONTRACT_MANIFEST_SHA256", "0".repeat(64))
        .env("ASTER_CONTRACT_BUNDLE_VERSION", "fixture")
        .env("ASTER_METADATA_STORE", "memory")
        .env("ASTER_STATE_STORE", "memory")
        .env("ASTER_NOTEBOOK_DIR", directory.path().join("notebooks"))
        .env("ASTER_BIND", "127.0.0.1:0")
        .env("ASTER_METRICS_BIND", "127.0.0.1:0")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                !status.success(),
                "invalid configured bundle must refuse startup"
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("invalid configured bundle was ignored; server kept running");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn compiled_bundle_requires_no_author_tree() {
    let bundle = tempfile::tempdir().unwrap();
    std::fs::create_dir(bundle.path().join("compiled")).unwrap();
    let document = b"apiVersion: v3.2.0\nkind: DataContract\nid: sales\nversion: 1.4.0\n";
    std::fs::write(bundle.path().join("compiled/sales.yaml"), document).unwrap();
    use sha2::{Digest, Sha256};
    let manifest = serde_json::json!({
        "manifestVersion": 1, "bundle": "sales", "version": "2026-09-28",
        "documents": [{"path":"compiled/sales.yaml", "sha256":format!("{:x}", Sha256::digest(document)),
                       "id":"sales", "version":"1.4.0", "target":"warehouse"}]
    });
    std::fs::write(bundle.path().join("manifest.json"), manifest.to_string()).unwrap();
    let loaded = intake::load_bundle(
        bundle.path(),
        &intake::sha256(manifest.to_string().as_bytes()),
        "2026-09-28",
    )
    .unwrap();
    assert_eq!(
        loaded.documents.len(),
        1,
        "manifest-selected nested compiled document must load"
    );
    assert_eq!(loaded.documents[0].document.projection.id, "sales");
    assert_eq!(loaded.documents[0].document.source, document);
    assert_eq!(loaded.documents[0].selection.target, "warehouse");
    assert_eq!(loaded.bundle, "sales");
    assert_eq!(loaded.version, "2026-09-28");
    assert_eq!(
        loaded.manifest_sha256,
        intake::sha256(manifest.to_string().as_bytes())
    );
    let rich = serde_json::json!({
        "apiVersion":"v3.2.0", "kind":"DataContract", "id":"sales", "version":"1.4.0",
        "description":{"purpose":"INTERNAL_ONLY_MEANING"},
        "schema":[
            {"id":"orders", "name":"orders", "physicalName":"order_rows", "properties":[
                {"id":"order-amount", "name":"amount", "logicalType":"number", "semanticType":"measure", "transformLogic":"SUM(amount)"},
                {"name":"details", "logicalType":"object", "properties":[{"name":"code", "logicalType":"string"}]}
            ]},
            {"id":"refunds", "name":"refunds", "properties":[{"id":"refund-amount", "name":"amount", "logicalType":"number"}]}
        ]
    });
    let text = rich.to_string();
    let preserved = aster_core::odcs::OdcsDocument::parse(text.as_bytes()).unwrap();
    intake::validate_schema(&preserved.raw).unwrap();
    assert_eq!(preserved.source, text.as_bytes());
    assert_eq!(preserved.raw, rich);
    assert_eq!(preserved.projection.schema.len(), 2);
    assert_eq!(
        preserved.projection.schema[0].properties[1].properties[0].name,
        "code"
    );
    assert!(intake::load_bundle(bundle.path(), &"0".repeat(64), "2026-09-28").is_err());
    assert!(intake::load_bundle(bundle.path(), &loaded.manifest_sha256, "wrong-version").is_err());
    std::fs::write(bundle.path().join("compiled/sales.yaml"), b"corrupted").unwrap();
    assert!(intake::load_bundle(bundle.path(), &loaded.manifest_sha256, "2026-09-28").is_err());
}

pub fn offline_schema_validation_is_distinct_from_support() {
    assert_eq!(intake::sha256(intake::SCHEMA), intake::SCHEMA_SHA256);
    let valid = serde_json::json!({"apiVersion":"v3.2.0","kind":"DataContract","id":"sales","version":"1.4.0"});
    assert!(intake::validate_schema(&valid).is_ok());
    let mut invalid = valid.clone();
    invalid["schema"] =
        serde_json::json!([{"name":"orders","properties":[{"name":"id","logicalType":"bigint"}]}]);
    assert!(intake::validate_schema(&invalid).is_err());
    invalid = valid;
    invalid.as_object_mut().unwrap().remove("version");
    assert!(intake::validate_schema(&invalid).is_err());
}

pub fn schema_valid_v31_is_rejected_without_conversion() {
    let raw = serde_json::json!({"apiVersion":"v3.1.0","kind":"DataContract","id":"sales","version":"1.4.0"});
    assert!(intake::validate_schema(&raw).is_ok());
    let source = raw.to_string();
    assert!(aster_core::odcs::OdcsDocument::parse(source.as_bytes()).is_err());
    assert_eq!(source, raw.to_string());
}

pub fn compiled_bundle_selection_and_pins_are_explicit() {
    let bundle = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(bundle.path().join("compiled/nested")).unwrap();
    let mut selections = Vec::new();
    for (file, version, target) in [
        ("a", "1.0", "warehouse"),
        ("b", "1.0", "lake"),
        ("c", "2.0", "warehouse"),
    ] {
        let raw = serde_json::json!({"apiVersion":"v3.2.0","kind":"DataContract","id":"same-id","version":version});
        let path = format!("compiled/nested/{file}.json");
        std::fs::write(bundle.path().join(&path), raw.to_string()).unwrap();
        selections.push(serde_json::json!({"path":path,"sha256":intake::sha256(raw.to_string().as_bytes()),"id":"same-id","version":version,"target":target}));
    }
    std::fs::write(
        bundle.path().join("compiled/intermediate.yaml"),
        "not a contract",
    )
    .unwrap();
    let mut manifest = serde_json::json!({"manifestVersion":1,"bundle":"fixture","version":"pinned","documents":selections});
    let write = |manifest: &serde_json::Value| {
        let bytes = manifest.to_string();
        std::fs::write(bundle.path().join("manifest.json"), &bytes).unwrap();
        intake::sha256(bytes.as_bytes())
    };
    let digest = write(&manifest);
    let loaded = intake::load_bundle(bundle.path(), &digest, "pinned").unwrap();
    assert_eq!(loaded.documents.len(), 3);
    assert_eq!(
        loaded
            .documents
            .iter()
            .map(|d| d.selection.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "compiled/nested/a.json",
            "compiled/nested/b.json",
            "compiled/nested/c.json"
        ]
    );
    manifest["documents"][1]["target"] = serde_json::json!("warehouse");
    assert!(intake::load_bundle(bundle.path(), &write(&manifest), "pinned").is_err());
    manifest["documents"][1]["target"] = serde_json::json!("lake");
    manifest["documents"][0]["version"] = serde_json::json!("wrong");
    assert!(intake::load_bundle(bundle.path(), &write(&manifest), "pinned").is_err());
    manifest["documents"][0]["path"] = serde_json::json!("compiled/missing.json");
    assert!(intake::load_bundle(bundle.path(), &write(&manifest), "pinned").is_err());
    // A configured physical mapping cannot disappear silently during intake.
    manifest["documents"] = serde_json::json!([selections[0].clone()]);
    std::fs::write(bundle.path().join("bindings.json"), r#"{"formatVersion":1,"version":"reviewed-1","bindings":[{"path":"compiled/nested/a.json","object":"/schema/99","catalog":"lake","namespaceSegments":["sales.eu"],"physicalName":"orders"}]}"#).unwrap();
    assert!(
        intake::load_bundle(bundle.path(), &write(&manifest), "pinned").is_err(),
        "invalid physical binding must reject bundle"
    );
    let document = serde_json::json!({"apiVersion":"v3.2.0","kind":"DataContract","id":"same-id","version":"1.0","schema":[{"name":"orders"}]});
    std::fs::write(
        bundle.path().join("compiled/nested/a.json"),
        document.to_string(),
    )
    .unwrap();
    manifest["documents"][0]["sha256"] =
        serde_json::json!(intake::sha256(document.to_string().as_bytes()));
    let bindings = serde_json::json!({"formatVersion":1,"version":"reviewed-1","bindings":[{"path":"compiled/nested/a.json","object":"/schema/0","catalog":"lake","namespaceSegments":["sales.eu"],"physicalName":"orders.part"}]});
    std::fs::write(bundle.path().join("bindings.json"), bindings.to_string()).unwrap();
    manifest["bindingsSha256"] = serde_json::json!(intake::sha256(bindings.to_string().as_bytes()));
    let loaded = intake::load_bundle(bundle.path(), &write(&manifest), "pinned").unwrap();
    let physical = loaded.physical_bindings.unwrap();
    assert_eq!(physical.version, "reviewed-1");
    assert_eq!(physical.bindings[0].namespace_segments, ["sales.eu"]);
    assert_eq!(physical.bindings[0].physical_name, "orders.part");
    assert_eq!(physical.bindings[0].object, "/schema/0");
    std::fs::write(bundle.path().join("bindings.json"), "{}").unwrap();
    assert!(intake::load_bundle(bundle.path(), &write(&manifest), "pinned").is_err());
}

struct NoNotebooks;
#[async_trait::async_trait]
impl aster_core::NotebookStore for NoNotebooks {
    async fn get(&self, _: &str) -> aster_core::Result<aster_core::Notebook> {
        unreachable!()
    }
    async fn list(&self, _: &str) -> aster_core::Result<Vec<String>> {
        unreachable!()
    }
    async fn save(&self, _: &aster_core::Notebook, _: &str) -> aster_core::Result<String> {
        unreachable!()
    }
}
