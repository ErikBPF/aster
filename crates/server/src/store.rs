use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};

use aster_core::{AuditEvent, AuditSink, CoreError, EngineId, Grants, Result};

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
        sqlx::raw_sql(include_str!("../../../migrations/0001_init.sql"))
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
