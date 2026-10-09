//! OIDC implementation of the identity port.
//!
//! Works against any OpenID Connect provider; Authentik and Keycloak differ
//! only in the claim that carries group membership, which is configuration
//! (`ASTER_IDP_GROUPS_CLAIM`) rather than code.

use aster_core::{
    CoreError, Identity, IdentityHandshake, IdentityProvider, Result, Role, SecretStore,
};
use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse,
};
use tokio::sync::RwLock;

/// Type-state of a client that knows its authorization and token endpoints, as
/// returned by `from_provider_metadata`.
type ConfiguredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// OIDC relying-party settings. Present only when the process is pointed at an
/// identity provider; otherwise the dev identity seam is used.
#[derive(Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub admin_group: String,
    pub editor_group: String,
    /// Claim carrying group membership. Authentik emits `groups`; a Keycloak
    /// realm gets the same claim from its group-membership mapper.
    pub groups_claim: String,
    /// Optional claim mapped by the deployed provider to its stable user UUID.
    /// Aster does not assume OIDC `sub` is an Authentik API UUID.
    pub user_uuid_claim: Option<String>,
    /// Scopes to request. Keycloak rejects a scope the realm does not define,
    /// so this is deployment configuration rather than a constant.
    pub scopes: Vec<String>,
}

/// Redacted on purpose: `client_secret` must never reach a log line.
impl std::fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcConfig")
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("redirect_uri", &self.redirect_uri)
            .field("admin_group", &self.admin_group)
            .field("editor_group", &self.editor_group)
            .field("groups_claim", &self.groups_claim)
            .field("user_uuid_claim", &self.user_uuid_claim)
            .field("scopes", &self.scopes)
            .finish()
    }
}

impl OidcConfig {
    /// `None` when `ASTER_OIDC_ISSUER` is unset. Non-secret settings come from
    /// the environment; the client secret comes from the secret provider.
    pub async fn from_env(secrets: &dyn SecretStore) -> Option<Self> {
        let issuer = std::env::var("ASTER_OIDC_ISSUER").ok()?;
        Some(Self {
            issuer,
            client_id: std::env::var("ASTER_OIDC_CLIENT_ID").unwrap_or_default(),
            client_secret: secrets
                .get("ASTER_OIDC_CLIENT_SECRET")
                .await
                .ok()
                .flatten()
                .unwrap_or_default(),
            redirect_uri: std::env::var("ASTER_OIDC_REDIRECT_URI")
                .unwrap_or_else(|_| "http://localhost:8080/callback".into()),
            admin_group: std::env::var("ASTER_OIDC_ADMIN_GROUP")
                .unwrap_or_else(|_| "aster-admins".into()),
            editor_group: std::env::var("ASTER_OIDC_EDITOR_GROUP")
                .unwrap_or_else(|_| "aster-editors".into()),
            groups_claim: std::env::var("ASTER_IDP_GROUPS_CLAIM")
                .unwrap_or_else(|_| "groups".into()),
            user_uuid_claim: std::env::var("ASTER_IDP_USER_UUID_CLAIM")
                .ok()
                .filter(|claim| !claim.trim().is_empty()),
            scopes: std::env::var("ASTER_IDP_SCOPES")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|scope| !scope.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_else(|_| vec!["openid".to_string(), "groups".to_string()]),
        })
    }
}

pub struct OidcProvider {
    config: OidcConfig,
    metadata: RwLock<Option<CoreProviderMetadata>>,
    http: reqwest::Client,
}

impl OidcProvider {
    pub fn new(config: OidcConfig) -> Self {
        // ponytail: redirects are disabled because following them during
        // discovery or token exchange is an SSRF vector (oauth2 docs). The
        // timeout keeps an unreachable identity provider from hanging a login.
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("reqwest client builds");
        Self {
            config,
            metadata: RwLock::new(None),
            http,
        }
    }

