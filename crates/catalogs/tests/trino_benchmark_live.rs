use aster_catalogs::catalog_from_config;
use aster_core::{CatalogConfig, TableRef};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

#[tokio::test]
#[ignore = "requires the isolated native Trino benchmark backend"]
async fn all_native_benchmark_tables_and_columns_match_live_metadata() {
    let endpoint = std::env::var("ASTER_BENCHMARK_TRINO").expect("explicit Trino endpoint");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/benchmark-catalog");
    let observed: Value =
        serde_json::from_slice(&std::fs::read(root.join("observed-metadata.json")).unwrap())
            .unwrap();
    for name in ["tpch", "tpcds"] {
        let catalog = catalog_from_config(&CatalogConfig {
            id: name.into(),
            kind: "trino".into(),
            endpoint: endpoint.clone(),
            catalog: Some(name.into()),
            token: None,
            credential: None,
        })
        .unwrap();
        let namespace = vec!["tiny".to_owned()];
        assert!(catalog
            .list_namespaces()
            .await
            .unwrap()
            .iter()
            .any(|entry| entry.segments == namespace));
        let tables = catalog.list_tables_qualified(&namespace).await.unwrap();
        let expected: BTreeSet<_> = observed[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row[0].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            tables
                .iter()
                .map(|table| table.name.clone())
                .collect::<BTreeSet<_>>(),
            expected
        );
        for table in tables {
            let actual = catalog
                .table_schema(&TableRef {
                    namespace: "tiny".into(),
                    namespace_segments: namespace.clone(),
                    name: table.name.clone(),
                })
                .await
                .unwrap();
            let expected: Vec<_> = observed[name]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row[0] == table.name)
                .map(|row| (row[1].as_str().unwrap(), row[2].as_str().unwrap()))
                .collect();
            assert_eq!(
                actual
                    .columns
                    .iter()
                    .map(|column| (column.name.as_str(), column.data_type.as_str()))
                    .collect::<Vec<_>>(),
                expected,
                "{name}.tiny.{}",
                table.name
            );
        }
    }
}
