use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
    /// Protected Trino only: an externally rotated short-lived service JWT
    /// and the CA certificate for its HTTPS endpoint.
    #[serde(default)]
    pub delegation: Option<TrinoDelegationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrinoDelegationConfig {
    pub token_file: String,
    pub ca_file: String,
}

/// Configuration for one catalog instance. `kind` selects the plugin
/// ("polaris", "nessie", "unity").
#[derive(Clone, Serialize, Deserialize)]
pub struct CatalogConfig {
    pub id: String,
    pub kind: String,
    pub endpoint: String,
    /// Optional catalog/warehouse name within the backend (Polaris prefix).
    pub catalog: Option<String>,
    /// A ready bearer token or `secret://KEY` resolved by the selected secret store.
    #[serde(default)]
    pub token: Option<String>,
    /// OAuth2 client credentials as `client_id:client_secret`, exchanged for a
    /// token on demand, or `secret://KEY`. Polaris only; a ready token takes precedence.
    #[serde(default)]
    pub credential: Option<String>,
}

impl std::fmt::Debug for CatalogConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogConfig")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("catalog", &self.catalog)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// One explicit browse catalog to engine connection. `native_catalog` is the
/// engine's SQL catalog name, which need not match the browse instance ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingPolicy {
    /// Local development only. This makes no per-user backend permission claim.
    Unprotected,
    /// Refused until the engine supplies authenticated user delegation.
    Protected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogBindingConfig {
    pub catalog: String,
    pub engine: String,
    pub native_catalog: String,
    pub policy: BindingPolicy,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub bind: String,
    pub engines: Vec<EngineConfig>,
    pub catalogs: Vec<CatalogConfig>,
    #[serde(default)]
    pub catalog_bindings: Vec<CatalogBindingConfig>,
    pub default_engine: Option<String>,
    pub default_catalog: Option<String>,
}

impl std::fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppConfig")
            .field("bind", &self.bind)
            .field("engines", &self.engines)
            .field("catalogs", &self.catalogs)
            .field("catalog_bindings", &self.catalog_bindings)
            .field("default_engine", &self.default_engine)
            .field("default_catalog", &self.default_catalog)
            .finish()
    }
}