    /// Discovery happens on first use and is cached, so the server still starts
    /// when the identity provider is temporarily unreachable.
    async fn client(&self) -> Result<ConfiguredClient> {
        // The guard must be dropped before taking the write lock below: a
        // temporary in a match scrutinee lives until the end of the match.
        let cached = self.metadata.read().await.clone();
        let metadata = match cached {
            Some(metadata) => metadata,
            None => {
                let mut guard = self.metadata.write().await;
                match guard.clone() {
                    Some(metadata) => metadata,
                    None => {
                        let discovered = CoreProviderMetadata::discover_async(
                            IssuerUrl::new(self.config.issuer.clone())
                                .map_err(|e| invalid(e.to_string()))?,
                            &self.http,
                        )
                        .await
                        .map_err(|e| {
                            CoreError::Unauthorized(format!("oidc discovery failed: {e}"))
                        })?;
                        *guard = Some(discovered.clone());
                        discovered
                    }
                }
            }
        };

        Ok(CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(
            RedirectUrl::new(self.config.redirect_uri.clone())
                .map_err(|e| invalid(e.to_string()))?,
        ))
    }
}

#[async_trait]
impl IdentityProvider for OidcProvider {
    fn kind(&self) -> &'static str {
        "oidc"
    }

    fn callback_uri(&self) -> Option<&str> {
        Some(&self.config.redirect_uri)
    }

    async fn begin(&self) -> Result<IdentityHandshake> {
        let client = self.client().await?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        for scope in &self.config.scopes {
            request = request.add_scope(Scope::new(scope.clone()));
        }
        let (url, state, nonce) = request.set_pkce_challenge(challenge).url();
        Ok(IdentityHandshake {
            url: url.to_string(),
            state: state.secret().clone(),
            nonce: nonce.secret().clone(),
            verifier: verifier.secret().clone(),
        })
    }

    /// Exchanges the authorization code, validates the ID token, and derives the
    /// subject plus its aster roles from the group claim.
    async fn complete(&self, code: &str, verifier: &str, nonce: &str) -> Result<Identity> {
        let client = self.client().await?;
        let response = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .map_err(|e| CoreError::Invalid(format!("token endpoint unavailable: {e}")))?
            .set_pkce_verifier(PkceCodeVerifier::new(verifier.to_string()))
            .request_async(&self.http)
            .await
            .map_err(|e| CoreError::Unauthorized(format!("token exchange failed: {e}")))?;

        let id_token = response
            .id_token()
            .ok_or_else(|| CoreError::Unauthorized("id token missing from response".into()))?;
        let claims = id_token
            .claims(&client.id_token_verifier(), &Nonce::new(nonce.to_string()))
            .map_err(|e| CoreError::Unauthorized(format!("id token rejected: {e}")))?;

        let groups = groups_from_jwt(&id_token.to_string(), &self.config.groups_claim);
        let user_uuid = self
            .config
            .user_uuid_claim
            .as_deref()
            .and_then(|claim| user_uuid_from_jwt(&id_token.to_string(), claim));
        Ok(Identity {
            subject: claims.subject().to_string(),
            roles: map_roles(&groups, &self.config.admin_group, &self.config.editor_group),
            groups,
            user_uuid,
        })
    }
}

/// Highest matching group wins; everyone else is a viewer.
///
/// A group is compared by exact name and by its trailing path segment, so a
/// Keycloak realm left at the default `full.path=true` emits `/aster-admins`
/// and still maps, without a second configuration knob.
pub fn map_roles(groups: &[String], admin_group: &str, editor_group: &str) -> Vec<Role> {
    let has = |wanted: &str| {
        groups
            .iter()
            .any(|group| group == wanted || group.ends_with(&format!("/{wanted}")))
    };
    if has(admin_group) {
        vec![Role::Admin]
    } else if has(editor_group) {
        vec![Role::Editor]
    } else {
        vec![Role::Viewer]
    }
}

