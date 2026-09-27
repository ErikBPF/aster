//! Session and one-shot handshake state.
//!
//! Sessions are opaque ids resolved in a store that every container shares
//! (D20): the client holds only the id, so any process can resolve, revoke and
//! list the same session, and a store outage fails closed. The store is the
//! source of truth for who is signed in — the cookie carries no claims.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::Role;
use crate::error::{CoreError, Result};
use crate::identity::Identity;

/// A signed-in user session as stored. Never sent to a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionRecord {
    pub sid: String,
    pub subject: String,
    pub roles: Vec<Role>,
    /// Verified identity-provider groups. Old sessions have none.
    #[serde(default)]
    pub groups: Vec<String>,
    /// Stable signed user UUID. Old sessions have none and cannot use shared models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_uuid: Option<String>,
    /// Set only after a verified identity-provider callback. Legacy and
    /// development sessions default to false, even if they carry old claims.
    #[serde(default)]
    pub verified: bool,
    pub created_at: i64,
    pub last_seen: i64,
    pub user_agent: Option<String>,
}

impl SessionRecord {
    /// A freshly created session: the subject is signed in at `now`.
    pub fn new(
        sid: String,
        subject: String,
        roles: Vec<Role>,
        groups: Vec<String>,
        now: i64,
        user_agent: Option<String>,
    ) -> Self {
        Self {
            sid,
            subject,
            roles,
            groups,
            user_uuid: None,
            verified: false,
            created_at: now,
            last_seen: now,
            user_agent,
        }
    }
}

#[async_trait]
pub trait SessionRegistry: Send + Sync {
    async fn create(
        &self,
        subject: &str,
        roles: Vec<Role>,
        groups: Vec<String>,
        user_agent: Option<String>,
        now: i64,
    ) -> Result<SessionRecord>;
    /// Persist a verified OIDC identity, including its stable UUID when the
    /// provider is configured to supply one. Development create stays UUID-free.
    async fn create_verified(
        &self,
        identity: &Identity,
        user_agent: Option<String>,
        now: i64,
    ) -> Result<SessionRecord>;
    /// `None` when the id is unknown or has been idle past the timeout.
    async fn get(&self, sid: &str, now: i64) -> Result<Option<SessionRecord>>;
    async fn revoke(&self, sid: &str) -> Result<()>;
    async fn list(&self, subject: &str) -> Result<Vec<SessionRecord>>;
}

/// Single-use OIDC handshake payload, keyed by the `state` parameter.
#[async_trait]
pub trait HandshakeStore: Send + Sync {
    async fn put(&self, state: &str, payload: &str, now: i64) -> Result<()>;
    /// Takes the payload out, so a replayed callback finds nothing.
    async fn take(&self, state: &str, now: i64) -> Result<Option<String>>;
}

/// Where a signed-in user left off. Deliberately small and opaque to the store:
/// the clients agree on the shape, the state plane only keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorkingState {
    /// Notebook the user last had open.
    pub notebook: Option<String>,
    /// Cell they were editing, so a reload lands in the same place.
    pub cell: Option<String>,
    /// Engine they last ran a cell on.
    pub engine: Option<String>,
    /// Legacy last-notebook choice, migrated once to a scoped helper selection.
    pub helper: Option<String>,
}

/// Per-subject working state, shared by every container (D18).
#[async_trait]
pub trait UserState: Send + Sync {
    async fn get(&self, subject: &str) -> Result<Option<WorkingState>>;
    async fn put(&self, subject: &str, state: &WorkingState) -> Result<()>;
    /// Notebook helper selection is separate from last-opened working state.
    async fn get_helper(&self, subject: &str, notebook: &str) -> Result<Option<String>>;
    async fn put_helper(&self, subject: &str, notebook: &str, helper: &str) -> Result<()>;
    /// One-time migration of the legacy single helper without replacing a new choice.
    async fn put_helper_if_absent(
        &self,
        subject: &str,
        notebook: &str,
        helper: &str,
    ) -> Result<String>;
}

/// 128 bits of randomness, hex encoded. The id is the whole credential, so it
/// must be unguessable rather than derived from a clock or a counter.
pub fn new_sid() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| CoreError::Storage(format!("no entropy: {error}")))?;
    let mut sid = String::with_capacity(32);
    for byte in bytes {
        sid.push_str(&format!("{byte:02x}"));
    }
    Ok(sid)
}

/// Process-local registry for tests and single-container development.
#[derive(Debug)]
pub struct InMemorySessions {
    ttl_seconds: i64,
    sessions: Mutex<HashMap<String, SessionRecord>>,
}

