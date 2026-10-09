//! Identity provider port.
//!
//! Authentication is a port like storage: the server asks an
//! [`IdentityProvider`] for an authorization URL and hands the returned code
//! back to it, so the concrete vendor (Authentik, Keycloak, or a future SAML or
//! LDAP adapter) is chosen in configuration and never named by the handlers.

use async_trait::async_trait;

use crate::{Result, Role};

/// Values the browser must carry through the identity provider and back. They
/// wait in the shared handshake store, keyed by the state parameter, so any
/// replica can finish the login (D20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityHandshake {
    /// Where to send the browser.
    pub url: String,
    /// Unpredictable, per-attempt anti-CSRF state; also the handshake store key.
    /// The callback must bind it to the initiating browser before redemption.
    pub state: String,
    /// OIDC nonce, echoed inside the ID token.
    pub nonce: String,
    /// PKCE code verifier, kept server-side.
    pub verifier: String,
}

/// The authenticated caller, before it becomes a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub subject: String,
    pub roles: Vec<Role>,
    /// Exact group claim strings from the verified OIDC token. Their stable
    /// grant identity must be proven against the current provider API.
    pub groups: Vec<String>,
    /// Stable provider user UUID from an explicitly configured signed claim.
    /// Missing until the deployed provider's claim mapping is verified.
    pub user_uuid: Option<String>,
}

/// One current identity read. Groups must use the same stable identifiers as
/// shared-model grants; callers must reject mismatched or incomplete records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentIdentity {
    pub user_uuid: String,
    pub active: bool,
    /// Derived from the same current groups by the authoritative adapter;
    /// never copied from a cached Aster session.
    pub roles: Vec<Role>,
    pub groups: Vec<String>,
}

#[async_trait]
pub trait CurrentIdentityProvider: Send + Sync {
    /// Read on every shared-model request; no cached allow decision.
    async fn current(&self, user_uuid: &str) -> Result<CurrentIdentity>;
    /// Check a grant target against the current provider directory, never a
    /// session claim or an administrator's own group membership.
    async fn group_exists(&self, group_uuid: &str) -> Result<bool>;
}

#[async_trait]
pub trait IdentityProvider: Send + Sync {
    /// Provider kind, for logs and the provider matrix.
    fn kind(&self) -> &'static str;

    /// Trusted relying-party callback configuration, never a request header.
    /// Unknown locations require secure browser cookies.
    fn callback_uri(&self) -> Option<&str> {
        None
    }

    /// Start an authorization-code flow with PKCE.
    async fn begin(&self) -> Result<IdentityHandshake>;

    /// Exchange the authorization code and verify the identity it proves.
    async fn complete(&self, code: &str, verifier: &str, nonce: &str) -> Result<Identity>;
}
