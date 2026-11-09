//! Immutable skill revisions with current sharing metadata in PostgreSQL.
use aster_core::skills::{
    validate_selections, validate_sharing, Skill, SkillAudience, SkillDefaults, SkillDraft,
    SkillStore,
};
use aster_core::{CoreError, Result};
use async_trait::async_trait;
use sqlx::PgPool;

pub struct PgSkills {
    pool: PgPool,
}
fn storage(error: impl std::fmt::Display) -> CoreError {
    CoreError::Storage(error.to_string())
}
type StoredSkill = (serde_json::Value, i64, String, Option<String>);
fn decode(row: StoredSkill) -> Result<Skill> {
    let mut skill: Skill = serde_json::from_value(row.0).map_err(storage)?;
    skill.sharing_revision = row.1;
    skill.audience = serde_json::from_value(serde_json::Value::String(row.2)).map_err(storage)?;
    skill.team = row.3;
    Ok(skill)
}
fn audience_name(audience: &SkillAudience) -> Result<String> {
    serde_json::to_value(audience)
        .map_err(storage)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| storage("invalid skill audience"))
}
impl PgSkills {
    pub async fn new(pool: PgPool) -> Result<Self> {
        sqlx::raw_sql(include_str!("../../../migrations/0012_skills.sql"))
            .execute(&pool)
            .await
            .map_err(storage)?;
        Ok(Self { pool })
    }
}
#[async_trait]
impl SkillStore for PgSkills {
    async fn defaults(&self, subject: &str) -> Result<SkillDefaults> {
        let row: Option<(i64, serde_json::Value)> =
            sqlx::query_as("SELECT revision,skills FROM ai_skill_defaults WHERE subject=$1")
                .bind(subject)
                .fetch_optional(&self.pool)
                .await
                .map_err(storage)?;
        match row {
            Some((revision, skills)) => Ok(SkillDefaults {
                revision,
                skills: serde_json::from_value(skills).map_err(storage)?,
            }),
            None => Ok(SkillDefaults::default()),
        }
    }
    async fn put_defaults(&self, subject: &str, defaults: SkillDefaults) -> Result<SkillDefaults> {
        validate_selections(&defaults.skills)?;
        let next = defaults
            .revision
            .checked_add(1)
            .filter(|v| *v > 0)
            .ok_or_else(|| CoreError::Invalid("invalid defaults revision".into()))?;
        let sql = if defaults.revision == 0 {
            "INSERT INTO ai_skill_defaults(subject,revision,skills) VALUES($1,$2,$3) ON CONFLICT(subject) DO NOTHING RETURNING revision,skills"
        } else {
            "UPDATE ai_skill_defaults SET revision=$2,skills=$3 WHERE subject=$1 AND revision=$2-1 RETURNING revision,skills"
        };
        let row: Option<(i64, serde_json::Value)> = sqlx::query_as(sql)
            .bind(subject)
            .bind(next)
            .bind(serde_json::to_value(&defaults.skills).map_err(storage)?)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        let (revision, skills) = row.ok_or_else(|| {
            CoreError::Conflict("skill defaults changed; reload before saving".into())
        })?;
        Ok(SkillDefaults {
            revision,
            skills: serde_json::from_value(skills).map_err(storage)?,
        })
    }
    async fn list(&self) -> Result<Vec<Skill>> {
        sqlx::query_as::<_, StoredSkill>("SELECT v.content,s.sharing_revision,s.audience,s.team FROM ai_skills s JOIN ai_skill_versions v ON v.skill_id=s.id AND v.revision=s.revision ORDER BY s.id")
            .fetch_all(&self.pool).await.map_err(storage)?.into_iter().map(decode).collect()
    }
    async fn get(&self, id: &str, revision: Option<i64>) -> Result<Option<Skill>> {
        sqlx::query_as::<_, StoredSkill>("SELECT v.content,s.sharing_revision,s.audience,s.team FROM ai_skills s JOIN ai_skill_versions v ON v.skill_id=s.id AND v.revision=COALESCE($2,s.revision) WHERE s.id=$1")
            .bind(id).bind(revision).fetch_optional(&self.pool).await.map_err(storage)?.map(decode).transpose()
    }
    async fn versions(&self, id: &str) -> Result<Vec<Skill>> {
        sqlx::query_as::<_, StoredSkill>("SELECT v.content,s.sharing_revision,s.audience,s.team FROM ai_skills s JOIN ai_skill_versions v ON v.skill_id=s.id WHERE s.id=$1 ORDER BY v.revision")
            .bind(id).fetch_all(&self.pool).await.map_err(storage)?.into_iter().map(decode).collect()
    }
    async fn put(&self, draft: SkillDraft) -> Result<Skill> {
        draft.validate()?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let created = if draft.expected_revision == 0 {
            sqlx::query("INSERT INTO ai_skills(id,owner,revision,sharing_revision,audience) VALUES($1,$2,1,1,'private') ON CONFLICT DO NOTHING")
                .bind(&draft.id).bind(&draft.owner).execute(&mut *tx).await.map_err(storage)?.rows_affected() == 1
        } else {
            false
        };
        let (owner, revision, sharing_revision, audience, team): (String,i64,i64,String,Option<String>) = sqlx::query_as("SELECT owner,revision,sharing_revision,audience,team FROM ai_skills WHERE id=$1 FOR UPDATE")
            .bind(&draft.id).fetch_optional(&mut *tx).await.map_err(storage)?.ok_or_else(|| CoreError::NotFound("skill unavailable".into()))?;
        if owner != draft.owner {
            return Err(CoreError::Unauthorized("skill owner required".into()));
        }
        let existing: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ai_skill_versions WHERE skill_id=$1)")
                .bind(&draft.id)
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
        if !existing && !created {
            return Err(CoreError::NotFound("skill unavailable".into()));
        }
        if (if existing { revision } else { 0 }) != draft.expected_revision {
            return Err(CoreError::Conflict(
                "skill changed; reload before saving".into(),
            ));
        }
        let next = draft
            .expected_revision
            .checked_add(1)
            .ok_or_else(|| CoreError::Conflict("skill revision exhausted".into()))?;
        let skill = Skill {
            id: draft.id,
            owner: owner.clone(),
            title: draft.title,
            description: draft.description,
            revision: next,
            sharing_revision,
            audience: serde_json::from_value(serde_json::Value::String(audience))
                .map_err(storage)?,
            team,
            instructions: draft.instructions,
            references: draft.references,
            author: owner,
            created_at_ms: draft.created_at_ms,
        };
        sqlx::query("INSERT INTO ai_skill_versions(skill_id,revision,content) VALUES($1,$2,$3)")
            .bind(&skill.id)
            .bind(next)
            .bind(serde_json::to_value(&skill).map_err(storage)?)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        sqlx::query("UPDATE ai_skills SET revision=$2 WHERE id=$1")
            .bind(&skill.id)
            .bind(next)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(skill)
    }
    async fn delete(
        &self,
        id: &str,
        owner: &str,
        revision: i64,
        sharing_revision: i64,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let current: Option<(String, i64, i64)> = sqlx::query_as(
            "SELECT owner,revision,sharing_revision FROM ai_skills WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
        let (actual_owner, actual_revision, actual_sharing) =
            current.ok_or_else(|| CoreError::NotFound("skill unavailable".into()))?;
        if actual_owner != owner {
            return Err(CoreError::Unauthorized("skill owner required".into()));
        }
        if actual_revision != revision || actual_sharing != sharing_revision {
            return Err(CoreError::Conflict(
                "skill changed; reload before deleting".into(),
            ));
        }
        // Keep the identity row as a tombstone: old pins must never resolve to replacement content.
        let removed = sqlx::query("DELETE FROM ai_skill_versions WHERE skill_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        if removed.rows_affected() == 0 {
            return Err(CoreError::NotFound("skill unavailable".into()));
        }
        tx.commit().await.map_err(storage)?;
        Ok(())
    }
    async fn share(
        &self,
        id: &str,
        expected_revision: i64,
        audience: SkillAudience,
        team: Option<String>,
    ) -> Result<Skill> {
        validate_sharing(&audience, team.as_deref())?;
        let next = expected_revision
            .checked_add(1)
            .ok_or_else(|| CoreError::Conflict("skill sharing revision exhausted".into()))?;
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let result = sqlx::query("UPDATE ai_skills SET sharing_revision=$3,audience=$4,team=$5 WHERE id=$1 AND sharing_revision=$2 AND EXISTS(SELECT 1 FROM ai_skill_versions WHERE skill_id=$1)")
            .bind(id).bind(expected_revision).bind(next).bind(audience_name(&audience)?).bind(team).execute(&mut *tx).await.map_err(storage)?;
        if result.rows_affected() != 1 {
            return Err(CoreError::Conflict(
                "skill sharing changed; reload before saving".into(),
            ));
        }
        let row = sqlx::query_as::<_, StoredSkill>("SELECT v.content,s.sharing_revision,s.audience,s.team FROM ai_skills s JOIN ai_skill_versions v ON v.skill_id=s.id AND v.revision=s.revision WHERE s.id=$1")
            .bind(id).fetch_one(&mut *tx).await.map_err(storage)?;
        let skill = decode(row)?;
        tx.commit().await.map_err(storage)?;
        Ok(skill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL; run tests/personal-model-postgres.sh"]
    async fn persists_immutable_versions_and_current_sharing() {
        let url = std::env::var("ASTER_TEST_SKILLS_URL").expect("disposable database URL");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .unwrap();
        let store = PgSkills::new(pool.clone()).await.unwrap();
        let id = aster_core::new_sid().unwrap();
        let draft = SkillDraft {
            id: id.clone(),
            owner: "alice".into(),
            title: "Review".into(),
            description: String::new(),
            instructions: "Version one".into(),
            references: vec![],
            expected_revision: 0,
            created_at_ms: 1,
        };
        store.put(draft.clone()).await.unwrap();
        let mut update = draft.clone();
        update.expected_revision = 1;
        update.instructions = "Version two".into();
        store.put(update.clone()).await.unwrap();
        assert!(matches!(
            store.put(update).await,
            Err(CoreError::Conflict(_))
        ));
        store
            .share(&id, 1, SkillAudience::Organization, None)
            .await
            .unwrap();
        drop(store);
        let reopened = PgSkills::new(pool).await.unwrap();
        let first = reopened.get(&id, Some(1)).await.unwrap().unwrap();
        assert_eq!(first.instructions, "Version one");
        assert_eq!(first.audience, SkillAudience::Organization);
        assert_eq!(first.sharing_revision, 2);
        assert_eq!(
            reopened.get(&id, None).await.unwrap().unwrap().instructions,
            "Version two"
        );
        assert_eq!(reopened.versions(&id).await.unwrap().len(), 2);
        assert!(matches!(
            reopened.share(&id, 1, SkillAudience::Private, None).await,
            Err(CoreError::Conflict(_))
        ));
        reopened
            .share(&id, 2, SkillAudience::Private, None)
            .await
            .unwrap();
        assert_eq!(
            reopened.get(&id, Some(1)).await.unwrap().unwrap().audience,
            SkillAudience::Private
        );
    }
    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL; run tests/personal-model-postgres.sh"]
    async fn deletion_purges_versions_and_reserves_identity_after_restart() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&std::env::var("ASTER_TEST_SKILLS_URL").unwrap())
            .await
            .unwrap();
        let store = PgSkills::new(pool.clone()).await.unwrap();
        let id = aster_core::new_sid().unwrap();
        let draft = SkillDraft {
            id: id.clone(),
            owner: "alice".into(),
            title: "Delete fixture".into(),
            description: String::new(),
            instructions: "First version".into(),
            references: vec![],
            expected_revision: 0,
            created_at_ms: 1,
        };
        store.put(draft.clone()).await.unwrap();
        let mut second = draft.clone();
        second.expected_revision = 1;
        store.put(second.clone()).await.unwrap();
        assert!(matches!(
            store.delete(&id, "bob", 2, 1).await,
            Err(CoreError::Unauthorized(_))
        ));
        assert!(matches!(
            store.delete(&id, "alice", 1, 1).await,
            Err(CoreError::Conflict(_))
        ));
        store
            .share(&id, 1, SkillAudience::Organization, None)
            .await
            .unwrap();
        assert!(matches!(
            store.delete(&id, "alice", 2, 1).await,
            Err(CoreError::Conflict(_))
        ));
        let (a, b) = tokio::join!(
            store.delete(&id, "alice", 2, 2),
            store.delete(&id, "alice", 2, 2)
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        drop(store);
        let reopened = PgSkills::new(pool.clone()).await.unwrap();
        assert!(reopened.get(&id, None).await.unwrap().is_none());
        assert!(reopened.get(&id, Some(1)).await.unwrap().is_none());
        assert!(reopened.versions(&id).await.unwrap().is_empty());
        assert!(!reopened
            .list()
            .await
            .unwrap()
            .iter()
            .any(|skill| skill.id == id));
        assert!(reopened.put(draft).await.is_err());
        assert!(reopened.put(second).await.is_err());
        assert!(reopened
            .share(&id, 2, SkillAudience::Private, None)
            .await
            .is_err());
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM ai_skill_versions WHERE skill_id=$1")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_skills WHERE id=$1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "identity tombstone must survive restart");
    }
}
