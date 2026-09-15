use serde::{Deserialize, Serialize};

/// Configuration for one engine instance. `kind` selects the plugin
/// ("trino", "spark", "starrocks"); adding a kind never changes the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub id: String,
    pub kind: String,
    pub endpoint: String,
    /// Gateway routing group, when the endpoint is a gateway fronting a pool.
    #[serde(default)]
    pub routing_group: Option<String>,
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
    ///
    /// `ASTER_ENGINES` / `ASTER_CATALOGS` accept a comma-separated pool, one
    /// entry per instance, fields separated by `;`:
    /// `id;kind;endpoint[;routing_group]` and `id;kind;endpoint[;catalog]`.
    pub fn from_env() -> Self {
        let engines = std::env::var("ASTER_ENGINES")
            .ok()
            .map(|spec| parse_engines(&spec))
            .filter(|engines| !engines.is_empty())
            .unwrap_or_else(|| {
                vec![EngineConfig {
                    id: "trino-local".into(),
                    kind: "trino".into(),
                    endpoint: std::env::var("TRINO_ENDPOINT")
                        .unwrap_or_else(|_| "http://localhost:8080".into()),
                    routing_group: std::env::var("TRINO_ROUTING_GROUP").ok(),
                }]
            });

        let catalogs = std::env::var("ASTER_CATALOGS")
            .ok()
            .map(|spec| parse_catalogs(&spec))
            .filter(|catalogs| !catalogs.is_empty())
            .unwrap_or_else(|| {
                vec![CatalogConfig {
                    id: "polaris-local".into(),
                    kind: "polaris".into(),
                    endpoint: std::env::var("POLARIS_ENDPOINT")
                        .unwrap_or_else(|_| "http://localhost:8181".into()),
                    catalog: Some(
                        std::env::var("POLARIS_CATALOG")
                            .unwrap_or_else(|_| "quickstart_catalog".into()),
                    ),
                }]
            });

        let default_engine = std::env::var("ASTER_DEFAULT_ENGINE")
            .ok()
            .or_else(|| engines.first().map(|engine| engine.id.clone()));
        let default_catalog = std::env::var("ASTER_DEFAULT_CATALOG")
            .ok()
            .or_else(|| catalogs.first().map(|catalog| catalog.id.clone()));

        Self {
            bind: std::env::var("ASTER_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            engines,
            catalogs,
            default_engine,
            default_catalog,
        }
    }
}

fn field(value: Option<&&str>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_engines(spec: &str) -> Vec<EngineConfig> {
    spec.split(',')
        .filter_map(|entry| {
            let fields: Vec<&str> = entry.split(';').map(str::trim).collect();
            let (id, kind, endpoint) = (fields.first()?, fields.get(1)?, fields.get(2)?);
            if id.is_empty() || kind.is_empty() || endpoint.is_empty() {
                return None;
            }
            Some(EngineConfig {
                id: (*id).to_string(),
                kind: (*kind).to_string(),
                endpoint: (*endpoint).to_string(),
                routing_group: field(fields.get(3)),
            })
        })
        .collect()
}

fn parse_catalogs(spec: &str) -> Vec<CatalogConfig> {
    spec.split(',')
        .filter_map(|entry| {
            let fields: Vec<&str> = entry.split(';').map(str::trim).collect();
            let (id, kind, endpoint) = (fields.first()?, fields.get(1)?, fields.get(2)?);
            if id.is_empty() || kind.is_empty() || endpoint.is_empty() {
                return None;
            }
            Some(CatalogConfig {
                id: (*id).to_string(),
                kind: (*kind).to_string(),
                endpoint: (*endpoint).to_string(),
                catalog: field(fields.get(3)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_engine_pool_with_routing_groups() {
        let engines =
            parse_engines("adhoc;trino;http://gw:8080;adhoc, etl;trino;http://gw:8080;etl");
        assert_eq!(engines.len(), 2);
        assert_eq!(engines[0].id, "adhoc");
        assert_eq!(engines[0].endpoint, "http://gw:8080");
        assert_eq!(engines[0].routing_group.as_deref(), Some("adhoc"));
        assert_eq!(engines[1].routing_group.as_deref(), Some("etl"));
    }

    #[test]
    fn routes_are_optional_and_malformed_entries_drop() {
        let engines = parse_engines("solo;trino;http://localhost:8080,,broken");
        assert_eq!(engines.len(), 1);
        assert_eq!(engines[0].routing_group, None);
    }
}
