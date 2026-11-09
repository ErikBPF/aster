//! Versioned instructions and text references. Content never grants execution authority.
use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{CoreError, Result};

pub const MAX_SKILL_BYTES: usize = 32 * 1024;
pub const MAX_SKILL_REFERENCES: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSelection {
    pub id: String,
    pub revision: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkillDefaults {
    pub revision: i64,
    pub skills: Vec<SkillSelection>,
}

pub fn validate_selections(skills: &[SkillSelection]) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if skills.len() > 8
        || skills
            .iter()
            .any(|s| !crate::llm::valid_id(&s.id) || s.revision <= 0 || !ids.insert(&s.id))
    {
        return Err(CoreError::Invalid(
            "select at most 8 distinct skills with valid versions".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillReference {
    pub name: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SkillAudience {
    Private,
    Team,
    Organization,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub owner: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub revision: i64,
    pub sharing_revision: i64,
    pub audience: SkillAudience,
    pub team: Option<String>,
    pub instructions: String,
    pub references: Vec<SkillReference>,
    pub author: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct SkillDraft {
    pub id: String,
    pub owner: String,
    pub title: String,
    pub description: String,
    pub instructions: String,
    pub references: Vec<SkillReference>,
    pub expected_revision: i64,
    pub created_at_ms: i64,
}

impl SkillDraft {
    pub fn validate(&self) -> Result<()> {
        if !crate::llm::valid_id(&self.id)
            || self.owner.is_empty()
            || self.title.trim().is_empty()
            || self.title.contains('\0')
            || self.title.len() > 160
            || self.description.len() > 512
            || self.description.contains('\0')
            || self.instructions.trim().is_empty()
            || self.references.len() > MAX_SKILL_REFERENCES
            || self.expected_revision < 0
        {
            return Err(CoreError::Invalid("invalid skill".into()));
        }
        let mut names = std::collections::HashSet::new();
        let mut bytes = self.title.len() + self.description.len() + self.instructions.len();
        for reference in &self.references {
            if !valid_reference_path(&reference.name)
                || self
                    .references
                    .iter()
                    .any(|other| reference.name.starts_with(&(other.name.clone() + "/")))
                || !names.insert(&reference.name)
                || reference.content.contains('\0')
            {
                return Err(CoreError::Invalid("invalid skill reference".into()));
            }
            bytes = bytes
                .saturating_add(reference.name.len())
                .saturating_add(reference.content.len());
        }
        if bytes > MAX_SKILL_BYTES || self.instructions.contains('\0') {
            return Err(CoreError::Invalid("skill content exceeds limits".into()));
        }
        Ok(())
    }
}

/// Logical package paths only; these names never authorize filesystem access.
pub fn valid_reference_path(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "SKILL.md"
        && !name.starts_with("SKILL.md/")
        && name.split('/').all(|part| {
            !matches!(part, "" | "." | "..")
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}

#[async_trait]
pub trait SkillStore: Send + Sync {
    async fn defaults(&self, subject: &str) -> Result<SkillDefaults>;
    async fn put_defaults(&self, subject: &str, defaults: SkillDefaults) -> Result<SkillDefaults>;
    async fn list(&self) -> Result<Vec<Skill>>;
    /// Content is historical; audience always reflects current sharing policy.
    async fn get(&self, id: &str, revision: Option<i64>) -> Result<Option<Skill>>;
    async fn versions(&self, id: &str) -> Result<Vec<Skill>>;
    async fn put(&self, draft: SkillDraft) -> Result<Skill>;
    /// Remove all content while reserving the identifier against pin reuse.
    async fn delete(
        &self,
        id: &str,
        owner: &str,
        revision: i64,
        sharing_revision: i64,
    ) -> Result<()>;
    async fn share(
        &self,
        id: &str,
        expected_revision: i64,
        audience: SkillAudience,
        team: Option<String>,
    ) -> Result<Skill>;
}

#[derive(Default)]
pub struct InMemorySkills {
    rows: RwLock<BTreeMap<String, Vec<Skill>>>,
    defaults: RwLock<BTreeMap<String, SkillDefaults>>,
}

pub fn validate_sharing(audience: &SkillAudience, team: Option<&str>) -> Result<()> {
    match (audience, team) {
        (SkillAudience::Team, Some(team)) if crate::llm::valid_id(team) => Ok(()),
        (SkillAudience::Private | SkillAudience::Organization, None) => Ok(()),
        _ => Err(CoreError::Invalid("invalid skill sharing scope".into())),
    }
}

fn unavailable_lock() -> CoreError {
    CoreError::Storage("skill store unavailable".into())
}

#[async_trait]
impl SkillStore for InMemorySkills {
    async fn defaults(&self, subject: &str) -> Result<SkillDefaults> {
        Ok(self
            .defaults
            .read()
            .map_err(|_| unavailable_lock())?
            .get(subject)
            .cloned()
            .unwrap_or_default())
    }
    async fn put_defaults(
        &self,
        subject: &str,
        mut defaults: SkillDefaults,
    ) -> Result<SkillDefaults> {
        validate_selections(&defaults.skills)?;
        let mut rows = self.defaults.write().map_err(|_| unavailable_lock())?;
        if rows.get(subject).map_or(0, |d| d.revision) != defaults.revision {
            return Err(CoreError::Conflict(
                "skill defaults changed; reload before saving".into(),
            ));
        }
        defaults.revision = defaults
            .revision
            .checked_add(1)
            .ok_or_else(|| CoreError::Conflict("defaults revision exhausted".into()))?;
        rows.insert(subject.into(), defaults.clone());
        Ok(defaults)
    }
    async fn list(&self) -> Result<Vec<Skill>> {
        Ok(self
            .rows
            .read()
            .map_err(|_| unavailable_lock())?
            .values()
            .filter_map(|versions| versions.last().cloned())
            .collect())
    }
    async fn get(&self, id: &str, revision: Option<i64>) -> Result<Option<Skill>> {
        let rows = self.rows.read().map_err(|_| unavailable_lock())?;
        Ok(rows
            .get(id)
            .and_then(|versions| match revision {
                Some(revision) => versions.iter().find(|skill| skill.revision == revision),
                None => versions.last(),
            })
            .cloned())
    }
    async fn versions(&self, id: &str) -> Result<Vec<Skill>> {
        Ok(self
            .rows
            .read()
            .map_err(|_| unavailable_lock())?
            .get(id)
            .cloned()
            .unwrap_or_default())
    }
    async fn put(&self, draft: SkillDraft) -> Result<Skill> {
        draft.validate()?;
        let mut rows = self.rows.write().map_err(|_| unavailable_lock())?;
        if rows.get(&draft.id).is_some_and(Vec::is_empty) {
            return Err(CoreError::NotFound("skill unavailable".into()));
        }
        let versions = rows.entry(draft.id.clone()).or_default();
        let previous = versions.last();
        if previous.is_some_and(|skill| skill.owner != draft.owner) {
            return Err(CoreError::Unauthorized("skill owner required".into()));
        }
        if previous.map_or(0, |skill| skill.revision) != draft.expected_revision {
            return Err(CoreError::Conflict(
                "skill changed; reload before saving".into(),
            ));
        }
        let skill = Skill {
            id: draft.id,
            owner: draft.owner.clone(),
            title: draft.title,
            description: draft.description,
            revision: draft
                .expected_revision
                .checked_add(1)
                .ok_or_else(|| CoreError::Conflict("skill revision exhausted".into()))?,
            sharing_revision: previous.map_or(1, |skill| skill.sharing_revision),
            audience: previous.map_or(SkillAudience::Private, |skill| skill.audience.clone()),
            team: previous.and_then(|skill| skill.team.clone()),
            instructions: draft.instructions,
            references: draft.references,
            author: draft.owner,
            created_at_ms: draft.created_at_ms,
        };
        versions.push(skill.clone());
        Ok(skill)
    }
    async fn delete(
        &self,
        id: &str,
        owner: &str,
        revision: i64,
        sharing_revision: i64,
    ) -> Result<()> {
        let mut rows = self.rows.write().map_err(|_| unavailable_lock())?;
        let versions = rows
            .get_mut(id)
            .ok_or_else(|| CoreError::NotFound("skill unavailable".into()))?;
        let latest = versions
            .last()
            .ok_or_else(|| CoreError::NotFound("skill unavailable".into()))?;
        if latest.owner != owner {
            return Err(CoreError::Unauthorized("skill owner required".into()));
        }
        if latest.revision != revision || latest.sharing_revision != sharing_revision {
            return Err(CoreError::Conflict(
                "skill changed; reload before deleting".into(),
            ));
        }
        // An empty history is a tombstone, so old pins cannot resolve to new content.
        versions.clear();
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
        let mut rows = self.rows.write().map_err(|_| unavailable_lock())?;
        let versions = rows
            .get_mut(id)
            .ok_or_else(|| CoreError::NotFound("skill unavailable".into()))?;
        let latest = versions
            .last()
            .ok_or_else(|| CoreError::NotFound("skill unavailable".into()))?;
        if latest.sharing_revision != expected_revision {
            return Err(CoreError::Conflict(
                "skill sharing changed; reload before saving".into(),
            ));
        }
        let revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| CoreError::Conflict("skill sharing revision exhausted".into()))?;
        for skill in versions.iter_mut() {
            skill.sharing_revision = revision;
            skill.audience = audience.clone();
            skill.team = team.clone();
        }
        Ok(versions.last().expect("nonempty skill history").clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft(expected_revision: i64) -> SkillDraft {
        SkillDraft {
            id: "skill-1".into(),
            owner: "alice".into(),
            title: "SQL review".into(),
            description: String::new(),
            instructions: format!("Review version {}", expected_revision + 1),
            references: vec![],
            expected_revision,
            created_at_ms: 42,
        }
    }
    #[tokio::test]
    async fn skill_revisions_are_immutable_and_edits_conflict() {
        let store = InMemorySkills::default();
        assert!(
            store.put(draft(0)).await.is_ok(),
            "first skill registration must succeed"
        );
        let second = store.put(draft(1)).await.unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(
            store
                .get("skill-1", Some(1))
                .await
                .unwrap()
                .unwrap()
                .instructions,
            "Review version 1"
        );
        assert!(matches!(
            store.put(draft(1)).await,
            Err(CoreError::Conflict(_))
        ));
        let mut wrong_owner = draft(2);
        wrong_owner.owner = "bob".into();
        assert!(matches!(
            store.put(wrong_owner).await,
            Err(CoreError::Unauthorized(_))
        ));
        assert_eq!(store.versions("skill-1").await.unwrap().len(), 2);
    }
    #[tokio::test]
    async fn historical_skill_reads_use_current_sharing() {
        let store = InMemorySkills::default();
        assert!(
            store.put(draft(0)).await.is_ok(),
            "first skill registration must succeed"
        );
        store.put(draft(1)).await.unwrap();
        let shared = store
            .share("skill-1", 1, SkillAudience::Team, Some("analytics".into()))
            .await
            .unwrap();
        assert_eq!(shared.sharing_revision, 2);
        assert_eq!(
            store
                .get("skill-1", Some(1))
                .await
                .unwrap()
                .unwrap()
                .audience,
            SkillAudience::Team
        );
        assert!(matches!(
            store
                .share("skill-1", 1, SkillAudience::Private, None)
                .await,
            Err(CoreError::Conflict(_))
        ));
        store
            .share("skill-1", 2, SkillAudience::Private, None)
            .await
            .unwrap();
        assert_eq!(
            store
                .get("skill-1", Some(1))
                .await
                .unwrap()
                .unwrap()
                .audience,
            SkillAudience::Private
        );
    }
    #[test]
    fn skill_references_have_aggregate_bounds_and_no_paths() {
        let mut value = draft(0);
        value.references = vec![SkillReference {
            name: "../secret".into(),
            content: "x".into(),
        }];
        assert!(value.validate().is_err());
        value.references = vec![SkillReference {
            name: "context.md".into(),
            content: "x".repeat(MAX_SKILL_BYTES),
        }];
        assert!(value.validate().is_err());
        value.references[0].content = "Text only".into();
        assert!(value.validate().is_ok());
        value.title = "SQL\0review".into();
        assert!(
            value.validate().is_err(),
            "skill titles must reject NUL for PostgreSQL JSONB"
        );
    }
}
