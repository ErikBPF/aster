//! Admin-owned shared model registrations. Ordinary-user access is reserved for
//! V8b2, after current identity grants can be checked on every request.

use std::collections::HashMap;
use std::sync::Arc;

use aster_core::shared_models::{SharedModelGrantEvent, SharedModelGrants};
use aster_core::{
    authorize, Action, AdminSharedModelSummary, CoreError, EncryptedSharedModel, LlmConfig,
    Principal, Result, SharedModelGrant, SharedModelStore,
};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, Generate, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::{current_identity, ApiError, AppState};
#[cfg(test)]
use aster_core::InMemorySharedModels;

/// Exact approved origins. Paths may select a deployment's OpenAI-compatible
/// base path, but no unapproved host, scheme, or port may receive a token.
pub struct DestinationPolicy {
    origins: Vec<String>,
}

impl DestinationPolicy {
    pub fn parse(origins: &str) -> Result<Self> {
        let mut approved = Vec::new();
        for entry in origins
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            let url = reqwest::Url::parse(entry)
                .map_err(|_| CoreError::Invalid("invalid shared-model approved origin".into()))?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || !matches!(url.path(), "" | "/")
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(CoreError::Invalid(
                    "invalid shared-model approved origin".into(),
                ));
            }
            approved.push(url.origin().ascii_serialization());
        }
        approved.sort();
        approved.dedup();
        Ok(Self { origins: approved })
    }

    pub fn validate(&self, base_url: &str) -> Result<()> {
        if base_url.len() > 2048 {
            return Err(CoreError::Invalid("invalid shared-model base URL".into()));
        }
        let url = reqwest::Url::parse(base_url)
            .map_err(|_| CoreError::Invalid("invalid shared-model base URL".into()))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !self.origins.contains(&url.origin().ascii_serialization())
        {
            return Err(CoreError::Invalid(
                "shared-model destination is not approved".into(),
            ));
        }
        Ok(())
    }
}

/// A versioned keyset supplied through SecretStore. A row's version selects
/// its decryption key; the active key encrypts new and rotated rows.
pub struct Keyring {
    active: String,
    keys: HashMap<String, [u8; 32]>,
}

impl Keyring {
    pub fn parse(active: &str, secret_json: &str) -> Result<Self> {
        let encoded: HashMap<String, String> = serde_json::from_str(secret_json)
            .map_err(|_| CoreError::Invalid("invalid shared-model keyset".into()))?;
        let mut keys = HashMap::new();
        for (version, key) in encoded {
            if !valid_version(&version) {
                return Err(CoreError::Invalid(
                    "invalid shared-model key version".into(),
                ));
            }
            let decoded = BASE64
                .decode(key)
                .map_err(|_| CoreError::Invalid("invalid shared-model keyset".into()))?;
            let bytes: [u8; 32] = decoded
                .try_into()
                .map_err(|_| CoreError::Invalid("invalid shared-model keyset".into()))?;
            keys.insert(version, bytes);
        }
        if !valid_version(active) || !keys.contains_key(active) {
            return Err(CoreError::Invalid(
                "active shared-model key is missing".into(),
            ));
        }
        Ok(Self {
            active: active.to_string(),
            keys,
        })
    }

