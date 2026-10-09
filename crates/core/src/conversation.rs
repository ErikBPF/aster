//! Private notebook conversations, independent of their storage location.
use crate::{new_sid, CoreError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex};

/// Exact compiled artifact/object identity; never a business-name search.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSelection {
    pub path: String,
    pub sha256: String,
    pub object: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractDependency {
    pub selection: ContractSelection,
    pub manifest_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub helper: String,
    /// None is legacy, untracked history; Some([]) has no contract dependency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_dependencies: Option<Vec<ContractDependency>>,
    /// Catalogs whose observations were actually sent, independent of contracts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_dependencies: Option<Vec<String>>,
}

impl ChatMessage {
    /// Content plus retained source identities share the transcript capacity.
    pub fn context_bytes(&self) -> usize {
        self.content.len()
            + self
                .contract_dependencies
                .iter()
                .flatten()
                .map(|dependency| {
                    dependency.selection.path.len()
                        + dependency.selection.sha256.len()
                        + dependency.selection.object.len()
                        + dependency.manifest_sha256.len()
                })
                .sum::<usize>()
            + self
                .catalog_dependencies
                .iter()
                .flatten()
                .map(String::len)
                .sum::<usize>()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub revision: i64,
    pub messages: Vec<ChatMessage>,
}

impl Conversation {
    pub fn empty() -> Result<Self> {
        Ok(Self {
            id: new_sid()?,
            revision: 0,
            messages: vec![],
        })
    }

    pub fn with_exchange(
        &self,
        expected: i64,
        user: ChatMessage,
        assistant: ChatMessage,
    ) -> Result<Self> {
        if self.revision != expected {
            return Err(CoreError::Conflict(
                "conversation changed; reload before resending".into(),
            ));
        }
        if user.role != "user" || assistant.role != "assistant" {
            return Err(CoreError::Invalid("invalid conversation exchange".into()));
        }
        let mut next = self.clone();
        next.messages.extend([user, assistant]);
        if next.messages.len() > 200
            || next
                .messages
                .iter()
                .map(ChatMessage::context_bytes)
                .sum::<usize>()
                > 262_144
        {
            return Err(CoreError::Invalid(
                "conversation is full (200 messages or 256 KiB)".into(),
            ));
        }
        next.revision += 1;
        Ok(next)
    }
}

#[async_trait]
pub trait ConversationStore: Send + Sync {
    async fn get(&self, subject: &str, notebook: &str) -> Result<Conversation>;
    async fn append(
        &self,
        subject: &str,
        notebook: &str,
        expected: i64,
        user: ChatMessage,
        assistant: ChatMessage,
    ) -> Result<Conversation>;
}

#[derive(Default)]
pub struct InMemoryConversations {
    conversations: Mutex<HashMap<(String, String), Conversation>>,
}

#[async_trait]
impl ConversationStore for InMemoryConversations {
    async fn get(&self, subject: &str, notebook: &str) -> Result<Conversation> {
        let mut rows = self.conversations.lock().expect("conversation lock");
        let key = (subject.into(), notebook.into());
        if !rows.contains_key(&key) {
            rows.insert(key.clone(), Conversation::empty()?);
        }
        Ok(rows[&key].clone())
    }
    async fn append(
        &self,
        subject: &str,
        notebook: &str,
        expected: i64,
        user: ChatMessage,
        assistant: ChatMessage,
    ) -> Result<Conversation> {
        let mut rows = self.conversations.lock().expect("conversation lock");
        let key = (subject.into(), notebook.into());
        let current = rows
            .get(&key)
            .ok_or_else(|| CoreError::NotFound("conversation".into()))?;
        let next = current.with_exchange(expected, user, assistant)?;
        rows.insert(key, next.clone());
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(role: &str, content: String) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content,
            helper: "test".into(),
            contract_dependencies: Some(vec![]),
            catalog_dependencies: Some(vec![]),
        }
    }
    #[test]
    fn transcript_limits_refuse_overflow_without_mutating_history() {
        let mut conversation = Conversation::empty().unwrap();
        for _ in 0..100 {
            conversation = conversation
                .with_exchange(
                    conversation.revision,
                    message("user", "question".into()),
                    message("assistant", "answer".into()),
                )
                .unwrap();
        }
        assert_eq!(conversation.messages.len(), 200);
        assert!(conversation
            .with_exchange(
                100,
                message("user", "extra".into()),
                message("assistant", "extra".into())
            )
            .is_err());
        assert_eq!(conversation.messages.len(), 200);
        let at_limit = Conversation::empty()
            .unwrap()
            .with_exchange(
                0,
                message("user", "a".repeat(131_072)),
                message("assistant", "b".repeat(131_072)),
            )
            .unwrap();
        assert!(at_limit
            .with_exchange(
                1,
                message("user", "x".into()),
                message("assistant", "".into())
            )
            .is_err());
        assert_eq!(at_limit.revision, 1);
        let mut grounded = message("user", "x".into());
        grounded.catalog_dependencies = Some(vec!["x".repeat(262_144)]);
        assert!(Conversation::empty()
            .unwrap()
            .with_exchange(0, grounded, message("assistant", String::new()))
            .is_err());
    }
}
