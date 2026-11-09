//! Persistent personal-model capability policy and encrypted credentials.
use crate::shared_models::Keyring;
use aster_core::{
    AdminSharedModelSummary, CoreError, EncryptedSharedModel, LlmConfig, LlmStore,
    PersonalModelPolicy, PersonalModelPolicyStore, Result,
};
use async_trait::async_trait;
use sqlx::PgPool;
use std::sync::Arc;

fn storage(error: impl std::fmt::Display) -> CoreError {
    CoreError::Storage(error.to_string())
}

pub struct PgPersonalModelPolicy {
    pool: PgPool,
}
impl PgPersonalModelPolicy {
    pub async fn new(pool: PgPool) -> Result<Self> {
        sqlx::raw_sql(include_str!("../../../migrations/0013_personal_models.sql"))
            .execute(&pool)
            .await
            .map_err(storage)?;
        Ok(Self { pool })
    }
}
#[async_trait]
impl PersonalModelPolicyStore for PgPersonalModelPolicy {
    async fn get(&self) -> Result<PersonalModelPolicy> {
        let (revision, subjects, teams): (i64, serde_json::Value, serde_json::Value) =
            sqlx::query_as(
                "SELECT revision, subjects, teams FROM personal_model_policy WHERE singleton=TRUE",
            )
            .fetch_one(&self.pool)
            .await
            .map_err(storage)?;
        Ok(PersonalModelPolicy {
            revision,
            subjects: serde_json::from_value(subjects).map_err(storage)?,
            teams: serde_json::from_value(teams).map_err(storage)?,
        })
    }
    async fn replace(
        &self,
        expected_revision: i64,
        mut subjects: Vec<String>,
        mut teams: Vec<String>,
        actor: &str,
    ) -> Result<PersonalModelPolicy> {
        subjects.sort();
        subjects.dedup();
        teams.sort();
        teams.dedup();
        let policy = PersonalModelPolicy {
            revision: expected_revision
                .checked_add(1)
                .ok_or_else(|| CoreError::Invalid("invalid policy revision".into()))?,
            subjects,
            teams,
        };
        policy.validate()?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let result=sqlx::query("UPDATE personal_model_policy SET revision=$1,subjects=$2,teams=$3 WHERE singleton=TRUE AND revision=$4")
            .bind(policy.revision).bind(serde_json::json!(policy.subjects)).bind(serde_json::json!(policy.teams)).bind(expected_revision).execute(&mut *tx).await.map_err(storage)?;
        if result.rows_affected() != 1 {
            return Err(CoreError::Conflict("personal-model policy changed".into()));
        }
        sqlx::query("INSERT INTO personal_model_policy_events(revision,actor,subjects,teams) VALUES($1,$2,$3,$4)").bind(policy.revision).bind(actor).bind(serde_json::json!(policy.subjects)).bind(serde_json::json!(policy.teams)).execute(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(policy)
    }
}

pub struct EncryptedPersonalModels {
    pool: PgPool,
    keys: Option<Arc<Keyring>>,
}

#[derive(sqlx::FromRow)]
struct PersonalRow {
    subject: String,
    id: String,
    base_url: String,
    model: String,
    api_key: String,
    key_version: Option<String>,
    nonce: Option<Vec<u8>>,
    ciphertext: Option<Vec<u8>>,
}
fn summary(config: &LlmConfig) -> AdminSharedModelSummary {
    AdminSharedModelSummary {
        // The namespace and tuple bind ciphertext to both its owner and name.
        id: serde_json::json!(["personal", config.subject, config.id]).to_string(),
        base_url: config.base_url.clone(),
        model: config.model.clone(),
    }
}
impl PersonalRow {
    fn config(self, keys: Option<&Keyring>) -> Result<LlmConfig> {
        let mut config = LlmConfig {
            subject: self.subject,
            id: self.id,
            base_url: self.base_url,
            model: self.model,
            api_key: String::new(),
        };
        if let Some(keys) = keys {
            let encrypted = EncryptedSharedModel {
                summary: summary(&config),
                key_version: self
                    .key_version
                    .ok_or_else(|| storage("personal model credential is not encrypted"))?,
                nonce: self
                    .nonce
                    .ok_or_else(|| storage("personal model nonce unavailable"))?,
                ciphertext: self
                    .ciphertext
                    .ok_or_else(|| storage("personal model ciphertext unavailable"))?,
            };
            config.api_key = keys.decrypt(&encrypted)?;
        }
        Ok(config)
    }
}
impl EncryptedPersonalModels {
    pub async fn new(pool: PgPool, keys: Option<Keyring>) -> Result<Self> {
        PgPersonalModelPolicy::new(pool.clone()).await?;
        if let Some(keys) = keys.as_ref() {
            let mut tx = pool.begin().await.map_err(storage)?;
            // Prevent concurrent legacy writes while the whole migration is atomic.
            sqlx::query("LOCK TABLE llm_helpers IN EXCLUSIVE MODE")
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            let rows=sqlx::query_as::<_,PersonalRow>("SELECT subject,id,base_url,model,api_key,key_version,nonce,ciphertext FROM llm_helpers").fetch_all(&mut *tx).await.map_err(storage)?;
            for row in rows {
                if row.ciphertext.is_some() || row.key_version.is_some() || row.nonce.is_some() {
                    if !row.api_key.is_empty() {
                        return Err(storage("ambiguous personal model credentials"));
                    }
                    row.config(Some(keys))?;
                } else {
                    let config = LlmConfig {
                        subject: row.subject,
                        id: row.id,
                        base_url: row.base_url,
                        model: row.model,
                        api_key: row.api_key,
                    };
                    let encrypted = keys.encrypt(summary(&config), &config.api_key)?;
                    sqlx::query("UPDATE llm_helpers SET api_key='',key_version=$1,nonce=$2,ciphertext=$3 WHERE subject=$4 AND id=$5")
                        .bind(encrypted.key_version).bind(encrypted.nonce).bind(encrypted.ciphertext).bind(config.subject).bind(config.id).execute(&mut *tx).await.map_err(storage)?;
                }
            }
            tx.commit().await.map_err(storage)?;
        }
        Ok(Self {
            pool,
            keys: keys.map(Arc::new),
        })
    }
}

#[async_trait]
impl LlmStore for EncryptedPersonalModels {
    async fn list(&self, subject: &str) -> Result<Vec<LlmConfig>> {
        // Plaintext is never selected outside the explicit startup migration.
        sqlx::query_as::<_,PersonalRow>("SELECT subject,id,base_url,model,''::text AS api_key,key_version,nonce,ciphertext FROM llm_helpers WHERE subject=$1 ORDER BY id")
            .bind(subject).fetch_all(&self.pool).await.map_err(storage)?.into_iter().map(|row|row.config(self.keys.as_deref())).collect()
    }
    async fn get(&self, subject: &str, id: &str) -> Result<Option<LlmConfig>> {
        sqlx::query_as::<_,PersonalRow>("SELECT subject,id,base_url,model,''::text AS api_key,key_version,nonce,ciphertext FROM llm_helpers WHERE subject=$1 AND id=$2")
            .bind(subject).bind(id).fetch_optional(&self.pool).await.map_err(storage)?.map(|row|row.config(self.keys.as_deref())).transpose()
    }
    async fn put(&self, config: LlmConfig) -> Result<()> {
        config.validate()?;
        let keys = self.keys.as_ref().ok_or_else(|| {
            CoreError::Unauthorized("personal model encryption is disabled".into())
        })?;
        let encrypted = keys.encrypt(summary(&config), &config.api_key)?;
        sqlx::query("INSERT INTO llm_helpers(subject,id,base_url,model,api_key,key_version,nonce,ciphertext) VALUES($1,$2,$3,$4,'',$5,$6,$7) ON CONFLICT(subject,id) DO UPDATE SET base_url=EXCLUDED.base_url,model=EXCLUDED.model,api_key='',key_version=EXCLUDED.key_version,nonce=EXCLUDED.nonce,ciphertext=EXCLUDED.ciphertext")
            .bind(config.subject).bind(config.id).bind(config.base_url).bind(config.model).bind(encrypted.key_version).bind(encrypted.nonce).bind(encrypted.ciphertext).execute(&self.pool).await.map_err(storage)?;
        Ok(())
    }
    async fn remove(&self, subject: &str, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM llm_helpers WHERE subject=$1 AND id=$2")
            .bind(subject)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    fn keys(byte: u8) -> Keyring {
        Keyring::parse(
            "v1",
            &serde_json::json!({"v1":base64::engine::general_purpose::STANDARD.encode([byte;32])})
                .to_string(),
        )
        .unwrap()
    }
    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL"]
    async fn postgres_personal_tokens_migrate_atomically_and_restart_without_plaintext() {
        let url = std::env::var("ASTER_TEST_PERSONAL_MODEL_URL").unwrap();
        let db = crate::store::PgStore::connect(&url).await.unwrap();
        let pool = db.pool.clone();
        let config = LlmConfig {
            subject: "alice".into(),
            id: "sql".into(),
            base_url: "https://models.example.invalid/v1".into(),
            model: "qwen".into(),
            api_key: "fixture-token".into(),
        };
        db.put(config.clone()).await.unwrap();
        let encrypted = EncryptedPersonalModels::new(pool.clone(), Some(keys(7)))
            .await
            .unwrap();
        let (plaintext, cipher): (String, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT api_key,ciphertext FROM llm_helpers WHERE subject='alice' AND id='sql'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(plaintext, "");
        assert!(cipher.is_some());
        assert_eq!(
            encrypted
                .get("alice", "sql")
                .await
                .unwrap()
                .unwrap()
                .api_key,
            "fixture-token"
        );
        drop(encrypted);
        let restarted = EncryptedPersonalModels::new(pool.clone(), Some(keys(7)))
            .await
            .unwrap();
        assert_eq!(
            restarted
                .get("alice", "sql")
                .await
                .unwrap()
                .unwrap()
                .api_key,
            "fixture-token"
        );
        assert!(EncryptedPersonalModels::new(pool.clone(), Some(keys(8)))
            .await
            .is_err());
        let disabled = EncryptedPersonalModels::new(pool.clone(), None)
            .await
            .unwrap();
        assert!(disabled.list("alice").await.unwrap()[0].api_key.is_empty());
        assert!(disabled.put(config.clone()).await.is_err());
        disabled.remove("alice", "sql").await.unwrap();
        assert!(disabled.get("alice", "sql").await.unwrap().is_none());

        // An interrupted migration must not leave a partly encrypted table.
        db.put(config.clone()).await.unwrap();
        let mut other = config.clone();
        other.id = "other".into();
        db.put(other).await.unwrap();
        sqlx::raw_sql("CREATE OR REPLACE FUNCTION fail_personal_migration() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='other' AND NEW.ciphertext IS NOT NULL THEN RAISE EXCEPTION 'fixture interruption'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_personal_migration BEFORE UPDATE ON llm_helpers FOR EACH ROW EXECUTE FUNCTION fail_personal_migration();").execute(&pool).await.unwrap();
        assert!(EncryptedPersonalModels::new(pool.clone(), Some(keys(7)))
            .await
            .is_err());
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM llm_helpers WHERE ciphertext IS NULL AND api_key='fixture-token'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 2);
        sqlx::query("DROP TRIGGER fail_personal_migration ON llm_helpers")
            .execute(&pool)
            .await
            .unwrap();
        let migrated = EncryptedPersonalModels::new(pool.clone(), Some(keys(7)))
            .await
            .unwrap();
        assert_eq!(
            migrated
                .get("alice", "other")
                .await
                .unwrap()
                .unwrap()
                .api_key,
            "fixture-token"
        );
        let policy = PgPersonalModelPolicy::new(pool.clone()).await.unwrap();
        assert_eq!(policy.get().await.unwrap().revision, 0);
        policy
            .replace(0, vec!["alice".into()], vec![], "admin")
            .await
            .unwrap();
        assert!(matches!(
            policy.replace(0, vec![], vec![], "admin").await,
            Err(CoreError::Conflict(_))
        ));
        let restarted = PgPersonalModelPolicy::new(pool.clone()).await.unwrap();
        assert_eq!(restarted.get().await.unwrap().subjects, vec!["alice"]);
        restarted.replace(1, vec![], vec![], "admin").await.unwrap();
        let events: i64 = sqlx::query_scalar("SELECT count(*) FROM personal_model_policy_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(events, 2);
        // Metadata tampering must not redirect a usable decrypted credential.
        sqlx::query("UPDATE llm_helpers SET base_url='https://elsewhere.invalid' WHERE id='other'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(migrated.get("alice", "other").await.is_err());
    }
}