    fn encrypt(
        &self,
        summary: AdminSharedModelSummary,
        token: &str,
    ) -> Result<EncryptedSharedModel> {
        let key = self
            .keys
            .get(&self.active)
            .ok_or_else(|| CoreError::Storage("active shared-model key missing".into()))?;
        let cipher = XChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| CoreError::Storage("invalid shared-model key".into()))?;
        let nonce = XNonce::generate();
        let aad = model_aad(&summary)?;
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: token.as_bytes(),
                    aad: &aad,
                },
            )
            .map_err(|_| CoreError::Storage("shared-model encryption failed".into()))?;
        Ok(EncryptedSharedModel {
            summary,
            key_version: self.active.clone(),
            nonce: nonce.to_vec(),
            ciphertext,
        })
    }

    fn decrypt(&self, model: &EncryptedSharedModel) -> Result<String> {
        let key = self
            .keys
            .get(&model.key_version)
            .ok_or_else(|| CoreError::Storage("shared-model key version unavailable".into()))?;
        if model.nonce.len() != 24 {
            return Err(CoreError::Storage("invalid shared-model ciphertext".into()));
        }
        let cipher = XChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| CoreError::Storage("invalid shared-model key".into()))?;
        let nonce = XNonce::try_from(model.nonce.as_slice())
            .map_err(|_| CoreError::Storage("invalid shared-model ciphertext".into()))?;
        let aad = model_aad(&model.summary)?;
        let plaintext = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &model.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| CoreError::Storage("shared-model decryption failed".into()))?;
        String::from_utf8(plaintext)
            .map_err(|_| CoreError::Storage("invalid shared-model ciphertext".into()))
    }
}

fn model_aad(summary: &AdminSharedModelSummary) -> Result<Vec<u8>> {
    // A JSON tuple encodes field boundaries unambiguously. Replacing an
    // approved URL path or model in PostgreSQL must invalidate the token too.
    serde_json::to_vec(&(&summary.id, &summary.base_url, &summary.model))
        .map_err(|_| CoreError::Storage("invalid shared-model metadata".into()))
}

