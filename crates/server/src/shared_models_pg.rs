//! PostgreSQL persistence for encrypted shared model registrations.

use aster_core::shared_models::{SharedModelGrantEvent, SharedModelGrants, SharedModelSnapshot};
use aster_core::{
    AdminSharedModelSummary, CoreError, EncryptedSharedModel, Result, SharedModelGrant,
    SharedModelStore,
};
use async_trait::async_trait;
use sqlx::PgPool;

pub struct PgSharedModels {
    pool: PgPool,
}

fn storage(error: sqlx::Error) -> CoreError {
    CoreError::Storage(error.to_string())
}

fn decode_grants(value: serde_json::Value) -> Result<Vec<SharedModelGrant>> {
    serde_json::from_value(value)
        .map_err(|_| CoreError::Storage("invalid stored shared-model grants".into()))
}

impl PgSharedModels {
    pub async fn new(pool: PgPool) -> Result<Self> {
        sqlx::raw_sql(include_str!("../../../migrations/0007_shared_models.sql"))
            .execute(&pool)
            .await
            .map_err(storage)?;
        sqlx::raw_sql(include_str!(
            "../../../migrations/0008_shared_model_grants.sql"
        ))
        .execute(&pool)
        .await
        .map_err(storage)?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl SharedModelStore for PgSharedModels {
    async fn list(&self) -> Result<Vec<EncryptedSharedModel>> {
        let rows: Vec<(String, String, String, String, Vec<u8>, Vec<u8>)> = sqlx::query_as(
            "SELECT id, base_url, model, key_version, nonce, ciphertext FROM shared_models ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        Ok(rows
            .into_iter()
            .map(
                |(id, base_url, model, key_version, nonce, ciphertext)| EncryptedSharedModel {
                    summary: AdminSharedModelSummary {
                        id,
                        base_url,
                        model,
                    },
                    key_version,
                    nonce,
                    ciphertext,
                },
            )
            .collect())
    }

    async fn snapshot(&self, id: &str) -> Result<SharedModelSnapshot> {
        let row: Option<(
            String,
            String,
            String,
            String,
            Vec<u8>,
            Vec<u8>,
            i64,
            i64,
            serde_json::Value,
        )> = sqlx::query_as(
            "SELECT id, base_url, model, key_version, nonce, ciphertext,
                    registration_generation, grant_revision, grants
             FROM shared_models WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let (id, base_url, model, key_version, nonce, ciphertext, generation, revision, grants) =
            row.ok_or_else(|| CoreError::NotFound("shared model is not registered".into()))?;
        Ok(SharedModelSnapshot {
            encrypted: EncryptedSharedModel {
                summary: AdminSharedModelSummary {
                    id,
                    base_url,
                    model,
                },
                key_version,
                nonce,
                ciphertext,
            },
            grants: SharedModelGrants {
                generation,
                revision,
                grants: decode_grants(grants)?,
            },
        })
    }

    async fn put(&self, model: EncryptedSharedModel) -> Result<()> {
        let changed = sqlx::query(
            "INSERT INTO shared_models (id, base_url, model, key_version, nonce, ciphertext)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET
               base_url = EXCLUDED.base_url,
               model = EXCLUDED.model,
               key_version = EXCLUDED.key_version,
               nonce = EXCLUDED.nonce,
               ciphertext = EXCLUDED.ciphertext,
               updated_at = now()
             WHERE shared_models.grant_revision = 0
                OR (shared_models.base_url = EXCLUDED.base_url
                    AND shared_models.model = EXCLUDED.model)",
        )
        .bind(&model.summary.id)
        .bind(&model.summary.base_url)
        .bind(&model.summary.model)
        .bind(&model.key_version)
        .bind(&model.nonce)
        .bind(&model.ciphertext)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        if changed.rows_affected() != 1 {
            return Err(CoreError::Conflict(
                "delete and recreate the shared model before changing its destination".into(),
            ));
        }
        Ok(())
    }

    async fn remove(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM shared_models WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn replace_all(
        &self,
        models: Vec<(EncryptedSharedModel, EncryptedSharedModel)>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query("LOCK TABLE shared_models IN EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM shared_models")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        if count != models.len() as i64 {
            return Err(CoreError::Conflict(
                "shared-model registry changed during key rotation".into(),
            ));
        }
        for (old, model) in models {
            if old.summary != model.summary {
                return Err(CoreError::Conflict(
                    "key rotation may not change shared-model metadata".into(),
                ));
            }
            let changed = sqlx::query(
                "UPDATE shared_models SET key_version = $2, nonce = $3, ciphertext = $4,
                 updated_at = now() WHERE id = $1 AND key_version = $5 AND nonce = $6
                 AND ciphertext = $7 AND base_url = $8 AND model = $9",
            )
            .bind(&model.summary.id)
            .bind(&model.key_version)
            .bind(&model.nonce)
            .bind(&model.ciphertext)
            .bind(&old.key_version)
            .bind(&old.nonce)
            .bind(&old.ciphertext)
            .bind(&old.summary.base_url)
            .bind(&old.summary.model)
            .execute(&mut *tx)
            .await
            .map_err(storage)?
            .rows_affected();
            if changed != 1 {
                return Err(CoreError::Conflict(
                    "shared-model registry changed during key rotation".into(),
                ));
            }
        }
        tx.commit().await.map_err(storage)?;
        Ok(())
    }

    async fn grants(&self, id: &str) -> Result<SharedModelGrants> {
        let row: Option<(i64, i64, serde_json::Value)> = sqlx::query_as(
            "SELECT registration_generation, grant_revision, grants FROM shared_models WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let (generation, revision, grants) =
            row.ok_or_else(|| CoreError::NotFound("shared model is not registered".into()))?;
        Ok(SharedModelGrants {
            generation,
            revision,
            grants: decode_grants(grants)?,
        })
    }

    async fn replace_grants(
        &self,
        id: &str,
        expected_revision: i64,
        grants: Vec<SharedModelGrant>,
        actor: &str,
        changed_at_ms: i64,
    ) -> Result<SharedModelGrants> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row: Option<(i64, i64, serde_json::Value)> = sqlx::query_as(
            "SELECT registration_generation, grant_revision, grants
             FROM shared_models WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let (generation, revision, previous) =
            row.ok_or_else(|| CoreError::NotFound("shared model is not registered".into()))?;
        if revision != expected_revision {
            return Err(CoreError::Conflict("shared-model grants changed".into()));
        }
        let next = revision
            .checked_add(1)
            .ok_or_else(|| CoreError::Invalid("grant revision exhausted".into()))?;
        let before = decode_grants(previous)?;
        let after_json = serde_json::to_value(&grants)
            .map_err(|_| CoreError::Invalid("invalid shared-model grants".into()))?;
        let before_json = serde_json::to_value(&before)
            .map_err(|_| CoreError::Storage("invalid stored shared-model grants".into()))?;
        sqlx::query("UPDATE shared_models SET grant_revision=$2, grants=$3 WHERE id=$1")
            .bind(id)
            .bind(next)
            .bind(&after_json)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        sqlx::query(
            "INSERT INTO shared_model_grant_events
             (model_id,registration_generation,revision,actor,before_grants,after_grants,changed_at_ms)
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(id)
        .bind(generation)
        .bind(next)
        .bind(actor)
        .bind(before_json)
        .bind(after_json)
        .bind(changed_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(SharedModelGrants {
            generation,
            revision: next,
            grants,
        })
    }

    async fn grant_events(&self, id: &str) -> Result<Vec<SharedModelGrantEvent>> {
        let rows: Vec<(
            String,
            i64,
            i64,
            String,
            serde_json::Value,
            serde_json::Value,
            i64,
        )> = sqlx::query_as(
            "SELECT model_id, registration_generation, revision, actor,
                    before_grants, after_grants, changed_at_ms
             FROM shared_model_grant_events WHERE model_id=$1 ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.into_iter()
            .map(
                |(model_id, generation, revision, actor, before, after, changed_at_ms)| {
                    Ok(SharedModelGrantEvent {
                        model_id,
                        generation,
                        revision,
                        actor,
                        before: decode_grants(before)?,
                        after: decode_grants(after)?,
                        changed_at_ms,
                    })
                },
            )
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aster_core::SharedModelStore;
    use base64::Engine;
    use serde_json::json;

    use super::*;
    use crate::shared_models::{DestinationPolicy, Keyring, SharedModels};

    fn keyring(active: &str, keys: &[(&str, u8)]) -> Keyring {
        let encoded = keys
            .iter()
            .map(|(version, byte)| {
                (
                    (*version).to_string(),
                    base64::engine::general_purpose::STANDARD.encode([*byte; 32]),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        Keyring::parse(active, &json!(encoded).to_string()).unwrap()
    }

    fn policy() -> DestinationPolicy {
        DestinationPolicy::parse("https://approved.example.invalid").unwrap()
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL; run tests/shared-model-postgres.sh"]
    async fn postgres_tokens_are_encrypted_restart_safe_and_rotation_is_atomic() {
        let url = std::env::var("ASTER_TEST_SHARED_MODEL_URL").unwrap();
        assert!(url.contains("127.0.0.1") && url.ends_with("/shared_models_test"));
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        let store = Arc::new(PgSharedModels::new(pool.clone()).await.unwrap());
        let v1 = SharedModels::new(store.clone(), policy(), keyring("v1", &[("v1", 7)]));
        v1.put(
            "qwen",
            "https://approved.example.invalid/v1".into(),
            "deepseek-v4.1".into(),
            "disposable-shared-token".into(),
        )
        .await
        .unwrap();
        let row: (String, Vec<u8>) =
            sqlx::query_as("SELECT key_version, ciphertext FROM shared_models WHERE id = 'qwen'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, "v1");
        assert!(!row
            .1
            .windows("disposable-shared-token".len())
            .any(|part| part == b"disposable-shared-token"));
        assert!(
            SharedModels::new(store.clone(), policy(), keyring("v1", &[("v1", 8)]))
                .verify_keys()
                .await
                .is_err()
        );
        assert!(
            SharedModels::new(store.clone(), policy(), keyring("v2", &[("v2", 9)]))
                .verify_keys()
                .await
                .is_err()
        );

        let v2 = SharedModels::new(
            store.clone(),
            policy(),
            keyring("v2", &[("v1", 7), ("v2", 9)]),
        );
        v2.verify_keys().await.unwrap();
        v2.reencrypt_to_active().await.unwrap();
        let initial_grants = store.grants("qwen").await.unwrap();
        assert_eq!(initial_grants.revision, 0);
        let analysts = SharedModelGrant::Group("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into());
        let granted = store
            .replace_grants("qwen", 0, vec![analysts.clone()], "root", 1000)
            .await
            .unwrap();
        assert_eq!(granted.revision, 1);
        assert!(store
            .replace_grants("qwen", 0, vec![], "other-admin", 1001)
            .await
            .is_err());
        v2.put(
            "qwen",
            "https://approved.example.invalid/v1".into(),
            "deepseek-v4.1".into(),
            "new-disposable-token".into(),
        )
        .await
        .unwrap();
        assert_eq!(store.grants("qwen").await.unwrap().grants, vec![analysts]);
        assert!(v2
            .put(
                "qwen",
                "https://approved.example.invalid/other".into(),
                "deepseek-v4.1".into(),
                "new-disposable-token".into(),
            )
            .await
            .is_err());
        assert_eq!(store.grant_events("qwen").await.unwrap().len(), 1);
        let rotated: (String, Vec<u8>) =
            sqlx::query_as("SELECT key_version, ciphertext FROM shared_models WHERE id = 'qwen'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rotated.0, "v2");
        assert_ne!(rotated.1, row.1);
        SharedModels::new(store.clone(), policy(), keyring("v2", &[("v2", 9)]))
            .verify_keys()
            .await
            .unwrap();

        let old = store.list().await.unwrap().remove(0);
        let mut next = old.clone();
        next.key_version = "v3".into();
        next.summary.base_url = "https://approved.example.invalid/other".into();
        assert!(store
            .replace_all(vec![(old.clone(), next.clone())])
            .await
            .is_err());
        next.summary = old.summary.clone();
        sqlx::query("UPDATE shared_models SET model = 'tampered-model' WHERE id = 'qwen'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(store.replace_all(vec![(old, next)]).await.is_err());
        let unchanged: String =
            sqlx::query_scalar("SELECT key_version FROM shared_models WHERE id = 'qwen'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unchanged, "v2");
        assert!(
            SharedModels::new(store.clone(), policy(), keyring("v2", &[("v2", 9)]))
                .verify_keys()
                .await
                .is_err()
        );
        v2.remove("qwen").await.unwrap();
        v2.put(
            "qwen",
            "https://approved.example.invalid/other".into(),
            "deepseek-v4.1".into(),
            "new-registration-token".into(),
        )
        .await
        .unwrap();
        let recreated = store.grants("qwen").await.unwrap();
        assert!(recreated.generation > initial_grants.generation);
        assert_eq!(recreated.revision, 0);
        assert!(recreated.grants.is_empty());
        let events = store.grant_events("qwen").await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].generation, initial_grants.generation);
        assert_eq!(events[0].actor, "root");
    }
}