impl AppConfig {
    /// Development defaults so the scaffold runs without external config.
    ///
    /// `ASTER_ENGINES` / `ASTER_CATALOGS` accept a comma-separated pool, one
    /// entry per instance, fields separated by `;`:
    /// `id;kind;endpoint[;routing_group[;token_file;ca_file]]` and
    /// `id;kind;endpoint[;catalog[;token[;credential]]]`, where `credential` is
    /// the OAuth2 `client_id:client_secret` pair used to mint a token.
    /// `ASTER_CATALOG_BINDINGS` uses
    /// `catalog_id;engine_id;native_sql_catalog;unprotected|protected`.
    pub fn from_env() -> Result<Self> {
        let engines = match std::env::var("ASTER_ENGINES") {
            Ok(spec) if !spec.trim().is_empty() => parse_engines(&spec)?,
            _ => {
                vec![EngineConfig {
                    id: "trino-local".into(),
                    kind: "trino".into(),
                    endpoint: std::env::var("TRINO_ENDPOINT")
                        .unwrap_or_else(|_| "http://localhost:8080".into()),
                    routing_group: std::env::var("TRINO_ROUTING_GROUP").ok(),
                    delegation: None,
                }]
            }
        };

        let catalogs = match std::env::var("ASTER_CATALOGS")
            .ok()
            .map(|spec| parse_catalogs(&spec))
            .filter(|catalogs| !catalogs.is_empty())
        {
            Some(catalogs) => catalogs,
            None => {
                vec![CatalogConfig {
                    id: "polaris".into(),
                    kind: "polaris".into(),
                    endpoint: std::env::var("POLARIS_ENDPOINT")
                        .unwrap_or_else(|_| "http://localhost:8181".into()),
                    catalog: Some(
                        std::env::var("POLARIS_CATALOG")
                            .unwrap_or_else(|_| "quickstart_catalog".into()),
                    ),
                    token: std::env::var("POLARIS_TOKEN").ok(),
                    credential: polaris_credential_from_env()?,
                }]
            }
        };

        let default_engine = std::env::var("ASTER_DEFAULT_ENGINE")
            .ok()
            .or_else(|| engines.first().map(|engine| engine.id.clone()));
        let default_catalog = std::env::var("ASTER_DEFAULT_CATALOG")
            .ok()
            .or_else(|| catalogs.first().map(|catalog| catalog.id.clone()));
        let catalog_bindings = std::env::var("ASTER_CATALOG_BINDINGS")
            .ok()
            .map(|spec| parse_catalog_bindings(&spec))
            .transpose()?
            .unwrap_or_default();

        let config = Self {
            bind: std::env::var("ASTER_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            engines,
            catalogs,
            catalog_bindings,
            default_engine,
            default_catalog,
        };
        config.validate_catalog_bindings()?;
        Ok(config)
    }

    pub fn validate_catalog_bindings(&self) -> Result<()> {
        for engine in &self.engines {
            if let Some(delegation) = &engine.delegation {
                if engine.kind != "trino"
                    || !engine.endpoint.starts_with("https://")
                    || !std::path::Path::new(&delegation.token_file).is_absolute()
                    || !std::path::Path::new(&delegation.ca_file).is_absolute()
                    || !self.catalog_bindings.iter().any(|binding| {
                        binding.engine == engine.id && binding.policy == BindingPolicy::Protected
                    })
                {
                    return Err(CoreError::Invalid(
                        "invalid Trino delegation binding".into(),
                    ));
                }
            }
        }
        let mut pairs = HashSet::new();
        for binding in &self.catalog_bindings {
            if binding.catalog.trim().is_empty()
                || binding.engine.trim().is_empty()
                || binding.native_catalog.trim().is_empty()
            {
                return Err(CoreError::Invalid("empty catalog binding field".into()));
            }
            if !self
                .catalogs
                .iter()
                .any(|catalog| catalog.id == binding.catalog)
                || !self
                    .engines
                    .iter()
                    .any(|engine| engine.id == binding.engine)
            {
                return Err(CoreError::Invalid(
                    "catalog binding references unknown instance".into(),
                ));
            }
            if !pairs.insert((&binding.catalog, &binding.engine)) {
                return Err(CoreError::Invalid(
                    "duplicate catalog-to-engine binding".into(),
                ));
            }
        }
        if let Some(first) = self.catalog_bindings.first() {
            if self
                .catalog_bindings
                .iter()
                .any(|binding| binding.policy != first.policy)
            {
                return Err(CoreError::Invalid(
                    "protected and unprotected bindings cannot share one server".into(),
                ));
            }
        }
        Ok(())
    }
}

fn field(value: Option<&&str>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// `POLARIS_CLIENT_ID` + `POLARIS_CLIENT_SECRET` as the `id:secret` pair the
/// catalog exchanges for a token. Only two absent variables select anonymous mode.
fn polaris_credential_from_env() -> Result<Option<String>> {
    use std::env::VarError::NotPresent;
    match (
        std::env::var("POLARIS_CLIENT_ID"),
        std::env::var("POLARIS_CLIENT_SECRET"),
    ) {
        (Err(NotPresent), Err(NotPresent)) => Ok(None),
        (Ok(id), Ok(secret)) if !id.is_empty() && !secret.is_empty() => {
            Ok(Some(format!("{id}:{secret}")))
        }
        _ => Err(CoreError::Invalid(
            "POLARIS_CLIENT_ID and POLARIS_CLIENT_SECRET must both be nonempty UTF-8 values or both be absent".into(),
        )),
    }
}

fn parse_engines(spec: &str) -> Result<Vec<EngineConfig>> {
    spec.split(',')
        .map(|entry| {
            let fields: Vec<&str> = entry.split(';').map(str::trim).collect();
            if !matches!(fields.len(), 3 | 4 | 6)
                || fields.iter().take(3).any(|value| value.is_empty())
            {
                return Err(CoreError::Invalid("invalid engine entry".into()));
            }
            let delegation = if fields.len() == 6 {
                if fields[1] != "trino"
                    || fields[4].is_empty()
                    || fields[5].is_empty()
                    || !fields[2].starts_with("https://")
                    || !std::path::Path::new(fields[4]).is_absolute()
                    || !std::path::Path::new(fields[5]).is_absolute()
                {
                    return Err(CoreError::Invalid("invalid Trino delegation entry".into()));
                }
                Some(TrinoDelegationConfig {
                    token_file: fields[4].into(),
                    ca_file: fields[5].into(),
                })
            } else {
                None
            };
            Ok(EngineConfig {
                id: fields[0].into(),
                kind: fields[1].into(),
                endpoint: fields[2].into(),
                routing_group: field(fields.get(3)),
                delegation,
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
                token: field(fields.get(4)),
                credential: field(fields.get(5)),
            })
        })
        .collect()
}

fn parse_catalog_bindings(spec: &str) -> Result<Vec<CatalogBindingConfig>> {
    if spec.trim().is_empty() {
        return Ok(Vec::new());
    }
    spec.split(',')
        .map(|entry| {
            let fields: Vec<&str> = entry.split(';').map(str::trim).collect();
            if fields.len() != 4 || fields.iter().any(|field| field.is_empty()) {
                return Err(CoreError::Invalid(
                    "malformed ASTER_CATALOG_BINDINGS entry".into(),
                ));
            }
            Ok(CatalogBindingConfig {
                catalog: fields[0].into(),
                engine: fields[1].into(),
                native_catalog: fields[2].into(),
                policy: match fields[3] {
                    "unprotected" => BindingPolicy::Unprotected,
                    "protected" => BindingPolicy::Protected,
                    _ => return Err(CoreError::Invalid("unknown catalog binding policy".into())),
                },
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
            parse_engines("adhoc;trino;http://gw:8080;adhoc, etl;trino;http://gw:8080;etl")
                .unwrap();
        assert_eq!(engines.len(), 2);
        assert_eq!(engines[0].id, "adhoc");
        assert_eq!(engines[0].endpoint, "http://gw:8080");
        assert_eq!(engines[0].routing_group.as_deref(), Some("adhoc"));
        assert_eq!(engines[1].routing_group.as_deref(), Some("etl"));
    }

    #[test]
    fn routes_are_optional_and_malformed_entries_refuse_the_pool() {
        let engines = parse_engines("solo;trino;http://localhost:8080").unwrap();
        assert_eq!(engines.len(), 1);
        assert_eq!(engines[0].routing_group, None);
        assert!(parse_engines("solo;trino;http://localhost:8080,,broken").is_err());
    }

    #[test]
    fn protected_trino_delegation_requires_https_absolute_files_and_binding() {
        let engines = parse_engines(
            "secure;trino;https://trino.example:8443;;/run/trino/token;/run/trino/ca.pem",
        )
        .unwrap();
        assert!(engines[0].delegation.is_some());
        for spec in [
            "secure;trino;http://trino.example:8080;;/run/trino/token;/run/trino/ca.pem",
            "secure;spark;https://trino.example:8443;;/run/trino/token;/run/trino/ca.pem",
            "secure;trino;https://trino.example:8443;;relative-token;/run/trino/ca.pem",
            "secure;trino;https://trino.example:8443;;/run/trino/token",
        ] {
            assert!(parse_engines(spec).is_err(), "accepted {spec}");
        }
        let mut config = AppConfig {
            bind: String::new(),
            engines,
            catalogs: vec![CatalogConfig {
                id: "lake".into(),
                kind: "polaris".into(),
                endpoint: "local".into(),
                catalog: None,
                token: None,
                credential: None,
            }],
            catalog_bindings: vec![],
            default_engine: None,
            default_catalog: None,
        };
        assert!(config.validate_catalog_bindings().is_err());
        config.catalog_bindings =
            parse_catalog_bindings("lake;secure;polaris;unprotected").unwrap();
        assert!(config.validate_catalog_bindings().is_err());
        config.catalog_bindings = parse_catalog_bindings("lake;secure;polaris;protected").unwrap();
        assert!(config.validate_catalog_bindings().is_ok());
    }

    #[test]
    fn parses_catalog_bindings_with_distinct_native_aliases() {
        let bindings = parse_catalog_bindings(
            "lake-a;trino-a;iceberg;unprotected, lake-a;spark-a;lakehouse;protected",
        )
        .unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].catalog, "lake-a");
        assert_eq!(bindings[0].engine, "trino-a");
        assert_eq!(bindings[0].native_catalog, "iceberg");
        assert_eq!(bindings[1].native_catalog, "lakehouse");
        assert_eq!(bindings[1].policy, BindingPolicy::Protected);
    }

    #[test]
    fn malformed_and_duplicate_catalog_bindings_fail_closed() {
        assert!(parse_catalog_bindings("lake-a;trino-a").is_err());
        assert!(parse_catalog_bindings("lake-a;trino-a;iceberg;").is_err());
        assert!(parse_catalog_bindings("lake-a;trino-a;iceberg;unsafe").is_err());
        let mut config = AppConfig {
            bind: String::new(),
            engines: vec![EngineConfig {
                id: "trino-a".into(),
                kind: "trino".into(),
                endpoint: "local".into(),
                routing_group: None,
                delegation: None,
            }],
            catalogs: vec![CatalogConfig {
                id: "lake-a".into(),
                kind: "polaris".into(),
                endpoint: "local".into(),
                catalog: None,
                token: None,
                credential: None,
            }],
            catalog_bindings: parse_catalog_bindings("lake-a;trino-a;iceberg;unprotected").unwrap(),
            default_engine: None,
            default_catalog: None,
        };
        assert!(config.validate_catalog_bindings().is_ok());
        config
            .catalog_bindings
            .push(config.catalog_bindings[0].clone());
        assert!(config.validate_catalog_bindings().is_err());
        config.catalog_bindings[1].catalog = "unknown".into();
        assert!(config.validate_catalog_bindings().is_err());
    }

    #[test]
    fn mixed_protected_and_unprotected_bindings_are_rejected_even_with_different_endpoints() {
        let mut config = AppConfig {
            bind: String::new(),
            engines: vec![
                EngineConfig {
                    id: "engine-a".into(),
                    kind: "trino".into(),
                    endpoint: "same".into(),
                    routing_group: None,
                    delegation: None,
                },
                EngineConfig {
                    id: "engine-b".into(),
                    kind: "trino".into(),
                    endpoint: "same".into(),
                    routing_group: None,
                    delegation: None,
                },
            ],
            catalogs: vec![CatalogConfig {
                id: "lake".into(),
                kind: "polaris".into(),
                endpoint: "local".into(),
                catalog: None,
                token: None,
                credential: None,
            }],
            catalog_bindings: parse_catalog_bindings(
                "lake;engine-a;lake_sql;protected,lake;engine-b;lake_sql;unprotected",
            )
            .unwrap(),
            default_engine: None,
            default_catalog: None,
        };
        assert!(config.validate_catalog_bindings().is_err());
        config.catalogs.push(CatalogConfig {
            id: "other".into(),
            kind: "polaris".into(),
            endpoint: "other".into(),
            catalog: None,
            token: None,
            credential: None,
        });
        config.catalog_bindings[1].catalog = "other".into();
        config.engines[1].endpoint = "different".into();
        assert!(config.validate_catalog_bindings().is_err());
        config.catalog_bindings[1].policy = BindingPolicy::Protected;
        assert!(config.validate_catalog_bindings().is_ok());
    }

    #[test]
    fn catalog_credentials_are_redacted_from_debug_output() {
        let catalog = CatalogConfig {
            id: "lake-a".into(),
            kind: "polaris".into(),
            endpoint: "http://local".into(),
            catalog: None,
            token: Some("token-secret".into()),
            credential: Some("credential-secret".into()),
        };
        let config = AppConfig {
            bind: String::new(),
            engines: vec![],
            catalogs: vec![catalog.clone()],
            catalog_bindings: vec![],
            default_engine: None,
            default_catalog: None,
        };
        for rendered in [format!("{catalog:?}"), format!("{config:?}")] {
            assert!(!rendered.contains("token-secret"), "token leaked in Debug");
            assert!(
                !rendered.contains("credential-secret"),
                "credential leaked in Debug"
            );
        }
    }
}
