use aster_core::{CoreError, Result, Role};
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
            .finish()
    }
}

impl OidcConfig {
    /// `None` when `ASTER_OIDC_ISSUER` is unset.
    pub fn from_env() -> Option<Self> {
        let issuer = std::env::var("ASTER_OIDC_ISSUER").ok()?;
        Some(Self {
            issuer,
            client_id: std::env::var("ASTER_OIDC_CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("ASTER_OIDC_CLIENT_SECRET").unwrap_or_default(),
            redirect_uri: std::env::var("ASTER_OIDC_REDIRECT_URI")
                .unwrap_or_else(|_| "http://localhost:8080/callback".into()),
            admin_group: std::env::var("ASTER_OIDC_ADMIN_GROUP")
                .unwrap_or_else(|_| "aster-admins".into()),
            editor_group: std::env::var("ASTER_OIDC_EDITOR_GROUP")
                .unwrap_or_else(|_| "aster-editors".into()),
        })
    }
}

/// Values that must survive the round trip to the IdP. They travel in a signed
/// cookie, so the server stays stateless between redirects.
pub struct Handshake {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub verifier: String,
}

pub struct Oidc {
    config: OidcConfig,
    metadata: RwLock<Option<CoreProviderMetadata>>,
    http: reqwest::Client,
}

impl Oidc {
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

    pub async fn handshake(&self) -> Result<Handshake> {
        let client = self.client().await?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state, nonce) = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new("openid".into()))
            .add_scope(Scope::new("groups".into()))
            .set_pkce_challenge(challenge)
            .url();
        Ok(Handshake {
            url: url.to_string(),
            state: state.secret().clone(),
            nonce: nonce.secret().clone(),
            verifier: verifier.secret().clone(),
        })
    }

    /// Exchanges the authorization code, validates the ID token, and derives the
    /// subject plus its aster roles from the group claim.
    pub async fn complete(
        &self,
        code: &str,
        verifier: &str,
        nonce: &str,
    ) -> Result<(String, Vec<Role>)> {
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

        let groups = groups_from_jwt(&id_token.to_string());
        Ok((
            claims.subject().to_string(),
            map_roles(&groups, &self.config.admin_group, &self.config.editor_group),
        ))
    }
}

/// Highest matching group wins; everyone else is a viewer.
pub fn map_roles(groups: &[String], admin_group: &str, editor_group: &str) -> Vec<Role> {
    if groups.iter().any(|group| group == admin_group) {
        vec![Role::Admin]
    } else if groups.iter().any(|group| group == editor_group) {
        vec![Role::Editor]
    } else {
        vec![Role::Viewer]
    }
}

/// Read the `groups` claim out of an ID token that `openidconnect` has already
/// validated. ponytail: the core client type erases custom claims
/// (`EmptyAdditionalClaims`); decode the verified payload rather than hand-roll
/// a second ID token type. Signature, issuer, audience and nonce are the
/// library's job, not this function's.
fn groups_from_jwt(jwt: &str) -> Vec<String> {
    let Some(payload) = jwt.split('.').nth(1) else {
        return Vec::new();
    };
    let bytes = B64
        .decode(payload)
        .or_else(|_| {
            base64::engine::general_purpose::URL_SAFE.decode(payload.trim_end_matches('='))
        })
        .unwrap_or_default();
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Vec::new();
    };
    value
        .get("groups")
        .and_then(|groups| groups.as_array())
        .map(|groups| {
            groups
                .iter()
                .filter_map(|group| group.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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
            groups_from_jwt(&jwt),
            vec!["aster-editors".to_string(), "x".to_string()]
        );
    }

    #[test]
    fn tolerates_tokens_without_groups() {
        assert!(groups_from_jwt(&jwt_with(r#"{"sub":"alice"}"#)).is_empty());
        assert!(groups_from_jwt("not-a-jwt").is_empty());
        assert!(groups_from_jwt("").is_empty());
    }
}
