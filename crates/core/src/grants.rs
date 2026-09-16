use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use async_trait::async_trait;

use crate::engine::EngineId;
use crate::error::{CoreError, Result};

/// Storage for subject -> engine grants. Implemented in memory for tests and
/// dev, and by the Postgres metadata store in the server.
#[async_trait]
pub trait Grants: Send + Sync {
    async fn allowed(&self, subject: &str, engine: &EngineId) -> Result<bool>;
}

/// Central check: a subject may run a query only on an engine it was granted.
pub async fn authorize_engine(grants: &dyn Grants, subject: &str, engine: &EngineId) -> Result<()> {
    if grants.allowed(subject, engine).await? {
        Ok(())
    } else {
        Err(CoreError::Unauthorized(format!(
            "{subject} is not granted access to engine {engine}"
        )))
    }
}

#[derive(Debug, Default)]
pub struct InMemoryGrants {
    map: RwLock<HashMap<String, HashSet<String>>>,
}

impl InMemoryGrants {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&self, subject: impl Into<String>, engine: impl Into<String>) {
        self.map
            .write()
            .unwrap()
            .entry(subject.into())
            .or_default()
            .insert(engine.into());
    }

    pub fn allowed_sync(&self, subject: &str, engine: &EngineId) -> bool {
        self.map
            .read()
            .unwrap()
            .get(subject)
            .is_some_and(|engines| engines.contains(&engine.0))
    }
}

#[async_trait]
impl Grants for InMemoryGrants {
    async fn allowed(&self, subject: &str, engine: &EngineId) -> Result<bool> {
        Ok(self.allowed_sync(subject, engine))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn grant_scopes_subject_to_engine() {
        let grants = InMemoryGrants::new();
        grants.grant("alice", "trino-a");

        let trino_a = EngineId::new("trino-a");
        let trino_b = EngineId::new("trino-b");

        assert!(grants.allowed_sync("alice", &trino_a));
        assert!(!grants.allowed_sync("alice", &trino_b));
        assert!(!grants.allowed_sync("bob", &trino_a));
        assert!(block_on(authorize_engine(&grants, "alice", &trino_a)).is_ok());
        assert!(block_on(authorize_engine(&grants, "alice", &trino_b)).is_err());
        assert!(block_on(authorize_engine(&grants, "bob", &trino_a)).is_err());
    }
}
