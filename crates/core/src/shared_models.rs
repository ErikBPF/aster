//! Storage port for administrator-owned shared model registrations. Token
//! encryption and access policy live in the server; the port carries only
//! already-encrypted rows to providers.

use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use serde::Serialize;

use crate::{CoreError, Result, SharedModelGrant};

/// Administrator-only metadata. V8b2 needs a separate ordinary-user view
/// that does not expose the endpoint path.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct AdminSharedModelSummary {
    pub id: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Clone)]
pub struct EncryptedSharedModel {
    pub summary: AdminSharedModelSummary,
    pub key_version: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// One registration and its grants read under the same storage snapshot.
#[derive(Clone)]
pub struct SharedModelSnapshot {
    pub encrypted: EncryptedSharedModel,
    pub grants: SharedModelGrants,
}

#[derive(Clone, Serialize)]
pub struct SharedModelGrants {
    pub generation: i64,
    pub revision: i64,
    pub grants: Vec<SharedModelGrant>,
}

/// Append-only authorization change. Never stores a token or endpoint URL.
#[derive(Clone, Serialize)]
pub struct SharedModelGrantEvent {
    pub model_id: String,
    pub generation: i64,
    pub revision: i64,
    pub actor: String,
    pub before: Vec<SharedModelGrant>,
    pub after: Vec<SharedModelGrant>,
    pub changed_at_ms: i64,
}

#[async_trait]
pub trait SharedModelStore: Send + Sync {
    async fn list(&self) -> Result<Vec<EncryptedSharedModel>>;
    async fn snapshot(&self, id: &str) -> Result<SharedModelSnapshot>;
    async fn put(&self, model: EncryptedSharedModel) -> Result<()>;
    async fn remove(&self, id: &str) -> Result<()>;
    /// All-or-nothing replacement keeps a key rotation recoverable after a crash.
    async fn replace_all(
        &self,
        models: Vec<(EncryptedSharedModel, EncryptedSharedModel)>,
    ) -> Result<()>;
    async fn grants(&self, id: &str) -> Result<SharedModelGrants>;
    async fn replace_grants(
        &self,
        id: &str,
        expected_revision: i64,
        grants: Vec<SharedModelGrant>,
        actor: &str,
        changed_at_ms: i64,
    ) -> Result<SharedModelGrants>;
    async fn grant_events(&self, id: &str) -> Result<Vec<SharedModelGrantEvent>>;
}

#[derive(Default)]
pub struct InMemorySharedModels {
    registry: RwLock<MemoryRegistry>,
}

#[derive(Default)]
struct MemoryRegistry {
    models: BTreeMap<String, MemoryModel>,
    events: Vec<SharedModelGrantEvent>,
    next_generation: i64,
}

struct MemoryModel {
    encrypted: EncryptedSharedModel,
    grants: SharedModelGrants,
}

fn lock_error() -> CoreError {
    CoreError::Storage("shared-model lock poisoned".into())
}

#[async_trait]
impl SharedModelStore for InMemorySharedModels {
    async fn list(&self) -> Result<Vec<EncryptedSharedModel>> {
        Ok(self
            .registry
            .read()
            .map_err(|_| lock_error())?
            .models
            .values()
            .map(|row| row.encrypted.clone())
            .collect())
    }

    async fn snapshot(&self, id: &str) -> Result<SharedModelSnapshot> {
        let registry = self.registry.read().map_err(|_| lock_error())?;
        let row = registry
            .models
            .get(id)
            .ok_or_else(|| CoreError::NotFound("shared model is not registered".into()))?;
        Ok(SharedModelSnapshot {
            encrypted: row.encrypted.clone(),
            grants: row.grants.clone(),
        })
    }

    async fn put(&self, model: EncryptedSharedModel) -> Result<()> {
        let mut registry = self.registry.write().map_err(|_| lock_error())?;
        if let Some(row) = registry.models.get_mut(&model.summary.id) {
            if row.grants.revision > 0
                && (row.encrypted.summary.base_url != model.summary.base_url
                    || row.encrypted.summary.model != model.summary.model)
            {
                return Err(CoreError::Conflict(
                    "delete and recreate the shared model before changing its destination".into(),
                ));
            }
            row.encrypted = model;
        } else {
            registry.next_generation += 1;
            let generation = registry.next_generation;
            registry.models.insert(
                model.summary.id.clone(),
                MemoryModel {
                    encrypted: model,
                    grants: SharedModelGrants {
                        generation,
                        revision: 0,
                        grants: Vec::new(),
                    },
                },
            );
        }
        Ok(())
    }

    async fn remove(&self, id: &str) -> Result<()> {
        self.registry
            .write()
            .map_err(|_| lock_error())?
            .models
            .remove(id);
        Ok(())
    }

    async fn replace_all(
        &self,
        models: Vec<(EncryptedSharedModel, EncryptedSharedModel)>,
    ) -> Result<()> {
        let mut current = self.registry.write().map_err(|_| lock_error())?;
        if current.models.len() != models.len()
            || models.iter().any(|(old, new)| {
                if old.summary != new.summary {
                    return true;
                }
                current.models.get(&old.summary.id).is_none_or(|row| {
                    row.encrypted.summary.base_url != old.summary.base_url
                        || row.encrypted.summary.model != old.summary.model
                        || row.encrypted.key_version != old.key_version
                        || row.encrypted.nonce != old.nonce
                        || row.encrypted.ciphertext != old.ciphertext
                })
            })
        {
            return Err(CoreError::Conflict(
                "shared-model registry changed during key rotation".into(),
            ));
        }
        for (_, new) in models {
            let id = new.summary.id.clone();
            current.models.get_mut(&id).unwrap().encrypted = new;
        }
        Ok(())
    }

    async fn grants(&self, id: &str) -> Result<SharedModelGrants> {
        self.registry
            .read()
            .map_err(|_| lock_error())?
            .models
            .get(id)
            .map(|row| row.grants.clone())
            .ok_or_else(|| CoreError::NotFound("shared model is not registered".into()))
    }

    async fn replace_grants(
        &self,
        id: &str,
        expected_revision: i64,
        grants: Vec<SharedModelGrant>,
        actor: &str,
        changed_at_ms: i64,
    ) -> Result<SharedModelGrants> {
        let mut registry = self.registry.write().map_err(|_| lock_error())?;
        let row = registry
            .models
            .get_mut(id)
            .ok_or_else(|| CoreError::NotFound("shared model is not registered".into()))?;
        if row.grants.revision != expected_revision {
            return Err(CoreError::Conflict("shared-model grants changed".into()));
        }
        let before = row.grants.grants.clone();
        row.grants.revision = row
            .grants
            .revision
            .checked_add(1)
            .ok_or_else(|| CoreError::Invalid("grant revision exhausted".into()))?;
        row.grants.grants = grants.clone();
        let result = row.grants.clone();
        registry.events.push(SharedModelGrantEvent {
            model_id: id.to_string(),
            generation: result.generation,
            revision: result.revision,
            actor: actor.to_string(),
            before,
            after: grants,
            changed_at_ms,
        });
        Ok(result)
    }

    async fn grant_events(&self, id: &str) -> Result<Vec<SharedModelGrantEvent>> {
        Ok(self
            .registry
            .read()
            .map_err(|_| lock_error())?
            .events
            .iter()
            .filter(|event| event.model_id == id)
            .cloned()
            .collect())
    }
}
