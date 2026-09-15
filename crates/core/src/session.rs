use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::auth::Role;
use crate::error::{CoreError, Result};

type HmacSha256 = Hmac<Sha256>;

/// A signed-in user. Persisted nowhere: the client holds it as a signed cookie
/// and the server re-derives it on every request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub subject: String,
    pub roles: Vec<Role>,
    /// Unix seconds. After this the cookie is rejected.
    pub expires_at: i64,
}

/// Signs sessions (and short-lived OIDC handshake state) with one HMAC key.
/// ponytail: single process-local key from the environment; move to a rotated
/// key set when sessions must survive a key rotation without a re-login.
pub struct SessionStore {
    key: Vec<u8>,
}

impl SessionStore {
    pub fn new(key: impl Into<Vec<u8>>) -> Self {
        Self { key: key.into() }
    }

    pub fn encode(&self, session: &Session) -> Result<String> {
        let json = serde_json::to_vec(session).map_err(|e| CoreError::Invalid(e.to_string()))?;
        Ok(self.seal_bytes(&json))
    }

    /// Returns `None` for a forged, truncated or expired token.
    pub fn decode(&self, token: &str, now: i64) -> Option<Session> {
        let json = self.open_bytes(token)?;
        let session: Session = serde_json::from_slice(&json).ok()?;
        if session.expires_at <= now {
            return None;
        }
        Some(session)
    }

    /// Sign an arbitrary short-lived value, used for the OIDC state cookie.
    pub fn seal(&self, value: &str) -> String {
        self.seal_bytes(value.as_bytes())
    }

    pub fn open(&self, token: &str) -> Option<String> {
        String::from_utf8(self.open_bytes(token)?).ok()
    }

    fn seal_bytes(&self, payload: &[u8]) -> String {
        let encoded = B64.encode(payload);
        let signature = B64.encode(self.mac(&encoded));
        format!("{encoded}.{signature}")
    }

    fn open_bytes(&self, token: &str) -> Option<Vec<u8>> {
        let (encoded, signature) = token.split_once('.')?;
        let signature = B64.decode(signature).ok()?;
        let mut mac = HmacSha256::new_from_slice(&self.key).ok()?;
        mac.update(encoded.as_bytes());
        mac.verify_slice(&signature).ok()?;
        B64.decode(encoded).ok()
    }

    fn mac(&self, message: &str) -> Vec<u8> {
        let mut mac =
            HmacSha256::new_from_slice(&self.key).expect("HMAC accepts keys of any length");
        mac.update(message.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SessionStore {
        SessionStore::new("test-key")
    }

    fn session() -> Session {
        Session {
            subject: "alice".into(),
            roles: vec![Role::Editor],
            expires_at: 1_000,
        }
    }

    #[test]
    fn round_trips_a_live_session() {
        let token = store().encode(&session()).unwrap();
        assert_eq!(store().decode(&token, 999), Some(session()));
    }

    #[test]
    fn rejects_expired_sessions() {
        let token = store().encode(&session()).unwrap();
        assert_eq!(store().decode(&token, 1_000), None);
    }

    #[test]
    fn rejects_tampered_payload() {
        let token = store().encode(&session()).unwrap();
        let (payload, signature) = token.split_once('.').unwrap();
        let forged = format!("{}x.{signature}", &payload[..payload.len() - 1]);
        assert_eq!(store().decode(&forged, 999), None);
    }

    #[test]
    fn rejects_a_foreign_key() {
        let token = store().encode(&session()).unwrap();
        assert_eq!(SessionStore::new("other-key").decode(&token, 999), None);
    }

    #[test]
    fn seals_and_opens_handshake_state() {
        let sealed = store().seal("state-1:nonce-1");
        assert_eq!(store().open(&sealed).as_deref(), Some("state-1:nonce-1"));
        assert_eq!(store().open("state-1:nonce-1"), None);
    }
}
