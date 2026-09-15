use serde::{Deserialize, Serialize};

/// Configuration for one engine instance. `kind` selects the plugin
/// ("trino", "spark", "starrocks"); adding a kind never changes the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub id: String,
    pub kind: String,
    pub endpoint: String,
}

/// Configuration for one catalog instance. `kind` selects the plugin
/// ("polaris", "nessie", "unity").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogConfig {
    pub id: String,
    pub kind: String,
    pub endpoint: String,
    /// Optional catalog/warehouse name within the backend (Polaris prefix).
    pub catalog: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub bind: String,
    pub engines: Vec<EngineConfig>,
    pub catalogs: Vec<CatalogConfig>,
    pub default_engine: Option<String>,
    pub default_catalog: Option<String>,
}

impl AppConfig {
    /// Development defaults so the scaffold runs without external config.
    pub fn from_env() -> Self {
        Self {
            bind: std::env::var("ASTER_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            engines: vec![EngineConfig {
                id: "trino-local".into(),
                kind: "trino".into(),
                endpoint: std::env::var("TRINO_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:8080".into()),
            }],
            catalogs: vec![CatalogConfig {
                id: "polaris-local".into(),
                kind: "polaris".into(),
                endpoint: std::env::var("POLARIS_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:8181".into()),
                catalog: Some(
                    std::env::var("POLARIS_CATALOG")
                        .unwrap_or_else(|_| "quickstart_catalog".into()),
                ),
            }],
            default_engine: Some("trino-local".into()),
            default_catalog: Some("polaris-local".into()),
        }
    }
}
