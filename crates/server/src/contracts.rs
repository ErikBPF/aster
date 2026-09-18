//! Loads data contracts from a directory at startup.
//!
//! Contract files are read once; changing a contract means restarting the pod (they are
//! deployment artifacts, not user data).

use std::path::Path;

use aster_core::DataContract;

/// Reads every `*.json`, `*.yaml` and `*.yml` contract from `dir`. A missing
/// directory is not an error: contracts are optional, and one that fails to parse
/// is skipped with a warning rather than stopping the server.
pub fn load(dir: &Path) -> Vec<DataContract> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => {
            tracing::debug!("no contracts directory at {}", dir.display());
            return Vec::new();
        }
    };

    let mut contracts = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let extension = path.extension().and_then(|extension| extension.to_str());
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();

        if !matches!(extension, Some("json" | "yaml" | "yml")) {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => match DataContract::parse(&id, &text) {
                Ok(contract) => {
                    tracing::info!("loaded data contract {name}");
                    contracts.push(contract);
                }
                Err(error) => tracing::warn!("skipping contract {name}: {error}"),
            },
            Err(error) => tracing::warn!("skipping contract {name}: {error}"),
        }
    }

    contracts.sort_by(|left, right| left.id.cmp(&right.id));
    contracts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_json_and_yaml_contracts_and_skips_broken_ones() {
        let dir = std::env::temp_dir().join(format!(
            "aster-contracts-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("orders.json"),
            r#"{"name":"orders","schema":[{"name":"id","type":"bigint","required":true}]}"#,
        )
        .expect("write json");
        std::fs::write(
            dir.join("people.yaml"),
            "apiVersion: v3.2.0\nkind: DataContract\nid: people\nschema:\n  - name: people\n    properties:\n      - name: name\n        logicalType: string\n",
        )
        .expect("write yaml");
        std::fs::write(dir.join("broken.json"), "{").expect("write broken");
        std::fs::write(dir.join("notes.txt"), "not a contract").expect("write notes");

        let contracts = load(&dir);

        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(
            contracts
                .iter()
                .map(|contract| contract.id.as_str())
                .collect::<Vec<_>>(),
            vec!["orders", "people"]
        );
        assert_eq!(contracts[0].fields.len(), 1);
        assert_eq!(contracts[1].fields[0].name, "name");
    }
}