impl InMemorySessions {
    pub fn new(ttl_seconds: i64) -> Self {
        Self {
            ttl_seconds,
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl SessionRegistry for InMemorySessions {
    async fn create(
        &self,
        subject: &str,
        roles: Vec<Role>,
        groups: Vec<String>,
        user_agent: Option<String>,
        now: i64,
    ) -> Result<SessionRecord> {
        let record = SessionRecord {
            sid: new_sid()?,
            subject: subject.to_string(),
            roles,
            groups,
            user_uuid: None,
            verified: false,
            created_at: now,
            last_seen: now,
            user_agent,
        };
        self.sessions
            .lock()
            .map_err(|_| CoreError::Storage("session store poisoned".into()))?
            .insert(record.sid.clone(), record.clone());
        Ok(record)
    }

    async fn create_verified(
        &self,
        identity: &Identity,
        user_agent: Option<String>,
        now: i64,
    ) -> Result<SessionRecord> {
        let mut record = SessionRecord::new(
            new_sid()?,
            identity.subject.clone(),
            identity.roles.clone(),
            identity.groups.clone(),
            now,
            user_agent,
        );
        record.user_uuid = identity.user_uuid.clone();
        record.verified = true;
        self.sessions
            .lock()
            .map_err(|_| CoreError::Storage("session store poisoned".into()))?
            .insert(record.sid.clone(), record.clone());
        Ok(record)
    }

    async fn get(&self, sid: &str, now: i64) -> Result<Option<SessionRecord>> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| CoreError::Storage("session store poisoned".into()))?;
        if let Some(record) = sessions.get_mut(sid) {
            if record.last_seen + self.ttl_seconds <= now {
                sessions.remove(sid);
                return Ok(None);
            }
            record.last_seen = now;
            return Ok(Some(record.clone()));
        }
        Ok(None)
    }

    async fn revoke(&self, sid: &str) -> Result<()> {
        self.sessions
            .lock()
            .map_err(|_| CoreError::Storage("session store poisoned".into()))?
            .remove(sid);
        Ok(())
    }

    async fn list(&self, subject: &str) -> Result<Vec<SessionRecord>> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| CoreError::Storage("session store poisoned".into()))?;
        let mut records: Vec<SessionRecord> = sessions
            .values()
            .filter(|record| record.subject == subject)
            .cloned()
            .collect();
        records.sort_by_key(|record| record.created_at);
        Ok(records)
    }
}

/// Process-local handshake store. Entries expire on their own timeout and are
/// removed on redemption.
#[derive(Debug)]
pub struct InMemoryHandshakes {
    ttl_seconds: i64,
    pending: Mutex<HashMap<String, (String, i64)>>,
}

