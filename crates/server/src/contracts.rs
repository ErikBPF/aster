//! Loads data contracts from a directory at startup.
//!
//! Contract files are read once; changing a contract means restarting the pod (they are
//! deployment artifacts, not user data).

use std::path::Path;

use aster_core::DataContract;

/// Reads `*.json` contracts from `dir`. Missing directory is not an error: contracts are optional.
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

        match extension {
            Some("json") => match std::fs::read_to_string(&path) {
                Ok(text) => match DataContract::parse(&id, &text) {
                    Ok(contract) => contracts.push(contract),
                    Err(error) => tracing::warn!("skipping contract {name}: {error}"),
                },
                Err(error) => tracing::warn!("skipping contract {name}: {error}"),
            },
            // ponytail: yaml contracts need a yaml crate; keep JSON until a team stores yaml.
            Some("yaml" | "yml") => {
                tracing::warn!("skipping contract {name}: yaml contracts are not supported yet")
            }
            _ => {}
        }
    }

    contracts.sort_by(|left, right| left.id.cmp(&right.id));
    contracts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_json_contracts_and_ignores_yaml() {
        let dir = std::env::temp_dir().join(format!("aster-contracts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(
            dir.join("orders.json"),
            r#"{"name":"orders","schema":[{"name":"id","type":"bigint","required":true}]}"#,
        )
        .expect("write json");
        std::fs::write(dir.join("people.yaml"), "name: people\n").expect("write yaml");
        std::fs::write(dir.join("broken.json"), "{").expect("write broken");

        let contracts = load(&dir);

        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].name, "orders");
    }
}
