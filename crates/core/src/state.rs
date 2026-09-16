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

/// A signed-in user session as stored. Never sent to a client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionRecord {
    pub sid: String,
    pub subject: String,
    pub roles: Vec<Role>,
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
        now: i64,
        user_agent: Option<String>,
    ) -> Self {
        Self {
            sid,
            subject,
            roles,
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
        user_agent: Option<String>,
        now: i64,
    ) -> Result<SessionRecord> {
        let record = SessionRecord {
            sid: new_sid()?,
            subject: subject.to_string(),
            roles,
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

    #[tokio::test]
    async fn create_get_revoke_and_list() {
        let store = InMemorySessions::new(3600);
        let record = store
            .create("alice", vec![Role::Editor], Some("tui".into()), 1000)
            .await
            .unwrap();
        assert_eq!(record.sid.len(), 32);

        let resolved = store.get(&record.sid, 1001).await.unwrap().unwrap();
        assert_eq!(resolved.subject, "alice");
        assert_eq!(resolved.last_seen, 1001);

        store
            .create("alice", vec![Role::Viewer], None, 1002)
            .await
            .unwrap();
        store
            .create("bob", vec![Role::Admin], None, 1003)
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
            .create("alice", vec![Role::Viewer], None, 1000)
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
}
