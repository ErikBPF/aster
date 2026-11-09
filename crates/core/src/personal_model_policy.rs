//! Administrator-owned capability grants for personal model registration and use.
use crate::{CoreError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonalModelPolicy {
    pub revision: i64,
    pub subjects: Vec<String>,
    pub teams: Vec<String>,
}

impl PersonalModelPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.subjects.len() + self.teams.len() > 1024
            || self.subjects.iter().chain(&self.teams).any(|value| {
                value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
            })
        {
            return Err(CoreError::Invalid("invalid personal-model grants".into()));
        }
        Ok(())
    }
}

#[async_trait]
pub trait PersonalModelPolicyStore: Send + Sync {
    async fn get(&self) -> Result<PersonalModelPolicy>;
    async fn replace(
        &self,
        expected_revision: i64,
        subjects: Vec<String>,
        teams: Vec<String>,
        actor: &str,
    ) -> Result<PersonalModelPolicy>;
}

#[derive(Default)]
pub struct InMemoryPersonalModelPolicy {
    policy: RwLock<PersonalModelPolicy>,
}

impl InMemoryPersonalModelPolicy {
    /// Explicit fixture/bootstrap grants; the default constructor grants nobody.
    pub fn with_subjects(subjects: &[&str]) -> Self {
        Self {
            policy: RwLock::new(PersonalModelPolicy {
                subjects: subjects.iter().map(|s| (*s).to_owned()).collect(),
                ..Default::default()
            }),
        }
    }
}

#[async_trait]
impl PersonalModelPolicyStore for InMemoryPersonalModelPolicy {
    async fn get(&self) -> Result<PersonalModelPolicy> {
        Ok(self
            .policy
            .read()
            .map_err(|_| CoreError::Storage("personal-model policy lock poisoned".into()))?
            .clone())
    }
    async fn replace(
        &self,
        expected_revision: i64,
        mut subjects: Vec<String>,
        mut teams: Vec<String>,
        _actor: &str,
    ) -> Result<PersonalModelPolicy> {
        subjects.sort();
        subjects.dedup();
        teams.sort();
        teams.dedup();
        let next = PersonalModelPolicy {
            revision: expected_revision
                .checked_add(1)
                .ok_or_else(|| CoreError::Invalid("invalid policy revision".into()))?,
            subjects,
            teams,
        };
        next.validate()?;
        let mut current = self
            .policy
            .write()
            .map_err(|_| CoreError::Storage("personal-model policy lock poisoned".into()))?;
        if current.revision != expected_revision {
            return Err(CoreError::Conflict("personal-model policy changed".into()));
        }
        *current = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn personal_model_grants_are_default_denied_and_revisioned() {
        let store = InMemoryPersonalModelPolicy::default();
        assert!(store.get().await.unwrap().subjects.is_empty());
        let grant = store
            .replace(0, vec!["alice".into()], vec![], "admin")
            .await
            .unwrap();
        assert_eq!(grant.revision, 1);
        assert_eq!(grant.subjects, vec!["alice"]);
        assert!(matches!(
            store.replace(0, vec!["bob".into()], vec![], "admin").await,
            Err(CoreError::Conflict(_))
        ));
        let revoked = store.replace(1, vec![], vec![], "admin").await.unwrap();
        assert_eq!(revoked.revision, 2);
        assert!(revoked.subjects.is_empty());
    }
}