impl InMemoryHandshakes {
    pub fn new(ttl_seconds: i64) -> Self {
        Self {
            ttl_seconds,
            pending: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl HandshakeStore for InMemoryHandshakes {
    async fn put(&self, state: &str, payload: &str, now: i64) -> Result<()> {
        self.pending
            .lock()
            .map_err(|_| CoreError::Storage("handshake store poisoned".into()))?
            .insert(state.to_string(), (payload.to_string(), now));
        Ok(())
    }

    async fn take(&self, state: &str, now: i64) -> Result<Option<String>> {
        let entry = self
            .pending
            .lock()
            .map_err(|_| CoreError::Storage("handshake store poisoned".into()))?
            .remove(state);
        match entry {
            Some((payload, stored_at)) if stored_at + self.ttl_seconds > now => Ok(Some(payload)),
            _ => Ok(None),
        }
    }
}

/// Process-local working state for tests and single-container development.
#[derive(Debug, Default)]
pub struct InMemoryUserState {
    subjects: Mutex<HashMap<String, WorkingState>>,
    helpers: Mutex<HashMap<String, HashMap<String, String>>>,
}

impl InMemoryUserState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl UserState for InMemoryUserState {
    async fn get(&self, subject: &str) -> Result<Option<WorkingState>> {
        Ok(self
            .subjects
            .lock()
            .map_err(|_| CoreError::Storage("user state poisoned".into()))?
            .get(subject)
            .cloned())
    }

    async fn put(&self, subject: &str, state: &WorkingState) -> Result<()> {
        self.subjects
            .lock()
            .map_err(|_| CoreError::Storage("user state poisoned".into()))?
            .insert(subject.to_string(), state.clone());
        Ok(())
    }

    async fn get_helper(&self, subject: &str, notebook: &str) -> Result<Option<String>> {
        Ok(self
            .helpers
            .lock()
            .map_err(|_| CoreError::Storage("helper state poisoned".into()))?
            .get(subject)
            .and_then(|notebooks| notebooks.get(notebook))
            .cloned())
    }

    async fn put_helper(&self, subject: &str, notebook: &str, helper: &str) -> Result<()> {
        self.helpers
            .lock()
            .map_err(|_| CoreError::Storage("helper state poisoned".into()))?
            .entry(subject.to_string())
            .or_default()
            .insert(notebook.to_string(), helper.to_string());
        Ok(())
    }

    async fn put_helper_if_absent(
        &self,
        subject: &str,
        notebook: &str,
        helper: &str,
    ) -> Result<String> {
        Ok(self
            .helpers
            .lock()
            .map_err(|_| CoreError::Storage("helper state poisoned".into()))?
            .entry(subject.to_string())
            .or_default()
            .entry(notebook.to_string())
            .or_insert_with(|| helper.to_string())
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_random_hex() {
        let first = new_sid().unwrap();
        let second = new_sid().unwrap();
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn session_roundtrip_preserves_verified_groups_and_accepts_old_records() {
        let old = serde_json::json!({
            "sid":"old", "subject":"alice", "roles":["editor"],
            "created_at":1000, "last_seen":1000, "user_agent":null
        });
        let old_record: SessionRecord = serde_json::from_value(old).unwrap();
        assert_eq!(
            serde_json::to_value(&old_record).unwrap()["verified"],
            serde_json::json!(false),
            "legacy sessions must not claim an OIDC provenance"
        );
        assert_eq!(
            serde_json::to_value(old_record).unwrap()["groups"],
            serde_json::json!([]),
            "old sessions must default to no verified group grants"
        );

        let with_groups = serde_json::json!({
            "sid":"new", "subject":"alice", "roles":["editor"],
            "groups":["/org/analysts", "aster-editors"],
            "created_at":1000, "last_seen":1000, "user_agent":null
        });
        let record: SessionRecord = serde_json::from_value(with_groups).unwrap();
        assert_eq!(
            serde_json::to_value(record).unwrap()["groups"],
            serde_json::json!(["/org/analysts", "aster-editors"]),
            "verified group IDs must survive the session round trip"
        );
    }

    #[tokio::test]
    async fn only_verified_identity_creation_marks_a_session_verified() {
        let store = InMemorySessions::new(3600);
        let dev = store
            .create("alice", vec![Role::Admin], vec![], None, 1000)
            .await
            .unwrap();
        let oidc = store
            .create_verified(
                &Identity {
                    subject: "alice".into(),
                    roles: vec![Role::Admin],
                    groups: vec![],
                    user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
                },
                None,
                1000,
            )
            .await
            .unwrap();
        assert_eq!(serde_json::to_value(dev).unwrap()["verified"], false);
        assert_eq!(serde_json::to_value(oidc).unwrap()["verified"], true);
    }

    #[tokio::test]
    async fn create_get_revoke_and_list() {
        let store = InMemorySessions::new(3600);
        let record = store
            .create(
                "alice",
                vec![Role::Editor],
                vec!["analysts".into()],
                Some("tui".into()),
                1000,
            )
            .await
            .unwrap();
        assert_eq!(record.sid.len(), 32);

        let resolved = store.get(&record.sid, 1001).await.unwrap().unwrap();
        assert_eq!(resolved.subject, "alice");
        assert_eq!(resolved.groups, vec!["analysts"]);
        assert_eq!(resolved.last_seen, 1001);

        store
            .create("alice", vec![Role::Viewer], vec![], None, 1002)
            .await
            .unwrap();
        store
            .create("bob", vec![Role::Admin], vec![], None, 1003)
            .await
            .unwrap();
        assert_eq!(store.list("alice").await.unwrap().len(), 2);

        store.revoke(&record.sid).await.unwrap();
        assert!(store.get(&record.sid, 1004).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn idle_sessions_expire() {
        let store = InMemorySessions::new(60);
        let record = store
            .create("alice", vec![Role::Viewer], vec![], None, 1000)
            .await
            .unwrap();
        assert!(store.get(&record.sid, 1059).await.unwrap().is_some());
        // The read above refreshed the idle deadline to 1059 + 60, so a later
        // read finds the session gone.
        assert!(store.get(&record.sid, 1150).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn handshake_payloads_are_single_use() {
        let store = InMemoryHandshakes::new(300);
        store
            .put("state-1", "{\"verifier\":\"v\"}", 1000)
            .await
            .unwrap();
        assert_eq!(
            store.take("state-1", 1001).await.unwrap().as_deref(),
            Some("{\"verifier\":\"v\"}")
        );
        assert!(store.take("state-1", 1002).await.unwrap().is_none());

        store.put("state-2", "payload", 1000).await.unwrap();
        assert!(store.take("state-2", 1400).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn notebook_helpers_are_private_and_independent_of_last_opened_state() {
        let store = InMemoryUserState::new();
        let (sales, forecast) = tokio::join!(
            store.put_helper("alice", "sales", "alpha"),
            store.put_helper("alice", "forecast", "beta"),
        );
        sales.unwrap();
        forecast.unwrap();
        store.put_helper("bob", "sales", "beta").await.unwrap();
        store
            .put(
                "alice",
                &WorkingState {
                    notebook: Some("forecast".into()),
                    helper: Some("beta".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            store.get_helper("alice", "sales").await.unwrap().as_deref(),
            Some("alpha")
        );
        assert_eq!(
            store
                .get_helper("alice", "forecast")
                .await
                .unwrap()
                .as_deref(),
            Some("beta")
        );
        assert_eq!(
            store.get_helper("bob", "sales").await.unwrap().as_deref(),
            Some("beta")
        );
        assert_eq!(store.get_helper("bob", "forecast").await.unwrap(), None);
        assert_eq!(
            store
                .put_helper_if_absent("alice", "sales", "beta")
                .await
                .unwrap(),
            "alpha"
        );
    }
}
