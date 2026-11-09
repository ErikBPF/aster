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
    #[serde(default)]
    pub mcp_connections: Vec<String>,
    /// Exact selected skill revisions; omitted for legacy messages without skills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_dependencies: Option<Vec<crate::skills::SkillSelection>>,
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
            + self.mcp_connections.iter().map(String::len).sum::<usize>()
            + self
                .skill_dependencies
                .iter()
                .flatten()
                .map(|skill| skill.id.len() + 8)
                .sum::<usize>()
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
pub struct NotebookTool {
    pub name: String,
    pub connection: String,
    pub tool: String,
    pub definition: serde_json::Value,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct NotebookToolCall {
    pub connection: String,
    pub tool: String,
    pub arguments: String,
    pub call_id: String,
    pub confirmation: String,
}
impl std::fmt::Debug for NotebookToolCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotebookToolCall")
            .field("connection", &self.connection)
            .field("tool", &self.tool)
            .field("call_id", &self.call_id)
            .field("arguments", &"<redacted>")
            .field("confirmation", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolTurn {
    pub id: String,
    pub user: ChatMessage,
    pub messages: Vec<serde_json::Value>,
    pub tools: Vec<NotebookTool>,
    pub call: Option<NotebookToolCall>,
    pub phase: String,
    pub calls: usize,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    #[serde(default)]
    pub pending: Option<ToolTurn>,
    pub id: String,
    pub revision: i64,
    pub messages: Vec<ChatMessage>,
}

impl Conversation {
    pub fn empty() -> Result<Self> {
        Ok(Self {
            pending: None,
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
        if self.pending.is_some() {
            return Err(CoreError::Conflict(
                "resolve the pending tool turn first".into(),
            ));
        }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    #[serde(default)]
    pub revision: i64,
    pub phrase: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub message_count: usize,
}

/// A short, stable label for a session: the first user message, whitespace
/// collapsed and bounded. Sessions without a message use "New session".
pub fn conversation_phrase(text: &str) -> String {
    let phrase = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if phrase.is_empty() {
        return "New session".into();
    }
    let mut out = phrase.chars().take(60).collect::<String>();
    if phrase.chars().count() > 60 {
        out.push('…');
    }
    out
}

fn now_ms() -> i64 {
    static LAST: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    let candidate = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    let mut last = LAST.load(std::sync::atomic::Ordering::Relaxed);
    loop {
        let next = candidate.max(last + 1);
        match LAST.compare_exchange(
            last,
            next,
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
        ) {
            Ok(_) => return next,
            Err(actual) => last = actual,
        }
    }
}

#[async_trait]
pub trait ConversationStore: Send + Sync {
    /// Delete one exact idle session without recreating it or another session.
    async fn delete_session(
        &self,
        _subject: &str,
        _notebook: &str,
        _session: &str,
        _expected: i64,
    ) -> Result<()> {
        Err(CoreError::Storage(
            "conversation deletion unsupported".into(),
        ))
    }

    /// Atomically reserve or finish a tool turn before any external dispatch.
    async fn save_turn(
        &self,
        _subject: &str,
        _notebook: &str,
        _expected: i64,
        _next: Conversation,
    ) -> Result<Conversation> {
        Err(CoreError::Storage(
            "tool turns unsupported by conversation store".into(),
        ))
    }
    async fn get(&self, subject: &str, notebook: &str) -> Result<Conversation>;
    async fn append(
        &self,
        subject: &str,
        notebook: &str,
        expected: i64,
        user: ChatMessage,
        assistant: ChatMessage,
    ) -> Result<Conversation>;
    /// Sessions for one scope, newest first.
    async fn list(&self, _subject: &str, _notebook: &str) -> Result<Vec<ConversationSummary>> {
        Ok(Vec::new())
    }
    /// Create an empty session and return it.
    async fn create(&self, _subject: &str, _notebook: &str) -> Result<Conversation> {
        Err(CoreError::Storage(
            "conversation sessions unsupported".into(),
        ))
    }
    async fn get_session(
        &self,
        subject: &str,
        notebook: &str,
        session: &str,
    ) -> Result<Conversation> {
        let conversation = self.get(subject, notebook).await?;
        if conversation.id == session {
            Ok(conversation)
        } else {
            Err(CoreError::NotFound("conversation session".into()))
        }
    }
    async fn append_session(
        &self,
        subject: &str,
        notebook: &str,
        session: &str,
        expected: i64,
        user: ChatMessage,
        assistant: ChatMessage,
    ) -> Result<Conversation> {
        let current = self.get(subject, notebook).await?;
        if current.id != session {
            return Err(CoreError::Storage(
                "conversation store does not support appending to a named session".into(),
            ));
        }
        self.append(subject, notebook, expected, user, assistant)
            .await
    }
}

#[derive(Clone)]
struct StoredConversation {
    conversation: Conversation,
    phrase: String,
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl StoredConversation {
    fn new() -> Result<Self> {
        let now = now_ms();
        Ok(Self {
            conversation: Conversation::empty()?,
            phrase: "New session".into(),
            created_at_ms: now,
            updated_at_ms: now,
        })
    }

    fn summary(&self) -> ConversationSummary {
        ConversationSummary {
            id: self.conversation.id.clone(),
            revision: self.conversation.revision,
            phrase: self.phrase.clone(),
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            message_count: self.conversation.messages.len(),
        }
    }
}

#[derive(Default)]
pub struct InMemoryConversations {
    conversations: Mutex<HashMap<(String, String), Vec<StoredConversation>>>,
}

impl InMemoryConversations {
    fn with_scope<'a>(
        rows: &'a mut HashMap<(String, String), Vec<StoredConversation>>,
        subject: &str,
        notebook: &str,
    ) -> &'a mut Vec<StoredConversation> {
        let key = (subject.to_string(), notebook.to_string());
        if !rows.contains_key(&key) {
            rows.insert(key.clone(), Vec::new());
        }
        rows.get_mut(&key).expect("scope inserted")
    }

    fn latest(sessions: &[StoredConversation]) -> Option<&StoredConversation> {
        sessions.iter().max_by_key(|session| session.updated_at_ms)
    }

    fn find<'a>(
        sessions: &'a [StoredConversation],
        session: &str,
    ) -> Option<&'a StoredConversation> {
        sessions
            .iter()
            .find(|entry| entry.conversation.id == session)
    }
}

#[async_trait]
impl ConversationStore for InMemoryConversations {
    async fn delete_session(
        &self,
        subject: &str,
        notebook: &str,
        session: &str,
        expected: i64,
    ) -> Result<()> {
        let mut rows = self.conversations.lock().expect("conversation lock");
        let sessions = rows
            .get_mut(&(subject.to_string(), notebook.to_string()))
            .ok_or_else(|| CoreError::NotFound("conversation session".into()))?;
        let index = sessions
            .iter()
            .position(|entry| entry.conversation.id == session)
            .ok_or_else(|| CoreError::NotFound("conversation session".into()))?;
        let current = &sessions[index].conversation;
        if current.revision != expected {
            return Err(CoreError::Conflict(
                "conversation changed; reload before deleting".into(),
            ));
        }
        if current.pending.is_some() {
            return Err(CoreError::Conflict(
                "resolve the tool turn before deleting this session".into(),
            ));
        }
        sessions.remove(index);
        Ok(())
    }

    async fn save_turn(
        &self,
        subject: &str,
        notebook: &str,
        expected: i64,
        mut next: Conversation,
    ) -> Result<Conversation> {
        let mut rows = self.conversations.lock().expect("conversation lock");
        let entry = Self::with_scope(&mut rows, subject, notebook)
            .iter_mut()
            .find(|s| s.conversation.id == next.id)
            .ok_or_else(|| CoreError::NotFound("conversation session".into()))?;
        if entry.conversation.revision != expected {
            return Err(CoreError::Conflict(
                "conversation changed; reload before resuming".into(),
            ));
        }
        next.revision = expected
            .checked_add(1)
            .ok_or_else(|| CoreError::Conflict("conversation revision exhausted".into()))?;
        entry.phrase = conversation_phrase(
            next.messages
                .first()
                .map(|m| m.content.as_str())
                .or_else(|| next.pending.as_ref().map(|p| p.user.content.as_str()))
                .unwrap_or_default(),
        );
        entry.conversation = next.clone();
        entry.updated_at_ms = now_ms();
        Ok(next)
    }
    async fn get(&self, subject: &str, notebook: &str) -> Result<Conversation> {
        let mut rows = self.conversations.lock().expect("conversation lock");
        let sessions = Self::with_scope(&mut rows, subject, notebook);
        if sessions.is_empty() {
            sessions.push(StoredConversation::new()?);
        }
        Ok(Self::latest(sessions)
            .expect("latest session")
            .conversation
            .clone())
    }

    async fn list(&self, subject: &str, notebook: &str) -> Result<Vec<ConversationSummary>> {
        let rows = self.conversations.lock().expect("conversation lock");
        let key = (subject.to_string(), notebook.to_string());
        let mut summaries = rows
            .get(&key)
            .map(|sessions| {
                sessions
                    .iter()
                    .map(StoredConversation::summary)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.updated_at_ms));
        summaries.truncate(100);
        Ok(summaries)
    }

    async fn create(&self, subject: &str, notebook: &str) -> Result<Conversation> {
        let mut rows = self.conversations.lock().expect("conversation lock");
        let session = StoredConversation::new()?;
        let conversation = session.conversation.clone();
        Self::with_scope(&mut rows, subject, notebook).push(session);
        Ok(conversation)
    }

    async fn get_session(
        &self,
        subject: &str,
        notebook: &str,
        session: &str,
    ) -> Result<Conversation> {
        let rows = self.conversations.lock().expect("conversation lock");
        let key = (subject.to_string(), notebook.to_string());
        rows.get(&key)
            .and_then(|sessions| Self::find(sessions, session))
            .map(|entry| entry.conversation.clone())
            .ok_or_else(|| CoreError::NotFound("conversation session".into()))
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
        let sessions = Self::with_scope(&mut rows, subject, notebook);
        if sessions.is_empty() {
            sessions.push(StoredConversation::new()?);
        }
        let latest = sessions
            .iter()
            .enumerate()
            .max_by_key(|(_, session)| session.updated_at_ms)
            .map(|(index, _)| index)
            .expect("latest session");
        let session = &mut sessions[latest];
        let next = session
            .conversation
            .with_exchange(expected, user, assistant)?;
        if session.phrase == "New session" {
            session.phrase = conversation_phrase(
                next.messages
                    .first()
                    .map(|message| message.content.as_str())
                    .unwrap_or_default(),
            );
        }
        session.conversation = next.clone();
        session.updated_at_ms = now_ms();
        Ok(next)
    }

    async fn append_session(
        &self,
        subject: &str,
        notebook: &str,
        session: &str,
        expected: i64,
        user: ChatMessage,
        assistant: ChatMessage,
    ) -> Result<Conversation> {
        let mut rows = self.conversations.lock().expect("conversation lock");
        let sessions = Self::with_scope(&mut rows, subject, notebook);
        let entry = sessions
            .iter_mut()
            .find(|entry| entry.conversation.id == session)
            .ok_or_else(|| CoreError::NotFound("conversation session".into()))?;
        let next = entry
            .conversation
            .with_exchange(expected, user, assistant)?;
        if entry.phrase == "New session" {
            entry.phrase = conversation_phrase(
                next.messages
                    .first()
                    .map(|message| message.content.as_str())
                    .unwrap_or_default(),
            );
        }
        entry.conversation = next.clone();
        entry.updated_at_ms = now_ms();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tool_confirmation_debug_is_redacted() {
        let call = super::NotebookToolCall {
            connection: "connection".into(),
            tool: "tool".into(),
            arguments: "sensitive-argument".into(),
            call_id: "call".into(),
            confirmation: "secret-confirmation".into(),
        };
        let debug = format!("{call:?}");
        assert!(!debug.contains("secret-confirmation"));
        assert!(!debug.contains("sensitive-argument"));
    }

    use super::*;
    fn message(role: &str, content: String) -> ChatMessage {
        ChatMessage {
            mcp_connections: vec![],
            role: role.into(),
            content,
            helper: "test".into(),
            skill_dependencies: Some(vec![]),
            contract_dependencies: Some(vec![]),
            catalog_dependencies: Some(vec![]),
        }
    }
    #[test]
    fn session_phrase_collapses_and_bounds_text() {
        assert_eq!(conversation_phrase("  "), "New session");
        assert_eq!(
            conversation_phrase(" revenue \n by region "),
            "revenue by region"
        );
        let long = "x".repeat(80);
        let phrase = conversation_phrase(&long);
        assert_eq!(phrase.chars().count(), 61);
        assert!(phrase.ends_with('…'));
    }

    #[tokio::test]
    async fn sessions_are_latest_first_and_isolated() {
        let store = InMemoryConversations::default();
        let first = store.create("alice", "sales").await.unwrap();
        let second = store.create("alice", "sales").await.unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(store.list("alice", "sales").await.unwrap().len(), 2);
        assert_eq!(store.get("alice", "sales").await.unwrap().id, second.id);
        assert_eq!(
            store
                .get_session("alice", "sales", &first.id)
                .await
                .unwrap()
                .id,
            first.id
        );
        assert!(store.get_session("bob", "sales", &first.id).await.is_err());
        store
            .append_session(
                "alice",
                "sales",
                &first.id,
                0,
                message("user", "revenue by region".into()),
                message("assistant", "ok".into()),
            )
            .await
            .unwrap();
        let sessions = store.list("alice", "sales").await.unwrap();
        assert_eq!(sessions[0].id, first.id);
        assert_eq!(sessions[0].phrase, "revenue by region");
        assert_eq!(sessions[0].message_count, 2);
        assert_eq!(store.get("alice", "sales").await.unwrap().id, first.id);
        assert_eq!(store.list("alice", "ops").await.unwrap().len(), 0);
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
