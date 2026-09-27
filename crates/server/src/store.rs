use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};

use aster_core::{
    AuditEvent, AuditSink, CoreError, EngineId, Grants, LlmConfig, LlmStore, NotebookOwnerChange,
    NotebookOwnerRecord, NotebookOwners, Result,
};

/// Postgres-backed metadata store for engine grants and the query audit trail.
pub struct PgStore {
    pub(crate) pool: PgPool,
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

#[derive(Clone)]
// Storage lands before the guarded production route in V7b1b.
#[allow(dead_code)]
pub(crate) struct TeamGitTargetChange {
    pub team: String,
    pub repository: String,
    pub repository_id: i64,
    pub installation_id: i64,
    pub default_branch: String,
    pub default_commit: String,
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
pub(crate) struct TeamGitTargetRecord {
    pub team: String,
    pub repository: String,
    pub repository_id: i64,
    pub installation_id: i64,
    pub default_branch: String,
    pub default_commit: String,
    pub version: i64,
}

impl PgStore {
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(storage)?;
        // CREATE TABLE IF NOT EXISTS still races across server replicas on an
        // empty database. Serialize bootstrap for this metadata schema.
        let mut tx = pool.begin().await.map_err(storage)?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(0x4153_5445_524d_4554_i64)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        sqlx::raw_sql(concat!(
            include_str!("../../../migrations/0001_init.sql"),
            include_str!("../../../migrations/0003_llm.sql"),
            include_str!("../../../migrations/0004_llm_helpers.sql"),
            include_str!("../../../migrations/0005_conversations.sql"),
            include_str!("../../../migrations/0006_notebook_owners.sql"),
            include_str!("../../../migrations/0009_team_git_targets.sql"),
        ))
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
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

    #[allow(dead_code)]
    pub(crate) async fn team_git_target(&self, team: &str) -> Result<Option<TeamGitTargetRecord>> {
        sqlx::query_as(
            "SELECT team, repository, repository_id, installation_id, default_branch, default_commit, version \
             FROM team_git_targets WHERE team = $1",
        )
        .bind(team)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)
    }

