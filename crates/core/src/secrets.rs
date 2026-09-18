//! Named secret resolution.
//!
//! Where a secret physically lives is a deployment decision: Kubernetes fills
//! the environment from an ExternalSecret backed by Vault/OpenBao, a local run
//! fills it from the shell. The process only asks a provider for a key, so the
//! Vault, file or environment choice is one implementation each.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::{CoreError, Result};

/// A provider of named secret values. Implementations must never log a value.
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// `None` when the key is not configured; an empty value counts as unset.
    async fn get(&self, key: &str) -> Result<Option<String>>;
}

/// The mandatory form: a value, or an error naming the key that is missing.
pub async fn require(store: &dyn SecretStore, key: &str) -> Result<String> {
    match store.get(key).await? {
        Some(value) if !value.is_empty() => Ok(value),
        _ => Err(CoreError::Invalid(format!("secret {key} is not set"))),
    }
}

/// The default provider: the process environment.
#[derive(Debug, Default)]
pub struct EnvSecrets;

#[async_trait]
impl SecretStore for EnvSecrets {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(std::env::var(key).ok().filter(|value| !value.is_empty()))
    }
}

/// Provider for tests and single-process runs.
#[derive(Debug, Default)]
pub struct InMemorySecrets {
    values: Mutex<HashMap<String, String>>,
}

impl InMemorySecrets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, key: &str, value: &str) {
        self.values
            .lock()
            .expect("secret lock")
            .insert(key.to_string(), value.to_string());
    }
}

#[async_trait]
impl SecretStore for InMemorySecrets {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let values = self
            .values
            .lock()
            .map_err(|_| CoreError::Storage("secret lock poisoned".into()))?;
        Ok(values.get(key).cloned().filter(|value| !value.is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_environment_is_the_default_provider() {
        std::env::set_var("ASTER_TEST_SECRET_KEY", "value-1");
        assert_eq!(
            EnvSecrets.get("ASTER_TEST_SECRET_KEY").await.unwrap(),
            Some("value-1".to_string())
        );
        assert_eq!(
            EnvSecrets.get("ASTER_TEST_SECRET_ABSENT").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn an_empty_value_counts_as_unset_and_require_names_the_key() {
        let secrets = InMemorySecrets::new();
        secrets.set("present", "value-1");
        secrets.set("empty", "");
        assert_eq!(
            secrets.get("present").await.unwrap().as_deref(),
            Some("value-1")
        );
        assert_eq!(secrets.get("empty").await.unwrap(), None);
        assert_eq!(secrets.get("absent").await.unwrap(), None);
        assert_eq!(require(&secrets, "present").await.unwrap(), "value-1");
        let error = require(&secrets, "absent").await.unwrap_err().to_string();
        assert!(error.contains("absent"), "{error}");
    }
}
