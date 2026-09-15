//! aster controller: single-replica reconcile loop against the metadata
//! database. It mirrors the configured engine and catalog pool into the
//! database with a fresh health observation, and prunes expired audit events.
//! ponytail: a plain loop, no CRD and no operator — promote to a kube-rs
//! controller only when configuration must become a cluster resource.

use std::time::Duration;

use aster_core::{AppConfig, CatalogHealth, CoreError, EngineHealth};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

const RECONCILE_SECONDS: u64 = 30;
const AUDIT_RETENTION_DAYS: i32 = 30;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,aster_controller=info".into()),
        )
        .init();

    let url = std::env::var("DATABASE_URL").map_err(|_| {
        anyhow::anyhow!("DATABASE_URL is required: the metadata database is the controller's job")
    })?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;
    // ponytail: the same idempotent scripts the server runs; move to
    // sqlx::migrate! when the schema starts evolving between releases.
    sqlx::raw_sql(concat!(
        include_str!("../../../migrations/0001_init.sql"),
        include_str!("../../../migrations/0002_controller.sql"),
        include_str!("../../../migrations/0003_llm.sql")
    ))
    .execute(&pool)
    .await?;

    let config = AppConfig::from_env();
    let interval = env_number("ASTER_RECONCILE_SECONDS", RECONCILE_SECONDS as i64) as u64;
    let retention_days =
        env_number("ASTER_AUDIT_RETENTION_DAYS", AUDIT_RETENTION_DAYS as i64) as i32;
    tracing::info!("aster-controller reconciling every {interval}s");

    loop {
        match reconcile(&pool, &config, retention_days).await {
            Ok(()) => tracing::info!("reconcile pass complete"),
            Err(error) => tracing::error!(%error, "reconcile pass failed; retrying next tick"),
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

fn env_number(name: &str, fallback: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

/// One reconcile pass: refresh cluster state, then prune expired audit events.
pub async fn reconcile(
    pool: &PgPool,
    config: &AppConfig,
    retention_days: i32,
) -> Result<(), CoreError> {
    for engine_config in &config.engines {
        let engine = aster_engines::engine_from_config(engine_config)?;
        let health = engine_health(engine.health().await);
        sqlx::query(
            "INSERT INTO engines (id, kind, endpoint, routing_group, health, checked_at)
             VALUES ($1, $2, $3, $4, $5, now())
             ON CONFLICT (id) DO UPDATE SET kind = EXCLUDED.kind, endpoint = EXCLUDED.endpoint,
                 routing_group = EXCLUDED.routing_group, health = EXCLUDED.health,
                 checked_at = now()",
        )
        .bind(&engine_config.id)
        .bind(&engine_config.kind)
        .bind(&engine_config.endpoint)
        .bind(engine_config.routing_group.as_deref())
        .bind(health)
        .execute(pool)
        .await
        .map_err(storage)?;
        tracing::debug!(engine = %engine_config.id, health, "engine reconciled");
    }

    for catalog_config in &config.catalogs {
        let catalog = aster_catalogs::catalog_from_config(catalog_config)?;
        let health = catalog_health(catalog.health().await);
        sqlx::query(
            "INSERT INTO catalogs (id, kind, endpoint, catalog, health, checked_at)
             VALUES ($1, $2, $3, $4, $5, now())
             ON CONFLICT (id) DO UPDATE SET kind = EXCLUDED.kind, endpoint = EXCLUDED.endpoint,
                 catalog = EXCLUDED.catalog, health = EXCLUDED.health, checked_at = now()",
        )
        .bind(&catalog_config.id)
        .bind(&catalog_config.kind)
        .bind(&catalog_config.endpoint)
        .bind(catalog_config.catalog.as_deref())
        .bind(health)
        .execute(pool)
        .await
        .map_err(storage)?;
        tracing::debug!(catalog = %catalog_config.id, health, "catalog reconciled");
    }

    let pruned =
        sqlx::query("DELETE FROM audit WHERE created_at < now() - make_interval(days => $1)")
            .bind(retention_days)
            .execute(pool)
            .await
            .map_err(storage)?
            .rows_affected();
    if pruned > 0 {
        tracing::info!(pruned, "pruned expired audit events");
    }

    Ok(())
}

fn engine_health(health: EngineHealth) -> &'static str {
    match health {
        EngineHealth::Healthy => "healthy",
        EngineHealth::Degraded => "degraded",
        EngineHealth::Unavailable => "unavailable",
    }
}

fn catalog_health(health: CatalogHealth) -> &'static str {
    match health {
        CatalogHealth::Healthy => "healthy",
        CatalogHealth::Degraded => "degraded",
        CatalogHealth::Unavailable => "unavailable",
    }
}

fn storage(error: sqlx::Error) -> CoreError {
    CoreError::Storage(format!("metadata database: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_labels_are_stable() {
        assert_eq!(engine_health(EngineHealth::Healthy), "healthy");
        assert_eq!(engine_health(EngineHealth::Degraded), "degraded");
        assert_eq!(engine_health(EngineHealth::Unavailable), "unavailable");
        assert_eq!(catalog_health(CatalogHealth::Healthy), "healthy");
    }

    #[test]
    fn env_numbers_default_and_parse() {
        assert_eq!(env_number("ASTER_DEFINITELY_UNSET_VAR", 42), 42);
    }
}