fn valid_version(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub struct SharedModels {
    store: Arc<dyn SharedModelStore>,
    policy: DestinationPolicy,
    keys: RwLock<Keyring>,
}

/// Ordinary-user view. Endpoint paths and credentials stay administrator-only.
pub(crate) struct PublicSharedModelSummary {
    pub id: String,
    pub model: String,
}

impl SharedModels {
    pub fn new(store: Arc<dyn SharedModelStore>, policy: DestinationPolicy, keys: Keyring) -> Self {
        Self {
            store,
            policy,
            keys: RwLock::new(keys),
        }
    }

    pub async fn verify_keys(&self) -> Result<()> {
        let keys = self.keys.read().await;
        for model in self.store.list().await? {
            self.validate_stored(&model)?;
            keys.decrypt(&model)?;
        }
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<AdminSharedModelSummary>> {
        self.store
            .list()
            .await?
            .into_iter()
            .map(|model| {
                self.validate_stored(&model)?;
                Ok(model.summary)
            })
            .collect()
    }

    pub(crate) async fn available_for_current(
        &self,
        current: &Principal,
    ) -> Result<Vec<PublicSharedModelSummary>> {
        let mut visible = Vec::new();
        for row in self.store.list().await? {
            // Refresh registration and grants together; delete/recreate of the
            // same ID must not pair the prior metadata with new permissions.
            let snapshot = match self.store.snapshot(&row.summary.id).await {
                Ok(snapshot) => snapshot,
                Err(CoreError::NotFound(_)) => continue,
                Err(error) => return Err(error),
            };
            self.validate_stored(&snapshot.encrypted)?;
            if snapshot
                .grants
                .grants
                .iter()
                .any(|grant| grant.allows(current))
            {
                visible.push(PublicSharedModelSummary {
                    id: snapshot.encrypted.summary.id,
                    model: snapshot.encrypted.summary.model,
                });
            }
        }
        Ok(visible)
    }

    /// Resolve token and grants from one registration incarnation. The caller
    /// supplies a freshly checked principal; stale session roles never enter.
    pub(crate) async fn authorized_config(
        &self,
        id: &str,
        current: &Principal,
    ) -> Result<LlmConfig> {
        if !aster_core::llm::valid_id(id) {
            return Err(CoreError::Invalid("invalid shared-model id".into()));
        }
        let snapshot = self.store.snapshot(id).await.map_err(|error| match error {
            CoreError::NotFound(_) => CoreError::Unauthorized("shared helper unavailable".into()),
            other => other,
        })?;
        self.validate_stored(&snapshot.encrypted)?;
        if !snapshot
            .grants
            .grants
            .iter()
            .any(|grant| grant.allows(current))
        {
            return Err(CoreError::Unauthorized("shared helper unavailable".into()));
        }
        let token = self.keys.read().await.decrypt(&snapshot.encrypted)?;
        Ok(LlmConfig {
            subject: current.subject.clone(),
            id: format!("shared/{id}"),
            base_url: snapshot.encrypted.summary.base_url,
            model: snapshot.encrypted.summary.model,
            api_key: token,
        })
    }

    pub async fn put(
        &self,
        id: &str,
        base_url: String,
        model: String,
        token: String,
    ) -> Result<AdminSharedModelSummary> {
        if !aster_core::llm::valid_id(id)
            || model.trim().is_empty()
            || model.len() > 256
            || model.chars().any(char::is_control)
            || token.is_empty()
            || token.len() > 4096
            || token.chars().any(char::is_control)
        {
            return Err(CoreError::Invalid(
                "invalid shared-model registration".into(),
            ));
        }
        self.policy.validate(&base_url)?;
        let summary = AdminSharedModelSummary {
            id: id.to_string(),
            base_url,
            model,
        };
        let keys = self.keys.read().await;
        let encrypted = keys.encrypt(summary.clone(), &token)?;
        self.store.put(encrypted).await?;
        Ok(summary)
    }

    pub async fn remove(&self, id: &str) -> Result<()> {
        if !aster_core::llm::valid_id(id) {
            return Err(CoreError::Invalid("invalid shared-model id".into()));
        }
        self.store.remove(id).await
    }

    pub async fn grants(&self, id: &str) -> Result<SharedModelGrants> {
        if !aster_core::llm::valid_id(id) {
            return Err(CoreError::Invalid("invalid shared-model id".into()));
        }
        self.store.grants(id).await
    }

    pub async fn replace_grants(
        &self,
        id: &str,
        expected_revision: i64,
        grants: Vec<SharedModelGrant>,
        actor: &str,
    ) -> Result<SharedModelGrants> {
        if !aster_core::llm::valid_id(id)
            || expected_revision < 0
            || grants.len() > 100
            || actor.is_empty()
            || actor.len() > 256
            || actor.chars().any(char::is_control)
            || grants.iter().any(|grant| match grant {
                SharedModelGrant::Group(group) => {
                    current_identity::uuid(group).as_deref() != Some(group.as_str())
                }
                SharedModelGrant::Role(_) => false,
            })
            || grants
                .iter()
                .enumerate()
                .any(|(index, grant)| grants[..index].contains(grant))
        {
            return Err(CoreError::Invalid("invalid shared-model grants".into()));
        }
        let now_ms: i64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CoreError::Storage("clock unavailable".into()))?
            .as_millis()
            .try_into()
            .map_err(|_| CoreError::Storage("clock unavailable".into()))?;
        self.store
            .replace_grants(id, expected_revision, grants, actor, now_ms)
            .await
    }

    pub async fn grant_events(&self, id: &str) -> Result<Vec<SharedModelGrantEvent>> {
        if !aster_core::llm::valid_id(id) {
            return Err(CoreError::Invalid("invalid shared-model id".into()));
        }
        self.store.grant_events(id).await
    }

    /// After deploying a keyset with a new active version and the old versions,
    /// rewrite every row atomically. Old keys can be removed on a later restart.
    pub async fn reencrypt_to_active(&self) -> Result<()> {
        let current = self.keys.write().await;
        let mut rotated = Vec::new();
        for model in self.store.list().await? {
            self.validate_stored(&model)?;
            let token = current.decrypt(&model)?;
            let next_model = current.encrypt(model.summary.clone(), &token)?;
            rotated.push((model, next_model));
        }
        self.store.replace_all(rotated).await?;
        Ok(())
    }

    fn validate_stored(&self, model: &EncryptedSharedModel) -> Result<()> {
        if !aster_core::llm::valid_id(&model.summary.id)
            || self.policy.validate(&model.summary.base_url).is_err()
        {
            return Err(CoreError::Storage(
                "invalid shared-model registry row".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct SharedModelBody {
    base_url: String,
    model: String,
    api_key: String,
}

#[derive(Deserialize)]
pub struct SharedModelGrantsBody {
    expected_revision: i64,
    grants: Vec<SharedModelGrant>,
}

fn enabled(state: &AppState) -> Result<&SharedModels> {
    state
        .shared_models
        .as_deref()
        .ok_or_else(|| CoreError::NotFound("shared-model registry is disabled".into()))
}

async fn admin(state: &AppState, headers: &HeaderMap) -> std::result::Result<Principal, ApiError> {
    let caller = current_identity::current_principal(state, headers, true).await?;
    authorize(&caller, Action::Administer)?;
    Ok(caller)
}

pub async fn list_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> std::result::Result<Json<Vec<AdminSharedModelSummary>>, ApiError> {
    admin(&state, &headers).await?;
    Ok(Json(enabled(&state)?.list().await?))
}

pub async fn put_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<SharedModelBody>,
) -> std::result::Result<Json<AdminSharedModelSummary>, ApiError> {
    admin(&state, &headers).await?;
    Ok(Json(
        enabled(&state)?
            .put(&id, body.base_url, body.model, body.api_key)
            .await?,
    ))
}

pub async fn remove_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> std::result::Result<StatusCode, ApiError> {
    admin(&state, &headers).await?;
    enabled(&state)?.remove(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_grants_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> std::result::Result<Json<SharedModelGrants>, ApiError> {
    admin(&state, &headers).await?;
    Ok(Json(enabled(&state)?.grants(&id).await?))
}

pub async fn replace_grants_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<SharedModelGrantsBody>,
) -> std::result::Result<StatusCode, ApiError> {
    let caller = admin(&state, &headers).await?;
    if let Some(authority) = state.current_identity.as_deref() {
        for grant in &body.grants {
            if let SharedModelGrant::Group(group_uuid) = grant {
                let exists = authority.group_exists(group_uuid).await.map_err(|_| {
                    CoreError::Unauthorized("current group directory unavailable".into())
                })?;
                if !exists {
                    return Err(CoreError::Invalid("shared group does not exist".into()).into());
                }
            }
        }
    }
    enabled(&state)?
        .replace_grants(&id, body.expected_revision, body.grants, &caller.subject)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn grant_events_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> std::result::Result<Json<Vec<SharedModelGrantEvent>>, ApiError> {
    admin(&state, &headers).await?;
    Ok(Json(enabled(&state)?.grant_events(&id).await?))
}

pub async fn reencrypt_admin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(_request): Json<serde_json::Value>,
) -> std::result::Result<StatusCode, ApiError> {
    admin(&state, &headers).await?;
    enabled(&state)?.reencrypt_to_active().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(active: &str, versions: &[(&str, u8)]) -> Keyring {
        let encoded: HashMap<_, _> = versions
            .iter()
            .map(|(version, byte)| ((*version).to_string(), BASE64.encode([*byte; 32])))
            .collect();
        Keyring::parse(active, &serde_json::to_string(&encoded).unwrap()).unwrap()
    }

    #[test]
    fn approved_origin_requires_exact_https_host_and_port() {
        let policy = DestinationPolicy::parse("https://models.example.invalid:8443").unwrap();
        assert!(policy
            .validate("https://models.example.invalid:8443/v1")
            .is_ok());
        for denied in [
            "http://models.example.invalid:8443/v1",
            "https://models.example.invalid/v1",
            "https://other.example.invalid:8443/v1",
            "https://user:secret@models.example.invalid:8443/v1",
            "https://models.example.invalid:8443/v1?token=secret",
            "https://models.example.invalid:8443/v1#secret",
        ] {
            assert!(policy.validate(denied).is_err(), "accepted {denied}");
        }
    }

    #[tokio::test]
    async fn token_is_encrypted_bound_to_id_and_can_be_reencrypted() {
        let store = Arc::new(InMemorySharedModels::default());
        let policy = DestinationPolicy::parse("https://models.example.invalid").unwrap();
        let registry = SharedModels::new(store.clone(), policy, keys("v1", &[("v1", 7)]));
        registry
            .put(
                "qwen",
                "https://models.example.invalid/v1".into(),
                "shared-qwen".into(),
                "shared-secret-token".into(),
            )
            .await
            .unwrap();
        let mut encrypted = store.list().await.unwrap().remove(0);
        assert_eq!(encrypted.key_version, "v1");
        assert!(!encrypted
            .ciphertext
            .windows("shared-secret-token".len())
            .any(|window| window == b"shared-secret-token"));
        assert!(registry.verify_keys().await.is_ok());

        let wrong = SharedModels::new(
            store.clone(),
            DestinationPolicy::parse("https://models.example.invalid").unwrap(),
            keys("v1", &[("v1", 8)]),
        );
        assert!(wrong.verify_keys().await.is_err());
        encrypted.summary.id = "other".into();
        assert!(keys("v1", &[("v1", 7)]).decrypt(&encrypted).is_err());
        encrypted.summary.id = "qwen".into();
        encrypted.summary.base_url = "https://models.example.invalid/other".into();
        assert!(keys("v1", &[("v1", 7)]).decrypt(&encrypted).is_err());
        encrypted.summary.base_url = "https://models.example.invalid/v1".into();
        encrypted.summary.model = "redirected-model".into();
        assert!(keys("v1", &[("v1", 7)]).decrypt(&encrypted).is_err());

        let rotating = SharedModels::new(
            store.clone(),
            DestinationPolicy::parse("https://models.example.invalid").unwrap(),
            keys("v2", &[("v1", 7), ("v2", 9)]),
        );
        rotating.verify_keys().await.unwrap();
        rotating.reencrypt_to_active().await.unwrap();
        assert_eq!(store.list().await.unwrap()[0].key_version, "v2");
        let reopened = SharedModels::new(
            store,
            DestinationPolicy::parse("https://models.example.invalid").unwrap(),
            keys("v2", &[("v2", 9)]),
        );
        assert!(reopened.verify_keys().await.is_ok());
    }

    #[tokio::test]
    async fn authorized_config_uses_one_registration_generation_and_current_grant() {
        let store = Arc::new(InMemorySharedModels::default());
        let registry = SharedModels::new(
            store.clone(),
            DestinationPolicy::parse("https://models.example.invalid").unwrap(),
            keys("v1", &[("v1", 7)]),
        );
        registry
            .put(
                "qwen",
                "https://models.example.invalid/v1".into(),
                "shared-qwen".into(),
                "first-token".into(),
            )
            .await
            .unwrap();
        registry
            .replace_grants(
                "qwen",
                0,
                vec![SharedModelGrant::Role(aster_core::Role::Editor)],
                "admin",
            )
            .await
            .unwrap();
        let current = Principal {
            subject: "alice".into(),
            roles: vec![aster_core::Role::Editor],
            groups: vec![],
            user_uuid: Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into()),
        };
        let first = registry.authorized_config("qwen", &current).await.unwrap();
        assert_eq!(first.model, "shared-qwen");
        assert_eq!(first.api_key, "first-token");
        assert_eq!(first.subject, "alice");

        store.remove("qwen").await.unwrap();
        registry
            .put(
                "qwen",
                "https://models.example.invalid/v1".into(),
                "new-qwen".into(),
                "second-token".into(),
            )
            .await
            .unwrap();
        assert!(matches!(
            registry.authorized_config("qwen", &current).await,
            Err(CoreError::Unauthorized(_))
        ));
    }
}
