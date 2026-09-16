use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// A caller's own OpenAI-compatible completion endpoint. The API key is stored
/// server-side and is never returned to a client.
#[derive(Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub subject: String,
    pub base_url: String,
    pub model: String,
    #[serde(skip_serializing)]
    pub api_key: String,
}

/// Redacted on purpose: `api_key` must never reach a log line.
impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmConfig")
            .field("subject", &self.subject)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

#[async_trait]
pub trait LlmStore: Send + Sync {
    async fn get(&self, subject: &str) -> Result<Option<LlmConfig>>;
    async fn put(&self, config: LlmConfig) -> Result<()>;
}

#[derive(Default)]
pub struct InMemoryLlm {
    configs: RwLock<HashMap<String, LlmConfig>>,
}

impl InMemoryLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl LlmStore for InMemoryLlm {
    async fn get(&self, subject: &str) -> Result<Option<LlmConfig>> {
        Ok(self.configs.read().expect("llm lock").get(subject).cloned())
    }

    async fn put(&self, config: LlmConfig) -> Result<()> {
        if config.base_url.trim().is_empty() || config.model.trim().is_empty() {
            return Err(CoreError::Invalid(
                "base_url and model are both required".into(),
            ));
        }
        self.configs
            .write()
            .expect("llm lock")
            .insert(config.subject.clone(), config);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(subject: &str) -> LlmConfig {
        LlmConfig {
            subject: subject.into(),
            base_url: "http://llm:11434/v1".into(),
            model: "qwen".into(),
            api_key: "secret-token".into(),
        }
    }

    #[tokio::test]
    async fn stores_a_configuration_per_subject() {
        let store = InMemoryLlm::new();
        assert!(store.get("alice").await.unwrap().is_none());
        store.put(config("alice")).await.unwrap();
        assert_eq!(store.get("alice").await.unwrap().unwrap().model, "qwen");
        assert!(store.get("bob").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_incomplete_configuration() {
        let store = InMemoryLlm::new();
        let mut incomplete = config("alice");
        incomplete.model = "  ".into();
        assert!(store.put(incomplete).await.is_err());
    }

    #[test]
    fn the_api_key_never_serializes() {
        let json = serde_json::to_string(&config("alice")).unwrap();
        assert!(!json.contains("secret-token"));
    }
}
