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
    /// Anti-CSRF state parameter; also the handshake store key.
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
}

#[async_trait]
pub trait IdentityProvider: Send + Sync {
    /// Provider kind, for logs and the provider matrix.
    fn kind(&self) -> &'static str;

    /// Start an authorization-code flow with PKCE.
    async fn begin(&self) -> Result<IdentityHandshake>;

    /// Exchange the authorization code and verify the identity it proves.
    async fn complete(&self, code: &str, verifier: &str, nonce: &str) -> Result<Identity>;
}
