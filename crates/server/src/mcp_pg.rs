//! Durable encrypted MCP registrations with compare-and-swap and metadata audit.
use aster_core::{
    mcp::{McpRecord, McpStore},
    CoreError, Result,
};
use async_trait::async_trait;
use sqlx::PgPool;
pub struct PgMcp {
    pool: PgPool,
}
fn storage(error: impl std::fmt::Display) -> CoreError {
    CoreError::Storage(error.to_string())
}
impl PgMcp {
    pub async fn new(pool: PgPool) -> Result<Self> {
        sqlx::raw_sql(include_str!("../../../migrations/0014_mcp_connections.sql"))
            .execute(&pool)
            .await
            .map_err(storage)?;
        Ok(Self { pool })
    }
}
#[async_trait]
impl McpStore for PgMcp {
    async fn list(&self) -> Result<Vec<McpRecord>> {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT content FROM ai_mcp_connections ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?
        .into_iter()
        .map(|v| serde_json::from_value(v).map_err(storage))
        .collect()
    }
    async fn get(&self, id: &str) -> Result<Option<McpRecord>> {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT content FROM ai_mcp_connections WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?
        .map(|v| serde_json::from_value(v).map_err(storage))
        .transpose()
    }
    async fn remember(
        &self,
        id: &str,
        subject: &str,
        fingerprint: &str,
        expires: i64,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query("LOCK TABLE ai_mcp_confirmations IN EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        sqlx::query("DELETE FROM ai_mcp_confirmations WHERE expires <= $1")
            .bind(expires - 300)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_mcp_confirmations")
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        if count >= 1000 {
            return Err(CoreError::Conflict(
                "too many pending tool confirmations".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO ai_mcp_confirmations(id,subject,fingerprint,expires) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(subject)
        .bind(fingerprint)
        .bind(expires)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(())
    }
    async fn consume(&self, id: &str, subject: &str, fingerprint: &str, now: i64) -> Result<bool> {
        Ok(sqlx::query("DELETE FROM ai_mcp_confirmations WHERE id=$1 AND subject=$2 AND fingerprint=$3 AND expires>$4").bind(id).bind(subject).bind(fingerprint).bind(now).execute(&self.pool).await.map_err(storage)?.rows_affected() == 1)
    }
    async fn put(&self, record: McpRecord, expected: i64, actor: &str) -> Result<()> {
        record.connection.validate()?;
        if expected.checked_add(1) != Some(record.connection.revision) {
            return Err(CoreError::Invalid("invalid MCP revision".into()));
        }
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let c = &record.connection;
        let result = if expected == 0 {
            sqlx::query("INSERT INTO ai_mcp_connections(id,revision,content) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(&c.id).bind(c.revision).bind(serde_json::to_value(&record).map_err(storage)?).execute(&mut *tx).await
        } else {
            sqlx::query("UPDATE ai_mcp_connections SET revision=$2,content=$3 WHERE id=$1 AND revision=$4")
                .bind(&c.id).bind(c.revision).bind(serde_json::to_value(&record).map_err(storage)?).bind(expected).execute(&mut *tx).await
        }.map_err(storage)?;
        if result.rows_affected() != 1 {
            return Err(CoreError::Conflict(
                "MCP connection changed; reload before saving".into(),
            ));
        }
        sqlx::query("INSERT INTO ai_mcp_connection_events(connection_id,revision,actor,metadata) VALUES($1,$2,$3,$4)").bind(&c.id).bind(c.revision).bind(actor).bind(serde_json::to_value(c).map_err(storage)?).execute(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_core::{mcp::McpConnection, AdminSharedModelSummary};
    use base64::Engine;
    use std::sync::Arc;
    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL; run just ai-registry-postgres-test"]
    async fn mcp_credentials_and_single_use_confirmations_survive_restart() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&std::env::var("ASTER_TEST_SKILLS_URL").unwrap())
            .await
            .unwrap();
        let keys = || {
            crate::Keyring::parse("v1",&serde_json::json!({"v1":base64::engine::general_purpose::STANDARD.encode([4u8;32])}).to_string()).unwrap()
        };
        let store = PgMcp::new(pool.clone()).await.unwrap();
        let id = aster_core::new_sid().unwrap();
        let c = McpConnection {
            id: id.clone(),
            name: "Donsetch mock".into(),
            description: String::new(),
            url: "https://tools.example.invalid/mcp".into(),
            auth: "bearer".into(),
            enabled: true,
            revision: 1,
            subjects: vec!["alice".into()],
            teams: vec![],
            approved_reads: Default::default(),
        };
        let sealed = keys()
            .encrypt(
                AdminSharedModelSummary {
                    id: format!("mcp/{id}"),
                    base_url: c.url.clone(),
                    model: c.auth.clone(),
                },
                "fixture-only-token",
            )
            .unwrap();
        store
            .put(
                McpRecord {
                    connection: c,
                    key_version: sealed.key_version,
                    nonce: sealed.nonce,
                    ciphertext: sealed.ciphertext,
                },
                0,
                "admin",
            )
            .await
            .unwrap();
        let nonce = aster_core::new_sid().unwrap();
        store
            .remember(&nonce, "alice", "exact-action", crate::now() + 300)
            .await
            .unwrap();
        drop(store);
        let reopened = Arc::new(PgMcp::new(pool.clone()).await.unwrap());
        let record = reopened.get(&id).await.unwrap().unwrap();
        assert!(!serde_json::to_string(&record)
            .unwrap()
            .contains("fixture-only-token"));
        assert_eq!(record.connection.subjects, vec!["alice"]);
        let registry = crate::mcp::McpRegistry::new(
            reopened.clone(),
            crate::DestinationPolicy::parse("https://tools.example.invalid").unwrap(),
            keys(),
            Arc::new(crate::HttpMcp),
        );
        registry.verify_keys().await.unwrap();
        let wrong = crate::Keyring::parse(
            "v1",
            &serde_json::json!({"v1":base64::engine::general_purpose::STANDARD.encode([5u8;32])})
                .to_string(),
        )
        .unwrap();
        assert!(crate::mcp::McpRegistry::new(
            reopened.clone(),
            crate::DestinationPolicy::parse("").unwrap(),
            wrong,
            Arc::new(crate::HttpMcp)
        )
        .verify_keys()
        .await
        .is_err());
        assert!(!reopened
            .consume(&nonce, "bob", "exact-action", crate::now())
            .await
            .unwrap());
        assert!(!reopened
            .consume(&nonce, "alice", "different-action", crate::now())
            .await
            .unwrap());
        let (a, b) = tokio::join!(
            reopened.consume(&nonce, "alice", "exact-action", crate::now()),
            reopened.consume(&nonce, "alice", "exact-action", crate::now())
        );
        assert_ne!(
            a.unwrap(),
            b.unwrap(),
            "exactly one concurrent confirmation may execute"
        );
        assert!(!reopened
            .consume(&nonce, "alice", "exact-action", crate::now())
            .await
            .unwrap());
    }
}
