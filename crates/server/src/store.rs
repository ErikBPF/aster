use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};

use aster_core::{AuditEvent, AuditSink, CoreError, EngineId, Grants, LlmConfig, LlmStore, Result};

/// Postgres-backed metadata store for engine grants and the query audit trail.
pub struct PgStore {
    pool: PgPool,
}

fn storage(error: sqlx::Error) -> CoreError {
    CoreError::Storage(error.to_string())
}

#[derive(sqlx::FromRow)]
struct AuditRow {
    subject: String,
    engine: String,
    catalog: Option<String>,
    schema: Option<String>,
    sql: String,
    latency_ms: i64,
    row_count: i64,
    ok: bool,
}

impl PgStore {
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(storage)?;
        // ponytail: one idempotent script per boot; move to sqlx::migrate! when
        // the schema starts evolving.
        sqlx::raw_sql(concat!(
            include_str!("../../../migrations/0001_init.sql"),
            include_str!("../../../migrations/0003_llm.sql"),
            include_str!("../../../migrations/0004_llm_helpers.sql"),
        ))
        .execute(&pool)
        .await
        .map_err(storage)?;
        Ok(Self { pool })
    }

    pub async fn seed_grant(&self, subject: &str, engine: &str) -> Result<()> {
        sqlx::query("INSERT INTO grants (subject, engine) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(subject)
            .bind(engine)
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(())
    }
}

fn llm_config(subject: &str, row: (String, String, String, String)) -> LlmConfig {
    let (id, base_url, model, api_key) = row;
    LlmConfig {
        subject: subject.to_string(),
        id,
        base_url,
        model,
        api_key,
    }
}

#[async_trait]
impl LlmStore for PgStore {
    async fn list(&self, subject: &str) -> Result<Vec<LlmConfig>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT id, base_url, model, api_key FROM llm_helpers
             WHERE subject = $1 ORDER BY id",
        )
        .bind(subject)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        Ok(rows
            .into_iter()
            .map(|row| llm_config(subject, row))
            .collect())
    }

    async fn get(&self, subject: &str, id: &str) -> Result<Option<LlmConfig>> {
        let row: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT id, base_url, model, api_key FROM llm_helpers
             WHERE subject = $1 AND id = $2",
        )
        .bind(subject)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.map(|row| llm_config(subject, row)))
    }

    async fn put(&self, config: LlmConfig) -> Result<()> {
        config.validate()?;
        sqlx::query(
            "INSERT INTO llm_helpers (subject, id, base_url, model, api_key)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (subject, id) DO UPDATE SET
               base_url = EXCLUDED.base_url,
               model = EXCLUDED.model,
               api_key = EXCLUDED.api_key,
               updated_at = now()",
        )
        .bind(&config.subject)
        .bind(&config.id)
        .bind(&config.base_url)
        .bind(&config.model)
        .bind(&config.api_key)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn remove(&self, subject: &str, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM llm_helpers WHERE subject = $1 AND id = $2")
            .bind(subject)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(())
    }
}

#[async_trait]
impl Grants for PgStore {
    async fn allowed(&self, subject: &str, engine: &EngineId) -> Result<bool> {
        let row: Option<(i32,)> =
            sqlx::query_as("SELECT 1 FROM grants WHERE subject = $1 AND engine = $2")
                .bind(subject)
                .bind(engine.0.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage)?;
        Ok(row.is_some())
    }
}

#[async_trait]
impl AuditSink for PgStore {
    async fn record(&self, event: &AuditEvent) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit (subject, engine, catalog, schema, sql, latency_ms, row_count, ok)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(event.subject.as_str())
        .bind(event.engine.0.as_str())
        .bind(event.catalog.as_deref())
        .bind(event.schema.as_deref())
        .bind(event.sql.as_str())
        .bind(event.latency_ms as i64)
        .bind(event.row_count as i64)
        .bind(event.ok)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn events(&self) -> Result<Vec<AuditEvent>> {
        let rows: Vec<AuditRow> = sqlx::query_as(
            "SELECT subject, engine, catalog, schema, sql, latency_ms, row_count, ok
             FROM audit ORDER BY id DESC LIMIT 500",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;

        Ok(rows
            .into_iter()
            .map(|row| AuditEvent {
                subject: row.subject,
                engine: EngineId::new(row.engine),
                catalog: row.catalog,
                schema: row.schema,
                sql: row.sql,
                latency_ms: row.latency_ms as u64,
                row_count: row.row_count as usize,
                ok: row.ok,
            })
            .collect())
    }
}
