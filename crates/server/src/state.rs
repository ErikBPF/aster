//! Valkey-backed session and handshake state (D18/D20).
//!
//! The cookie carries an opaque id and nothing else; every container resolves it
//! here, so a session survives a restart and works behind more than one replica.
//! Keys are namespaced `aster:v1:<domain>:<entity>` and every write carries a
//! TTL, so an abandoned session expires without a sweeper.

use aster_core::{
    new_sid, CoreError, HandshakeStore, Result, Role, SessionRecord, SessionRegistry, UserState,
    WorkingState,
};
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;

/// Namespace prefix plus schema version. Bump `v1` when the shape changes.
const PREFIX: &str = "aster:v1";

fn storage(error: redis::RedisError) -> CoreError {
    CoreError::Storage(format!("state store: {error}"))
}

fn session_key(sid: &str) -> String {
    format!("{PREFIX}:session:{sid}")
}

fn subject_index(subject: &str) -> String {
    format!("{PREFIX}:sessions:{subject}")
}

fn handshake_key(state: &str) -> String {
    format!("{PREFIX}:handshake:{state}")
}

fn user_state_key(subject: &str) -> String {
    format!("{PREFIX}:user:{subject}")
}

async fn open_connection(url: &str) -> Result<MultiplexedConnection> {
    let client = redis::Client::open(url).map_err(storage)?;
    client
        .get_multiplexed_async_connection()
        .await
        .map_err(storage)
}

/// Sessions resolved from Valkey. The record itself is the source of truth, so
/// the host that wrote it does not matter.
#[derive(Clone)]
pub struct ValkeySessions {
    conn: MultiplexedConnection,
    ttl_seconds: i64,
}

/// Single-use OIDC handshake payloads resolved from Valkey.
#[derive(Clone)]
pub struct ValkeyHandshakes {
    conn: MultiplexedConnection,
    ttl_seconds: i64,
}

/// Per-subject working state resolved from Valkey.
#[derive(Clone)]
pub struct ValkeyUserState {
    conn: MultiplexedConnection,
    ttl_seconds: i64,
}

/// Opens one connection per store. One connection each keeps the three domains
/// independent; none is hot enough to need pooling beyond multiplexing.
pub async fn connect(
    url: &str,
    session_ttl_seconds: i64,
    handshake_ttl_seconds: i64,
    user_ttl_seconds: i64,
) -> Result<(ValkeySessions, ValkeyHandshakes, ValkeyUserState)> {
    let sessions = ValkeySessions {
        conn: open_connection(url).await?,
        ttl_seconds: session_ttl_seconds,
    };
    let handshakes = ValkeyHandshakes {
        conn: open_connection(url).await?,
        ttl_seconds: handshake_ttl_seconds,
    };
    let user_state = ValkeyUserState {
        conn: open_connection(url).await?,
        ttl_seconds: user_ttl_seconds,
    };
    Ok((sessions, handshakes, user_state))
}

#[async_trait::async_trait]
impl SessionRegistry for ValkeySessions {
    async fn create(
        &self,
        subject: &str,
        roles: Vec<Role>,
        user_agent: Option<String>,
        now: i64,
    ) -> Result<SessionRecord> {
        let record = SessionRecord::new(new_sid()?, subject.to_string(), roles, now, user_agent);
        let mut conn = self.conn.clone();
        let body = serde_json::to_string(&record)
            .map_err(|error| CoreError::Storage(format!("session encode: {error}")))?;
        let _: () = conn
            .set_ex(session_key(&record.sid), body, self.ttl_seconds as u64)
            .await
            .map_err(storage)?;
        let _: () = conn
            .sadd(subject_index(subject), &record.sid)
            .await
            .map_err(storage)?;
        let _: () = conn
            .expire(subject_index(subject), self.ttl_seconds)
            .await
            .map_err(storage)?;
        Ok(record)
    }

    async fn get(&self, sid: &str, now: i64) -> Result<Option<SessionRecord>> {
        let mut conn = self.conn.clone();
        let body: Option<String> = conn.get(session_key(sid)).await.map_err(storage)?;
        let Some(body) = body else {
            return Ok(None);
        };
        let mut record: SessionRecord = serde_json::from_str(&body)
            .map_err(|error| CoreError::Storage(format!("session decode: {error}")))?;
        if record.last_seen + self.ttl_seconds <= now {
            // The key TTL should already have removed it; this covers a stale
            // entry written by a longer-lived TTL. Drop the index entry too, or
            // the subject's set keeps naming sessions that no longer exist.
            let _: () = conn.del(session_key(sid)).await.map_err(storage)?;
            let _: () = conn
                .srem(subject_index(&record.subject), sid)
                .await
                .map_err(storage)?;
            return Ok(None);
        }
        record.last_seen = now;
        let refreshed = serde_json::to_string(&record)
            .map_err(|error| CoreError::Storage(format!("session encode: {error}")))?;
        let _: () = conn
            .set_ex(session_key(sid), refreshed, self.ttl_seconds as u64)
            .await
            .map_err(storage)?;
        let _: () = conn
            .expire(subject_index(&record.subject), self.ttl_seconds)
            .await
            .map_err(storage)?;
        Ok(Some(record))
    }

    async fn revoke(&self, sid: &str) -> Result<()> {
        let mut conn = self.conn.clone();
        let body: Option<String> = conn.get(session_key(sid)).await.map_err(storage)?;
        let _: () = conn.del(session_key(sid)).await.map_err(storage)?;
        if let Some(body) = body {
            if let Ok(record) = serde_json::from_str::<SessionRecord>(&body) {
                let _: () = conn
                    .srem(subject_index(&record.subject), sid)
                    .await
                    .map_err(storage)?;
            }
        }
        Ok(())
    }

