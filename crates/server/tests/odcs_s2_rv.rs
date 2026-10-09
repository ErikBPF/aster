#[path = "support/odcs_intake.rs"]
#[allow(dead_code)]
mod support;

#[tokio::test]
async fn browser_namespace_links_roundtrip() {
    support::namespace_checks("browser").await;
}

#[tokio::test]
async fn semantic_segments_only_are_faithful_or_refused() {
    support::namespace_checks("semantic").await;
}

#[tokio::test]
async fn opaque_provider_namespaces_roundtrip() {
    use aster_core::Catalog;
    let mock = aster_catalogs::MockCatalog::new("mock");
    let ns = vec!["aster_demo.sales".to_string()];
    let tables = mock
        .list_tables_qualified(&ns)
        .await
        .expect("opaque mock namespace");
    assert_eq!(tables[0].namespace_segments, ns);
    assert!(!mock
        .table_schema(&tables[0])
        .await
        .unwrap()
        .columns
        .is_empty());
    assert!(mock
        .list_tables_qualified(&["aster_demo".into(), "sales".into()])
        .await
        .is_err());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = axum::Router::new().route(
        "/api/v1/tables",
        axum::routing::get(
            |axum::extract::Query(q): axum::extract::Query<
                std::collections::HashMap<String, String>,
            >| async move {
                assert_eq!(q["databaseSchema"], "trino.platform.sales");
                axum::Json(serde_json::json!({"data":[{"name":"orders.part","columns":[]}]}))
            },
        ),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let om =
        aster_catalogs::OpenMetadataCatalog::new("om", format!("http://{address}"), None, None);
    let ns = vec!["trino.platform.sales".to_string()];
    let tables = om.list_descriptors_qualified(&ns).await.unwrap();
    assert_eq!(tables[0].table.namespace_segments, ns);
    assert_eq!(tables[0].table.name, "orders.part");
    om.table_schema(&tables[0].table).await.unwrap();
    task.abort();
}

#[test]
fn oversized_manifest_is_refused_before_parsing() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = vec![b' '; 1024 * 1024 + 1];
    std::fs::write(dir.path().join("manifest.json"), &bytes).unwrap();
    let result = aster_server::odcs_intake::load_bundle(
        dir.path(),
        &aster_server::odcs_intake::sha256(&bytes),
        "v",
    );
    let error = result.err().unwrap().to_string();
    assert!(error.contains("size limit"), "{error}");
}

#[test]
fn nonregular_and_oversized_inputs_are_bounded() {
    use aster_server::odcs_intake::*;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("compiled")).unwrap();
    let doc = b"apiVersion: v3.2.0\nkind: DataContract\nid: fixture\nversion: v\n";
    std::fs::write(dir.path().join("compiled/a.yaml"), doc).unwrap();
    let base = serde_json::json!({"manifestVersion":1,"bundle":"test","version":"v","documents":[{"path":"compiled/a.yaml","sha256":sha256(doc),"id":"fixture","version":"v","target":"one"}]});
    let write = |manifest: &serde_json::Value| {
        let bytes = manifest.to_string();
        std::fs::write(dir.path().join("manifest.json"), &bytes).unwrap();
        sha256(bytes.as_bytes())
    };
    let oversized = vec![b' '; MAX_DOCUMENT_BYTES + 1];
    std::fs::write(dir.path().join("compiled/a.yaml"), &oversized).unwrap();
    assert!(load_bundle(dir.path(), &write(&base), "v")
        .err()
        .unwrap()
        .to_string()
        .contains("size limit"));
    std::fs::write(dir.path().join("compiled/a.yaml"), doc).unwrap();
    let mut manifest = base.clone();
    manifest["documents"] =
        serde_json::Value::Array(vec![base["documents"][0].clone(); MAX_DOCUMENTS + 1]);
    assert!(load_bundle(dir.path(), &write(&manifest), "v").is_err());
    manifest = base.clone();
    manifest["bindingsSha256"] = serde_json::json!(sha256(b"{}"));
    std::fs::write(
        dir.path().join("bindings.json"),
        vec![b' '; MAX_CONFIG_BYTES + 1],
    )
    .unwrap();
    assert!(load_bundle(dir.path(), &write(&manifest), "v")
        .err()
        .unwrap()
        .to_string()
        .contains("size limit"));
    std::fs::remove_file(dir.path().join("bindings.json")).unwrap();
    let mut padded = doc.to_vec();
    padded.resize(MAX_DOCUMENT_BYTES, b' ');
    let mut selections = Vec::new();
    for index in 0..9 {
        let path = format!("compiled/padded-{index}.yaml");
        std::fs::write(dir.path().join(&path), &padded).unwrap();
        selections.push(serde_json::json!({"path":path,"sha256":sha256(&padded),"id":"fixture","version":"v","target":format!("target-{index}")}));
    }
    let mut aggregate_manifest = base.clone();
    aggregate_manifest["documents"] = serde_json::Value::Array(selections);
    assert!(load_bundle(dir.path(), &write(&aggregate_manifest), "v")
        .err()
        .unwrap()
        .to_string()
        .contains("size limit"));
    // Exercise FIFO at each shared reader call in a child process with a bounded wait.
    for relative in ["manifest.json", "compiled/a.yaml", "bindings.json"] {
        std::fs::write(dir.path().join("compiled/a.yaml"), doc).unwrap();
        let manifest = if relative == "bindings.json" {
            manifest.clone()
        } else {
            base.clone()
        };
        let digest = write(&manifest);
        let path = dir.path().join(relative);
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        let cpath = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_aster-server"))
            .env("ASTER_CONTRACT_BUNDLE", dir.path())
            .env("ASTER_CONTRACT_MANIFEST_SHA256", digest)
            .env("ASTER_CONTRACT_BUNDLE_VERSION", "v")
            .env("ASTER_METADATA_STORE", "memory")
            .env("ASTER_STATE_STORE", "memory")
            .env("ASTER_NOTEBOOK_DIR", dir.path().join("notebooks"))
            .env("ASTER_BIND", "127.0.0.1:0")
            .env("ASTER_METRICS_BIND", "127.0.0.1:0")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("FIFO blocked intake: {relative}");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("regular file"));
        std::fs::remove_file(path).unwrap();
    }
}
