use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// A caller's own OpenAI-compatible completion endpoint. A subject may register
/// several, each under a short name, and pick one per notebook. The API key is
/// stored server-side and is never returned to a client.
#[derive(Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub subject: String,
    /// Short name the caller chose, unique per subject (for example `lab-qwen`).
    pub id: String,
    #[serde(skip_serializing)]
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
            .field("id", &self.id)
            .field("base_url", &"<redacted>")
            .field("model", &self.model)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

impl LlmConfig {
    /// Rejects an endpoint that could not be called and a name that could not be
    /// used in a path. One place, so every provider agrees on what is valid.
    pub fn validate(&self) -> Result<()> {
        if self.base_url.trim().is_empty() || self.model.trim().is_empty() {
            return Err(CoreError::Invalid(
                "base_url and model are both required".into(),
            ));
        }
        if !valid_id(&self.id) {
            return Err(CoreError::Invalid(
                "helper name must be 1-64 characters of letters, digits, dash or underscore".into(),
            ));
        }
        Ok(())
    }
}

/// The same shape the notebook store accepts: safe to put in a URL path.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[async_trait]
pub trait LlmStore: Send + Sync {
    /// Every helper the subject registered, in a stable order.
    async fn list(&self, subject: &str) -> Result<Vec<LlmConfig>>;
    async fn get(&self, subject: &str, id: &str) -> Result<Option<LlmConfig>>;
    async fn put(&self, config: LlmConfig) -> Result<()>;
    async fn remove(&self, subject: &str, id: &str) -> Result<()>;
}

#[derive(Default)]
pub struct InMemoryLlm {
    configs: RwLock<HashMap<String, HashMap<String, LlmConfig>>>,
}

impl InMemoryLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl LlmStore for InMemoryLlm {
    async fn list(&self, subject: &str) -> Result<Vec<LlmConfig>> {
        let mut configs: Vec<_> = self
            .configs
            .read()
            .expect("llm lock")
            .get(subject)
            .map(|by_id| by_id.values().cloned().collect())
            .unwrap_or_default();
        configs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(configs)
    }

    async fn get(&self, subject: &str, id: &str) -> Result<Option<LlmConfig>> {
        Ok(self
            .configs
            .read()
            .expect("llm lock")
            .get(subject)
            .and_then(|by_id| by_id.get(id))
            .cloned())
    }

    async fn put(&self, config: LlmConfig) -> Result<()> {
        config.validate()?;
        self.configs
            .write()
            .expect("llm lock")
            .entry(config.subject.clone())
            .or_default()
            .insert(config.id.clone(), config);
        Ok(())
    }

    async fn remove(&self, subject: &str, id: &str) -> Result<()> {
        if let Some(by_id) = self.configs.write().expect("llm lock").get_mut(subject) {
            by_id.remove(id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(subject: &str, id: &str) -> LlmConfig {
        LlmConfig {
            subject: subject.into(),
            id: id.into(),
            base_url: "http://llm:11434/v1".into(),
            model: "qwen".into(),
            api_key: "secret-token".into(),
        }
    }

    #[tokio::test]
    async fn stores_several_helpers_per_subject() {
        let store = InMemoryLlm::new();
        assert!(store.list("alice").await.unwrap().is_empty());
        store.put(config("alice", "lab-qwen")).await.unwrap();
        store.put(config("alice", "cloud")).await.unwrap();
        store.put(config("bob", "lab-qwen")).await.unwrap();

        let alice = store.list("alice").await.unwrap();
        assert_eq!(
            alice.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["cloud", "lab-qwen"]
        );
        assert_eq!(
            store.get("alice", "cloud").await.unwrap().unwrap().model,
            "qwen"
        );
        assert!(store.get("alice", "missing").await.unwrap().is_none());
        // Another subject's helper of the same name is not visible.
        assert!(store.get("bob", "cloud").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn removes_only_the_named_helper() {
        let store = InMemoryLlm::new();
        store.put(config("alice", "lab-qwen")).await.unwrap();
        store.put(config("alice", "cloud")).await.unwrap();
        store.remove("alice", "lab-qwen").await.unwrap();
        let ids: Vec<_> = store
            .list("alice")
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["cloud"]);
    }

    #[tokio::test]
    async fn rejects_incomplete_configuration_and_unsafe_names() {
        let store = InMemoryLlm::new();
        let mut incomplete = config("alice", "lab-qwen");
        incomplete.model = "  ".into();
        assert!(store.put(incomplete).await.is_err());

        let mut unsafe_name = config("alice", "../escape");
        unsafe_name.model = "qwen".into();
        assert!(store.put(unsafe_name).await.is_err());
    }

    #[test]
    fn the_api_key_never_serializes() {
        let json = serde_json::to_string(&config("alice", "lab-qwen")).unwrap();
        assert!(!json.contains("secret-token"));
    }

    #[test]
    fn legacy_url_credentials_never_serialize() {
        let mut legacy = config("alice", "legacy");
        legacy.base_url = "http://url-user:url-secret@example.invalid/v1".into();
        let json = serde_json::to_string(&legacy).unwrap();
        assert!(!json.contains("url-user"));
        assert!(!json.contains("url-secret"));
    }

    #[test]
    fn debug_does_not_expose_legacy_url_credentials() {
        let mut legacy = config("alice", "legacy");
        legacy.base_url = "http://url-user:url-secret@example.invalid/v1".into();
        let debug = format!("{legacy:?}");
        assert!(!debug.contains("url-user"));
        assert!(!debug.contains("url-secret"));
        assert!(!debug.contains("secret-token"));
    }
}