    #[allow(dead_code)]
    pub(crate) async fn change_team_git_target(
        &self,
        change: &TeamGitTargetChange,
        expected_version: Option<i64>,
        actor: &str,
    ) -> Result<TeamGitTargetRecord> {
        if change.team.is_empty()
            || change.repository.is_empty()
            || change.default_branch.is_empty()
            || change.repository_id <= 0
            || change.installation_id <= 0
            || ((change.default_commit.len() != 40 && change.default_commit.len() != 64)
                || !change
                    .default_commit
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit()))
            || actor.is_empty()
            || expected_version.is_some_and(|version| version < 1)
        {
            return Err(CoreError::Invalid("invalid team Git target record".into()));
        }
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let version: Option<i64> = if let Some(expected) = expected_version {
            sqlx::query_scalar(
                "UPDATE team_git_targets SET repository = $2, repository_id = $3, installation_id = $4, \
                 default_branch = $5, default_commit = $6, version = version + 1, configured_by = $7, \
                 configured_at = now() WHERE team = $1 AND version = $8 RETURNING version",
            )
            .bind(&change.team)
            .bind(&change.repository)
            .bind(change.repository_id)
            .bind(change.installation_id)
            .bind(&change.default_branch)
            .bind(&change.default_commit)
            .bind(actor)
            .bind(expected)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?
        } else {
            sqlx::query_scalar(
                "INSERT INTO team_git_targets \
                 (team, repository, repository_id, installation_id, default_branch, default_commit, version, configured_by) \
                 VALUES ($1, $2, $3, $4, $5, $6, 1, $7) ON CONFLICT DO NOTHING RETURNING version",
            )
            .bind(&change.team)
            .bind(&change.repository)
            .bind(change.repository_id)
            .bind(change.installation_id)
            .bind(&change.default_branch)
            .bind(&change.default_commit)
            .bind(actor)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?
        };
        let version =
            version.ok_or_else(|| CoreError::Conflict("team Git target changed".into()))?;
        sqlx::query(
            "INSERT INTO team_git_target_events \
             (team, previous_version, version, repository, repository_id, installation_id, default_branch, default_commit, actor_subject) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&change.team)
        .bind(expected_version)
        .bind(version)
        .bind(&change.repository)
        .bind(change.repository_id)
        .bind(change.installation_id)
        .bind(&change.default_branch)
        .bind(&change.default_commit)
        .bind(actor)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(TeamGitTargetRecord {
            team: change.team.clone(),
            repository: change.repository.clone(),
            repository_id: change.repository_id,
            installation_id: change.installation_id,
            default_branch: change.default_branch.clone(),
            default_commit: change.default_commit.clone(),
            version,
        })
    }
}

#[async_trait]
impl NotebookOwners for PgStore {
    async fn record(&self, source: &str, id: &str) -> Result<Option<NotebookOwnerRecord>> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT owner_subject, source_blob FROM notebook_owners WHERE source = $1 AND notebook_id = $2",
        )
        .bind(source)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(row.map(|(owner, content_revision)| NotebookOwnerRecord {
            owner,
            content_revision,
        }))
    }

    async fn change(&self, change: NotebookOwnerChange) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let changed = if let Some(expected) = &change.expected_owner {
            sqlx::query("UPDATE notebook_owners SET owner_subject = $4, source_blob = $5 WHERE source = $1 AND notebook_id = $2 AND owner_subject = $3")
                .bind(&change.source)
                .bind(&change.id)
                .bind(expected)
                .bind(&change.owner)
                .bind(&change.source_blob)
                .execute(&mut *tx)
                .await
                .map_err(storage)?
                .rows_affected()
        } else {
            sqlx::query("INSERT INTO notebook_owners (source, notebook_id, owner_subject, source_blob) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
                .bind(&change.source)
                .bind(&change.id)
                .bind(&change.owner)
                .bind(&change.source_blob)
                .execute(&mut *tx)
                .await
                .map_err(storage)?
                .rows_affected()
        };
        if changed != 1 {
            return Err(CoreError::Conflict("notebook owner changed".into()));
        }
        sqlx::query("INSERT INTO notebook_owner_events (source, notebook_id, previous_owner, owner_subject, source_blob, actor_subject, reason) VALUES ($1, $2, $3, $4, $5, $6, $7)")
            .bind(&change.source)
            .bind(&change.id)
            .bind(&change.expected_owner)
            .bind(&change.owner)
            .bind(&change.source_blob)
            .bind(&change.actor)
            .bind(&change.reason)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(())
    }

    async fn advance(
        &self,
        source: &str,
        id: &str,
        owner: &str,
        before: &str,
        after: &str,
    ) -> Result<()> {
        let changed = sqlx::query("UPDATE notebook_owners SET source_blob = $5 WHERE source = $1 AND notebook_id = $2 AND owner_subject = $3 AND source_blob = $4")
            .bind(source)
            .bind(id)
            .bind(owner)
            .bind(before)
            .bind(after)
            .execute(&self.pool)
            .await
            .map_err(storage)?
            .rows_affected();
        if changed != 1 {
            return Err(CoreError::Conflict(
                "notebook owner or content changed".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod notebook_owner_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires disposable Postgres via ASTER_TEST_METADATA_URL"]
    async fn postgres_notebook_owner_cas_and_audit_survive_reconnect() {
        let url = std::env::var("ASTER_TEST_METADATA_URL").expect("disposable metadata URL");
        let source = format!("test:{}", aster_core::new_sid().unwrap());
        let store = PgStore::connect(&url).await.unwrap();
        let initial = NotebookOwnerChange {
            source: source.clone(),
            id: "sales".into(),
            expected_owner: None,
            owner: "alice".into(),
            source_blob: "first".into(),
            actor: "root".into(),
            reason: Some("legacy assignment".into()),
        };
        store.change(initial.clone()).await.unwrap();
        assert!(matches!(
            store.change(initial).await,
            Err(CoreError::Conflict(_))
        ));
        store
            .advance(&source, "sales", "alice", "first", "second")
            .await
            .unwrap();
        assert!(matches!(
            store
                .advance(&source, "sales", "alice", "first", "third")
                .await,
            Err(CoreError::Conflict(_))
        ));
        store
            .change(NotebookOwnerChange {
                source: source.clone(),
                id: "sales".into(),
                expected_owner: Some("alice".into()),
                owner: "bob".into(),
                source_blob: "second".into(),
                actor: "root".into(),
                reason: Some("correct typo".into()),
            })
            .await
            .unwrap();
        drop(store);
        let store = PgStore::connect(&url).await.unwrap();
        let record = NotebookOwners::record(&store, &source, "sales")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record.owner, "bob");
        assert_eq!(record.content_revision, "second");
        let events: Vec<(Option<String>, String, String)> = sqlx::query_as(
            "SELECT previous_owner, owner_subject, reason FROM notebook_owner_events WHERE source = $1 ORDER BY id"
        ).bind(&source).fetch_all(&store.pool).await.unwrap();
        assert_eq!(
            events,
            vec![
                (None, "alice".into(), "legacy assignment".into()),
                (Some("alice".into()), "bob".into(), "correct typo".into()),
            ]
        );
    }
}

#[cfg(test)]
mod team_target_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires disposable Postgres via ASTER_TEST_METADATA_URL"]
    async fn postgres_team_target_schema_bootstraps_on_connect() {
        let url = std::env::var("ASTER_TEST_METADATA_URL").expect("disposable metadata URL");
        let store = PgStore::connect(&url).await.unwrap();
        let target: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('team_git_targets')::text")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        let events: Option<String> =
            sqlx::query_scalar("SELECT to_regclass('team_git_target_events')::text")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(target.as_deref(), Some("team_git_targets"));
        assert_eq!(events.as_deref(), Some("team_git_target_events"));
    }

    #[tokio::test]
    #[ignore = "requires disposable Postgres via ASTER_TEST_METADATA_URL"]
    async fn postgres_team_target_cas_and_audit_survive_reconnect() {
        let url = std::env::var("ASTER_TEST_METADATA_URL").expect("disposable metadata URL");
        let team = format!("test-{}", aster_core::new_sid().unwrap());
        let store = PgStore::connect(&url).await.unwrap();
        let first = TeamGitTargetChange {
            team: team.clone(),
            repository: "example/alpha".into(),
            repository_id: 101,
            installation_id: 201,
            default_branch: "main".into(),
            default_commit: "a".repeat(40),
        };
        let created = store
            .change_team_git_target(&first, None, "alice")
            .await
            .unwrap();
        assert_eq!(created.version, 1);
        assert!(matches!(
            store.change_team_git_target(&first, None, "bob").await,
            Err(CoreError::Conflict(_))
        ));
        let mut changed = first.clone();
        changed.repository = "example/next".into();
        changed.repository_id = 102;
        changed.default_branch = "trunk".into();
        changed.default_commit = "b".repeat(40);
        assert!(matches!(
            store.change_team_git_target(&changed, Some(2), "bob").await,
            Err(CoreError::Conflict(_))
        ));
        let updated = store
            .change_team_git_target(&changed, Some(1), "alice")
            .await
            .unwrap();
        assert_eq!(updated.version, 2);
        let reopened = PgStore::connect(&url).await.unwrap();
        let current = reopened.team_git_target(&team).await.unwrap().unwrap();
        assert_eq!(current.team, team);
        assert_eq!(current.repository, "example/next");
        assert_eq!(current.repository_id, 102);
        assert_eq!(current.installation_id, 201);
        assert_eq!(current.default_branch, "trunk");
        assert_eq!(current.default_commit, "b".repeat(40));
        assert_eq!(current.version, 2);
        let events: Vec<(Option<i64>, i64, String)> = sqlx::query_as(
            "SELECT previous_version, version, actor_subject FROM team_git_target_events WHERE team = $1 ORDER BY id",
        )
        .bind(&team)
        .fetch_all(&reopened.pool)
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![(None, 1, "alice".into()), (Some(1), 2, "alice".into())]
        );
    }

    #[tokio::test]
    #[ignore = "requires disposable Postgres via ASTER_TEST_METADATA_URL"]
    async fn postgres_team_target_concurrent_updates_choose_one_version() {
        let url = std::env::var("ASTER_TEST_METADATA_URL").expect("disposable metadata URL");
        let team = format!("test-{}", aster_core::new_sid().unwrap());
        let store = PgStore::connect(&url).await.unwrap();
        let initial = TeamGitTargetChange {
            team: team.clone(),
            repository: "example/alpha".into(),
            repository_id: 101,
            installation_id: 201,
            default_branch: "main".into(),
            default_commit: "a".repeat(40),
        };
        store
            .change_team_git_target(&initial, None, "alice")
            .await
            .unwrap();
        let mut left = initial.clone();
        left.repository = "example/left".into();
        let mut right = initial;
        right.repository = "example/right".into();
        let (left_result, right_result) = tokio::join!(
            store.change_team_git_target(&left, Some(1), "alice"),
            store.change_team_git_target(&right, Some(1), "bob"),
        );
        assert_eq!(left_result.is_ok() as u8 + right_result.is_ok() as u8, 1);
        assert!(left_result.is_ok() || matches!(left_result, Err(CoreError::Conflict(_))));
        assert!(right_result.is_ok() || matches!(right_result, Err(CoreError::Conflict(_))));
        let current = store.team_git_target(&team).await.unwrap().unwrap();
        assert_eq!(current.version, 2);
        assert!(matches!(
            current.repository.as_str(),
            "example/left" | "example/right"
        ));
        let events: i64 =
            sqlx::query_scalar("SELECT count(*) FROM team_git_target_events WHERE team = $1")
                .bind(&team)
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(events, 2);
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
