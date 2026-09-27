//! Provider selection.
//!
//! Every storage domain names its provider in configuration and one `match`
//! turns that name into an implementation. Adding a provider is a new arm plus
//! its module; an unknown name is refused at startup instead of failing on the
//! first request. The matrix in `docs/provider-matrix.md` is checked by
//! `tests/provider-matrix.sh`.

use std::sync::Arc;

use aster_core::{
    AuditSink, CoreError, EnvSecrets, Grants, HandshakeStore, IdentityProvider, InMemoryAudit,
    InMemoryGrants, InMemoryHandshakes, InMemoryLlm, InMemoryNotebookOwners, InMemorySecrets,
    InMemorySessions, InMemorySharedModels, InMemoryUserState, LlmStore, NotebookOwners,
    NotebookStore, Result, SecretStore, SessionRegistry, SharedModelStore, UserState,
};

use crate::gitstore::GitNotebookStore;
use crate::identity::OidcProvider;
use crate::state as valkey;
use crate::store::PgStore;

/// Provider name from the environment, falling back on the deployment default.
pub fn kind(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

/// `env` reads the process environment, which Kubernetes fills from an
/// ExternalSecret and a local run fills from the shell. A Vault/OpenBao or file
/// provider is a new arm here and a new `SecretStore` implementation.
pub fn secret_store(kind: &str) -> Result<Arc<dyn SecretStore>> {
    match kind {
        "env" => Ok(Arc::new(EnvSecrets)),
        "memory" => Ok(Arc::new(InMemorySecrets::new())),
        other => Err(CoreError::Invalid(format!(
            "unknown secret provider {other}: expected env or memory"
        ))),
    }
}

/// The metadata domains that share one database: engine grants, the query audit
/// trail and per-subject LLM endpoints.
pub struct Metadata {
    pub grants: Arc<dyn Grants>,
    pub audit: Arc<dyn AuditSink>,
    pub llm: Arc<dyn LlmStore>,
    pub conversations: Arc<dyn aster_core::ConversationStore>,
    pub notebook_owners: Arc<dyn NotebookOwners>,
    pub shared_models: Arc<dyn SharedModelStore>,
}

pub async fn metadata(
    kind: &str,
    url: Option<&str>,
    seeds: &[(String, String)],
) -> Result<Metadata> {
    match kind {
        "postgres" => {
            let url = url.ok_or_else(|| {
                CoreError::Invalid("metadata provider postgres needs a database url".into())
            })?;
            let store = Arc::new(PgStore::connect(url).await?);
            for (subject, engine) in seeds {
                store.seed_grant(subject, engine).await?;
            }
            Ok(Metadata {
                grants: store.clone(),
                audit: store.clone(),
                conversations: Arc::new(crate::conversations::PgConversations {
                    pool: store.pool.clone(),
                }),
                notebook_owners: store.clone(),
                shared_models: Arc::new(
                    crate::shared_models_pg::PgSharedModels::new(store.pool.clone()).await?,
                ),
                llm: store,
            })
        }
        "memory" => {
            tracing::warn!(
                "metadata provider memory: grants, audit and llm configs vanish on restart"
            );
            let grants = InMemoryGrants::new();
            for (subject, engine) in seeds {
                grants.grant(subject.clone(), engine.clone());
            }
            Ok(Metadata {
                grants: Arc::new(grants),
                audit: Arc::new(InMemoryAudit::new()),
                llm: Arc::new(InMemoryLlm::new()),
                conversations: Arc::new(aster_core::InMemoryConversations::default()),
                notebook_owners: Arc::new(InMemoryNotebookOwners::default()),
                shared_models: Arc::new(InMemorySharedModels::default()),
            })
        }
        other => Err(CoreError::Invalid(format!(
            "unknown metadata provider {other}: expected postgres or memory"
        ))),
    }
}

/// The shared state domains: sessions, one-time OIDC handshakes and per-subject
/// working state (D18/D20).
pub struct StateStores {
    pub sessions: Arc<dyn SessionRegistry>,
    pub handshakes: Arc<dyn HandshakeStore>,
    pub user_state: Arc<dyn UserState>,
}

pub async fn state(
    kind: &str,
    url: Option<&str>,
    session_ttl: i64,
    handshake_ttl: i64,
    user_ttl: i64,
) -> Result<StateStores> {
    match kind {
        "valkey" => {
            let url = url.ok_or_else(|| {
                CoreError::Invalid("state provider valkey needs a state url".into())
            })?;
            let (sessions, handshakes, user_state) =
                valkey::connect(url, session_ttl, handshake_ttl, user_ttl).await?;
            Ok(StateStores {
                sessions: Arc::new(sessions),
                handshakes: Arc::new(handshakes),
                user_state: Arc::new(user_state),
            })
        }
        "memory" => {
            tracing::warn!(
                "state provider memory: session state is per-process, so a second replica \
                 will not see it"
            );
            Ok(StateStores {
                sessions: Arc::new(InMemorySessions::new(session_ttl)),
                handshakes: Arc::new(InMemoryHandshakes::new(handshake_ttl)),
                user_state: Arc::new(InMemoryUserState::new()),
            })
        }
        other => Err(CoreError::Invalid(format!(
            "unknown state provider {other}: expected valkey or memory"
        ))),
    }
}

/// Notebook persistence. `git` is the only provider today; an object-store
/// provider is a new arm here plus a new `NotebookStore` implementation.
pub fn notebooks(kind: &str, dir: String, branch: String) -> Result<Arc<dyn NotebookStore>> {
    match kind {
        "git" => Ok(Arc::new(GitNotebookStore::open(dir, branch)?)),
        other => Err(CoreError::Invalid(format!(
            "unknown notebook provider {other}: expected git"
        ))),
    }
}

/// Authentication. `oidc` covers every OpenID Connect vendor (Authentik,
/// Keycloak, ...) through one implementation; `none` is the unconfigured dev
/// case and leaves the dev identity seam available.
pub fn identity(
    kind: &str,
    config: Option<crate::identity::OidcConfig>,
) -> Result<Option<Arc<dyn IdentityProvider>>> {
    match kind {
        "oidc" => {
            let config = config.ok_or_else(|| {
                CoreError::Invalid("identity provider oidc needs an issuer".into())
            })?;
            Ok(Some(Arc::new(OidcProvider::new(config))))
        }
        "none" => Ok(None),
        other => Err(CoreError::Invalid(format!(
            "unknown identity provider {other}: expected oidc or none"
        ))),
    }
}

/// Conversation storage defaults to the existing metadata store.
pub async fn conversations(
    kind: &str,
    url: Option<&str>,
    metadata: Arc<dyn aster_core::ConversationStore>,
) -> Result<Arc<dyn aster_core::ConversationStore>> {
    match kind {
        "metadata" => Ok(metadata),
        "memory" => Ok(Arc::new(aster_core::InMemoryConversations::default())),
        "postgres" => Ok(Arc::new(
            crate::conversations::PgConversations::connect(url.ok_or_else(|| {
                CoreError::Invalid("conversation provider postgres needs a database url".into())
            })?)
            .await?,
        )),
        other => Err(CoreError::Invalid(format!(
            "unknown conversation provider {other}: expected metadata, postgres or memory"
        ))),
    }
}

/// Session exchange storage is ephemeral; it never defaults to the metadata
/// store, which holds durable user data.
pub async fn exchanges(
    kind: &str,
    url: Option<&str>,
) -> Result<Arc<dyn aster_core::ExchangeStore>> {
    match kind {
        "memory" => Ok(Arc::new(aster_core::InMemoryExchanges::default())),
        "postgres" => Ok(Arc::new(
            crate::exchange::PgExchanges::connect(url.ok_or_else(|| {
                CoreError::Invalid("exchange provider postgres needs a database url".into())
            })?)
            .await?,
        )),
        other => Err(CoreError::Invalid(format!(
            "unknown exchange provider {other}: expected postgres or memory"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unknown_provider_is_refused_by_every_domain() {
        let Err(secrets) = secret_store("redis") else {
            panic!("expected a refusal");
        };
        assert!(secrets.to_string().contains("unknown secret provider"));

        let Err(metadata) = metadata("mysql", None, &[]).await else {
            panic!("expected a refusal");
        };
        assert!(metadata.to_string().contains("unknown metadata provider"));

        let Err(state) = state("etcd", None, 10, 10, 10).await else {
            panic!("expected a refusal");
        };
        assert!(state.to_string().contains("unknown state provider"));

        let Err(notebooks) = notebooks("s3", "dir".into(), "branch".into()) else {
            panic!("expected a refusal");
        };
        assert!(notebooks.to_string().contains("unknown notebook provider"));

        let Err(identity) = identity("ldap", None) else {
            panic!("expected a refusal");
        };
        assert!(identity.to_string().contains("unknown identity provider"));
    }

    #[tokio::test]
    async fn a_real_provider_without_its_endpoint_is_refused_by_name() {
        let Err(metadata) = metadata("postgres", None, &[]).await else {
            panic!("expected a refusal");
        };
        assert!(metadata
            .to_string()
            .contains("postgres needs a database url"));

        let Err(state) = state("valkey", None, 10, 10, 10).await else {
            panic!("expected a refusal");
        };
        assert!(state.to_string().contains("valkey needs a state url"));

        let Err(identity) = identity("oidc", None) else {
            panic!("expected a refusal");
        };
        assert!(identity.to_string().contains("oidc needs an issuer"));
    }
}