    async fn list(&self, subject: &str) -> Result<Vec<SessionRecord>> {
        let mut conn = self.conn.clone();
        let sids: Vec<String> = conn
            .smembers(subject_index(subject))
            .await
            .map_err(storage)?;
        let mut records = Vec::new();
        for sid in sids {
            let body: Option<String> = conn.get(session_key(&sid)).await.map_err(storage)?;
            match body {
                Some(body) => match serde_json::from_str::<SessionRecord>(&body) {
                    Ok(record) => records.push(record),
                    Err(_) => {
                        // A record the index still names but we cannot read is
                        // stale; drop it rather than fail the listing.
                        let _: () = conn
                            .srem(subject_index(subject), &sid)
                            .await
                            .map_err(storage)?;
                    }
                },
                None => {
                    let _: () = conn
                        .srem(subject_index(subject), &sid)
                        .await
                        .map_err(storage)?;
                }
            }
        }
        records.sort_by_key(|record| record.created_at);
        Ok(records)
    }
}

/// Redeems a handshake payload exactly once. The read-and-delete runs as one
/// Lua script so two concurrent callbacks cannot both win it.
const TAKE_SCRIPT: &str = r"
local value = redis.call('GET', KEYS[1])
if value then
  redis.call('DEL', KEYS[1])
end
return value
";

#[async_trait::async_trait]
impl HandshakeStore for ValkeyHandshakes {
    async fn put(&self, state: &str, payload: &str, _now: i64) -> Result<()> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .set_ex(handshake_key(state), payload, self.ttl_seconds as u64)
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn take(&self, state: &str, _now: i64) -> Result<Option<String>> {
        let mut conn = self.conn.clone();
        let payload: Option<String> = redis::Script::new(TAKE_SCRIPT)
            .key(handshake_key(state))
            .invoke_async(&mut conn)
            .await
            .map_err(storage)?;
        Ok(payload)
    }
}

#[async_trait::async_trait]
impl UserState for ValkeyUserState {
    async fn get(&self, subject: &str) -> Result<Option<WorkingState>> {
        let mut conn = self.conn.clone();
        let body: Option<String> = conn.get(user_state_key(subject)).await.map_err(storage)?;
        let Some(body) = body else {
            return Ok(None);
        };
        let state: WorkingState = serde_json::from_str(&body)
            .map_err(|error| CoreError::Storage(format!("user state decode: {error}")))?;
        Ok(Some(state))
    }

    async fn put(&self, subject: &str, state: &WorkingState) -> Result<()> {
        let mut conn = self.conn.clone();
        let body = serde_json::to_string(state)
            .map_err(|error| CoreError::Storage(format!("user state encode: {error}")))?;
        let _: () = conn
            .set_ex(user_state_key(subject), body, self.ttl_seconds as u64)
            .await
            .map_err(storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_are_namespaced_and_versioned() {
        assert_eq!(session_key("abc"), "aster:v1:session:abc");
        assert_eq!(handshake_key("abc"), "aster:v1:handshake:abc");
        assert_eq!(subject_index("alice"), "aster:v1:sessions:alice");
        assert_eq!(user_state_key("alice"), "aster:v1:user:alice");
    }

    /// Proves the cross-container claim: two independent connections (what two
    /// replicas are) share one session and one handshake. Ignored by default
    /// because it needs a live Valkey; run with
    /// `cargo test -p aster-server -- --ignored` and `ASTER_TEST_STATE_URL` set.
    #[tokio::test]
    #[ignore = "needs a valkey reachable at ASTER_TEST_STATE_URL"]
    async fn sessions_are_shared_between_connections() {
        let url = std::env::var("ASTER_TEST_STATE_URL")
            .unwrap_or_else(|_| "redis://:aster-dev@127.0.0.1:6379".into());
        let (one, one_handshakes, one_user) =
            connect(&url, 3600, 300, 3600).await.expect("container one");
        let (two, two_handshakes, two_user) =
            connect(&url, 3600, 300, 3600).await.expect("container two");

        let record = one
            .create("alice", vec![Role::Editor], Some("test".into()), 1_000)
            .await
            .expect("create");
        let seen = two
            .get(&record.sid, 1_001)
            .await
            .expect("get")
            .expect("the other container sees the session");
        assert_eq!(seen.subject, "alice");
        assert_eq!(seen.roles, vec![Role::Editor]);

        one_handshakes
            .put("state-1", "verifier-1:nonce-1", 1_000)
            .await
            .expect("put");
        assert_eq!(
            two_handshakes.take("state-1", 1_001).await.expect("take"),
            Some("verifier-1:nonce-1".to_string())
        );
        assert_eq!(
            one_handshakes.take("state-1", 1_002).await.expect("replay"),
            None,
            "a redeemed handshake is gone for every container"
        );

        two.revoke(&record.sid).await.expect("revoke");
        assert_eq!(one.get(&record.sid, 1_003).await.expect("get"), None);
        assert!(one.list("alice").await.expect("list").is_empty());

        // An idle session that expires must also leave its index entry behind
        // it, or a subject's set grows with every session it ever created.
        let expiring = one
            .create("alice", vec![Role::Editor], None, 2_000)
            .await
            .expect("create");
        assert_eq!(
            one.get(&expiring.sid, 2_000 + 3_600).await.expect("get"),
            None
        );
        assert!(one.list("alice").await.expect("list").is_empty());

        let mut state = WorkingState::default();
        state.notebook = Some("sales".into());
        state.cell = Some("c2".into());
        state.engine = Some("trino-local".into());
        one_user.put("alice", &state).await.expect("put state");
        assert_eq!(
            two_user.get("alice").await.expect("get state"),
            Some(state),
            "the other container resumes where alice left off"
        );
    }
}
