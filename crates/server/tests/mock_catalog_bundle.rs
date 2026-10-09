use aster_core::Catalog;
use aster_server::odcs_intake as intake;
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

// Fixture consistency, not a runtime policy or executed Cube proof.
#[tokio::test]
async fn mock_bundle_covers_catalog_and_preserves_pinned_sources() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/mock-catalog");
    let pins: Value =
        serde_json::from_slice(&std::fs::read(root.join("fixture-pins.json")).unwrap()).unwrap();
    assert_eq!(intake::sha256(intake::SCHEMA), intake::SCHEMA_SHA256);
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let bundle = intake::load_bundle(
        &root,
        pins["manifestSha256"].as_str().unwrap(),
        manifest["version"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        bundle.documents.len(),
        5,
        "one complete contract per mock materialized table"
    );
    assert_eq!(bundle.version, "mock-catalog-r23");
    let grants: Value =
        serde_json::from_slice(&std::fs::read(root.join("grants.json")).unwrap()).unwrap();
    assert_eq!(grants["formatVersion"], 1);
    assert_eq!(grants["grants"].as_array().unwrap().len(), 5);
    assert_eq!(
        intake::sha256(&std::fs::read(root.join("grants.json")).unwrap()),
        pins["grantsSha256"].as_str().unwrap()
    );
    let selected_files: BTreeSet<_> = bundle
        .documents
        .iter()
        .map(|a| root.join(&a.selection.path))
        .collect();
    let compiled_files: BTreeSet<_> = std::fs::read_dir(root.join("compiled"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(
        selected_files, compiled_files,
        "no superseded schema-level documents remain"
    );
    assert_eq!(
        bundle
            .documents
            .iter()
            .map(|a| &a.selection.id)
            .collect::<BTreeSet<_>>()
            .len(),
        5
    );
    let realm: Value = serde_json::from_slice(
        &std::fs::read(root.join("../../keycloak/aster-realm.json")).unwrap(),
    )
    .unwrap();
    let alice = realm["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "alice")
        .unwrap();
    assert_eq!(alice["groups"], serde_json::json!(["/aster-editors"]));

    let catalog = aster_catalogs::MockCatalog::new("mock-local");
    let mut observed = BTreeSet::new();
    for namespace in catalog.list_namespaces().await.unwrap() {
        for table in catalog
            .list_tables_qualified(&namespace.segments)
            .await
            .unwrap()
        {
            observed.insert((namespace.segments.clone(), table.name));
        }
    }
    let bindings = &bundle.physical_bindings.as_ref().unwrap().bindings;
    assert_eq!(bindings.len(), 5);
    let mut covered = BTreeSet::new();
    for artifact in &bundle.documents {
        let bytes = std::fs::read(root.join(&artifact.selection.path)).unwrap();
        assert_eq!(artifact.document.source, bytes);
        let raw: Value = serde_norway::from_slice(&bytes).unwrap();
        assert_eq!(artifact.document.raw, raw);
        assert_eq!(raw["apiVersion"], "v3.2.0");
        assert_eq!(raw["status"], "draft");
        assert_eq!(raw["schema"].as_array().unwrap().len(), 1);
        assert_eq!(
            bindings
                .iter()
                .filter(|b| b.path == artifact.selection.path)
                .count(),
            1
        );
        intake::validate_schema(&raw).unwrap();
        assert!(grants["grants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["path"] == artifact.selection.path
                && g["sha256"] == artifact.selection.sha256
                && g["teams"] == serde_json::json!(["aster-editors"])));
    }
    for binding in bindings {
        assert_eq!(binding.catalog, "mock-local");
        let artifact = bundle
            .documents
            .iter()
            .find(|a| a.selection.path == binding.path)
            .unwrap();
        let object = artifact.document.raw.pointer(&binding.object).unwrap();
        assert_eq!(binding.object, "/schema/0");
        let stable_id = match binding.physical_name.as_str() {
            "orders" => "aster-mock-demo-orders",
            "customers" => "aster-mock-demo-customers",
            "daily_revenue" => "aster-mock-demo-sales-daily-revenue",
            "events" => "aster-mock-raw-events",
            "clickstream" => "aster-mock-raw-clickstream",
            other => panic!("unexpected mock materialization: {other}"),
        };
        assert_eq!(artifact.selection.id, stable_id);
        assert_eq!(object["physicalName"], binding.physical_name);
        let schema = catalog
            .table_schema(&aster_core::TableRef {
                namespace: binding.namespace_segments.join("."),
                namespace_segments: binding.namespace_segments.clone(),
                name: binding.physical_name.clone(),
            })
            .await
            .unwrap();
        let declared: Vec<_> = object["properties"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["physicalName"].as_str().unwrap(),
                    p["physicalType"].as_str().unwrap(),
                )
            })
            .collect();
        let actual: Vec<_> = schema
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.data_type.as_str()))
            .collect();
        assert_eq!(declared, actual);
        assert!(covered.insert((
            binding.namespace_segments.clone(),
            binding.physical_name.clone()
        )));
    }
    assert_eq!(observed, covered);
    let mut semantic_coverage = BTreeSet::new();
    for pin in pins["semantics"].as_array().unwrap() {
        let bytes = std::fs::read(root.join(pin["path"].as_str().unwrap())).unwrap();
        assert_eq!(intake::sha256(&bytes), pin["sha256"].as_str().unwrap());
        let semantic: Value = serde_norway::from_slice(&bytes).unwrap();
        assert_eq!(semantic["mock"], true);
        assert_eq!(semantic["ownerGroup"], alice["groups"][0]);
        assert_eq!(semantic["ownerTeam"], "aster-editors");
        assert_eq!(semantic["formatVersion"], 2);
        assert!(
            semantic.get("contract").is_none(),
            "schema semantics spans independent table contracts"
        );
        for table in semantic["tables"].as_array().unwrap() {
            let artifact = bundle
                .documents
                .iter()
                .find(|a| table["contract"]["path"] == a.selection.path)
                .unwrap();
            assert_eq!(table["contract"]["id"], artifact.selection.id);
            assert_eq!(table["contract"]["version"], artifact.selection.version);
            assert_eq!(table["contract"]["sha256"], artifact.selection.sha256);
            let object = table["object"].as_str().unwrap();
            let binding = bindings
                .iter()
                .find(|b| b.path == artifact.selection.path && b.object == object)
                .unwrap();
            assert_eq!(semantic["catalog"], binding.catalog);
            assert_eq!(
                semantic["namespaceSegments"],
                serde_json::json!(binding.namespace_segments)
            );
            assert_eq!(table["physicalName"], binding.physical_name);
            assert_eq!(table["joins"], serde_json::json!([]));
            assert!(semantic_coverage.insert((
                binding.namespace_segments.clone(),
                binding.physical_name.clone()
            )));
            for measure in table["measures"].as_array().unwrap() {
                match measure["type"].as_str().unwrap() {
                    "count" => assert!(measure.get("sql").is_none()),
                    "sum" | "avg" => {
                        assert!(artifact.document.raw.pointer(object).unwrap()["properties"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|p| p["physicalName"] == measure["sql"]
                                && p["logicalType"] == "number"))
                    }
                    other => panic!("undeclared fixture measure type: {other}"),
                }
            }
        }
    }
    assert_eq!(semantic_coverage, covered);
    assert!(intake::load_bundle(&root, &"0".repeat(64), "mock-catalog-r23").is_err());
}