/// Read a group claim out of an ID token that `openidconnect` has already
/// validated. ponytail: the core client type erases custom claims
/// (`EmptyAdditionalClaims`); decode the verified payload rather than hand-roll
/// a second ID token type. Signature, issuer, audience and nonce are the
/// library's job, not this function's.
fn groups_from_jwt(jwt: &str, claim: &str) -> Vec<String> {
    let Some(value) = verified_payload(jwt) else {
        return Vec::new();
    };
    value
        .get(claim)
        .and_then(|groups| groups.as_array())
        .map(|groups| {
            groups
                .iter()
                .filter_map(|group| group.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn user_uuid_from_jwt(jwt: &str, claim: &str) -> Option<String> {
    let value = verified_payload(jwt)?;
    let candidate = value.get(claim)?.as_str()?;
    let bytes = candidate.as_bytes();
    if bytes.len() != 36
        || bytes.iter().enumerate().any(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte != b'-'
            } else {
                !byte.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(candidate.to_ascii_lowercase())
}

fn verified_payload(jwt: &str) -> Option<serde_json::Value> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = B64
        .decode(payload)
        .or_else(|_| {
            base64::engine::general_purpose::URL_SAFE.decode(payload.trim_end_matches('='))
        })
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn invalid(message: String) -> CoreError {
    CoreError::Invalid(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt_with(payload: &str) -> String {
        let encoded = B64.encode(payload.as_bytes());
        format!("header.{encoded}.signature")
    }

    #[test]
    fn maps_groups_to_the_highest_role() {
        let cfg = ("aster-admins", "aster-editors");
        assert_eq!(map_roles(&[], cfg.0, cfg.1), vec![Role::Viewer]);
        assert_eq!(
            map_roles(&["other".into()], cfg.0, cfg.1),
            vec![Role::Viewer]
        );
        assert_eq!(
            map_roles(&["aster-editors".into()], cfg.0, cfg.1),
            vec![Role::Editor]
        );
        assert_eq!(
            map_roles(
                &["aster-editors".into(), "aster-admins".into()],
                cfg.0,
                cfg.1
            ),
            vec![Role::Admin]
        );
    }

    #[test]
    fn reads_groups_from_a_verified_payload() {
        let jwt = jwt_with(r#"{"sub":"alice","groups":["aster-editors","x"]}"#);
        assert_eq!(
            groups_from_jwt(&jwt, "groups"),
            vec!["aster-editors".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn reads_the_configured_claim_and_maps_full_paths() {
        let jwt = jwt_with(r#"{"sub":"alice","realm_access":["/aster-admins"]}"#);
        assert_eq!(
            groups_from_jwt(&jwt, "realm_access"),
            vec!["/aster-admins".to_string()]
        );
        let cfg = ("aster-admins", "aster-editors");
        assert_eq!(
            map_roles(&["/aster-editors".into()], cfg.0, cfg.1),
            vec![Role::Editor]
        );
        assert_eq!(
            map_roles(&["/aster-admins".into()], cfg.0, cfg.1),
            vec![Role::Admin]
        );
    }

    #[test]
    fn tolerates_tokens_without_groups() {
        assert!(groups_from_jwt(&jwt_with(r#"{"sub":"alice"}"#), "groups").is_empty());
        assert!(groups_from_jwt("not-a-jwt", "groups").is_empty());
        assert!(groups_from_jwt("", "groups").is_empty());
    }

    #[test]
    fn stable_user_uuid_requires_a_configured_valid_claim() {
        let jwt =
            jwt_with(r#"{"sub":"opaque-sub","user_uuid":"BBBBBBBB-BBBB-4BBB-8BBB-BBBBBBBBBBBB"}"#);
        assert_eq!(
            user_uuid_from_jwt(&jwt, "user_uuid").as_deref(),
            Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")
        );
        assert_eq!(user_uuid_from_jwt(&jwt, "other"), None);
        assert_eq!(
            user_uuid_from_jwt(&jwt_with(r#"{"user_uuid":"alice"}"#), "user_uuid"),
            None
        );
    }
}
