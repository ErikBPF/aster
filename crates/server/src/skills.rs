//! Authenticated skill registration and current audience admission.
use crate::AppState;
use aster_core::skills::{Skill, SkillAudience, SkillDraft, SkillReference, SkillSelection};
use aster_core::{authorize, Action, CoreError, Principal, Result};
use axum::http::HeaderMap;

pub struct SkillView {
    pub skill: Skill,
    pub can_edit: bool,
}
fn denied() -> CoreError {
    CoreError::Unauthorized("skill unavailable".into())
}
fn validate_id(id: &str) -> Result<()> {
    if !aster_core::llm::valid_id(id) {
        return Err(CoreError::Invalid("invalid skill id".into()));
    }
    Ok(())
}
fn member(state: &AppState, principal: &Principal, team: &str) -> bool {
    if let Some(teams) = &state.team_git_targets {
        teams.member(team, principal).is_ok()
    } else {
        state
            .team_workspaces
            .as_ref()
            .is_some_and(|teams| teams.member(team, principal).is_ok())
    }
}
async fn visible(
    state: &AppState,
    headers: &HeaderMap,
    principal: &Principal,
    skill: &Skill,
) -> bool {
    if skill.owner == principal.subject {
        return true;
    }
    if skill.audience == SkillAudience::Private {
        return false;
    }
    let Ok(current) = crate::current_identity::current_principal(state, headers, true).await else {
        return false;
    };
    visible_to_current(state, &current, skill)
}
fn visible_to_current(state: &AppState, current: &Principal, skill: &Skill) -> bool {
    match skill.audience {
        SkillAudience::Organization => true,
        SkillAudience::Team => skill
            .team
            .as_deref()
            .is_some_and(|team| member(state, current, team)),
        _ => false,
    }
}

async fn principal(state: &AppState, headers: &HeaderMap) -> Result<Principal> {
    let principal = crate::principal(state, headers)
        .await
        .map_err(|error| error.0)?;
    authorize(&principal, Action::ReadNotebook)?;
    Ok(principal)
}
fn view(skill: Skill, principal: &Principal) -> SkillView {
    let can_edit =
        skill.owner == principal.subject && authorize(principal, Action::WriteNotebook).is_ok();
    SkillView { skill, can_edit }
}
pub async fn list(state: &AppState, headers: &HeaderMap) -> Result<Vec<SkillView>> {
    let principal = principal(state, headers).await?;
    let skills = state.skills.list().await?;
    let current = if skills
        .iter()
        .any(|skill| skill.owner != principal.subject && skill.audience != SkillAudience::Private)
    {
        crate::current_identity::current_principal(state, headers, true)
            .await
            .ok()
    } else {
        None
    };
    let mut result = Vec::new();
    for mut skill in skills {
        if skill.owner == principal.subject
            || current
                .as_ref()
                .is_some_and(|current| visible_to_current(state, current, &skill))
        {
            skill.instructions.clear();
            skill.references.clear();
            result.push(view(skill, &principal));
        }
    }
    Ok(result)
}
pub async fn get(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    revision: Option<i64>,
) -> Result<SkillView> {
    validate_id(id)?;
    if revision.is_some_and(|revision| revision <= 0) {
        return Err(CoreError::Invalid("invalid skill revision".into()));
    }
    let principal = principal(state, headers).await?;
    let skill = state.skills.get(id, revision).await?.ok_or_else(denied)?;
    if !visible(state, headers, &principal, &skill).await {
        return Err(denied());
    }
    Ok(view(skill, &principal))
}
pub async fn resolve(
    state: &AppState,
    headers: &HeaderMap,
    selection: &SkillSelection,
) -> Result<Skill> {
    Ok(get(state, headers, &selection.id, Some(selection.revision))
        .await?
        .skill)
}
pub async fn versions(state: &AppState, headers: &HeaderMap, id: &str) -> Result<Vec<SkillView>> {
    validate_id(id)?;
    let principal = principal(state, headers).await?;
    let rows = state.skills.versions(id).await?;
    let latest = rows.last().ok_or_else(denied)?;
    if !visible(state, headers, &principal, latest).await {
        return Err(denied());
    }
    Ok(rows
        .into_iter()
        .map(|mut skill| {
            skill.instructions.clear();
            skill.references.clear();
            view(skill, &principal)
        })
        .collect())
}
#[allow(clippy::too_many_arguments)]
pub async fn put(
    state: &AppState,
    headers: &HeaderMap,
    id: String,
    expected_revision: i64,
    title: String,
    description: String,
    instructions: String,
    references: Vec<SkillReference>,
) -> Result<SkillView> {
    let principal = principal(state, headers).await?;
    authorize(&principal, Action::WriteNotebook)?;
    let id = if id.is_empty() {
        if expected_revision != 0 {
            return Err(CoreError::Invalid("new skill expects revision zero".into()));
        }
        aster_core::new_sid()?
    } else {
        // Existing identifiers cannot be claimed by another subject.
        let existing = get(state, headers, &id, None).await?;
        if existing.skill.owner != principal.subject {
            return Err(denied());
        }
        id
    };
    let skill = state
        .skills
        .put(SkillDraft {
            id,
            owner: principal.subject.clone(),
            title,
            description,
            instructions,
            references,
            expected_revision,
            created_at_ms: crate::now().saturating_mul(1000),
        })
        .await?;
    Ok(view(skill, &principal))
}
pub async fn share(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    expected_revision: i64,
    audience: SkillAudience,
    team: Option<String>,
) -> Result<SkillView> {
    let principal = principal(state, headers).await?;
    authorize(&principal, Action::WriteNotebook)?;
    let existing = get(state, headers, id, None).await?;
    if existing.skill.owner != principal.subject {
        return Err(denied());
    }
    match audience {
        SkillAudience::Organization => {
            let current = crate::current_identity::current_principal(state, headers, true)
                .await
                .map_err(|error| error.0)?;
            authorize(&current, Action::Administer)?;
        }
        SkillAudience::Team => {
            let current = crate::current_identity::current_principal(state, headers, false)
                .await
                .map_err(|error| error.0)?;
            authorize(&current, Action::WriteNotebook)?;
            if !team
                .as_deref()
                .is_some_and(|team| member(state, &current, team))
            {
                return Err(denied());
            }
        }
        SkillAudience::Private => {}
        _ => return Err(CoreError::Invalid("invalid skill audience".into())),
    }
    let skill = state
        .skills
        .share(id, expected_revision, audience, team)
        .await?;
    Ok(view(skill, &principal))
}

/// Destructive mutations require current identity and matching content/sharing state.
pub async fn delete(
    state: &AppState,
    headers: &HeaderMap,
    id: &str,
    revision: i64,
    sharing_revision: i64,
) -> Result<()> {
    validate_id(id)?;
    if revision <= 0 || sharing_revision <= 0 {
        return Err(CoreError::Invalid("invalid skill revision".into()));
    }
    let principal = crate::current_identity::current_principal(state, headers, true)
        .await
        .map_err(|error| error.0)?;
    authorize(&principal, Action::WriteNotebook)?;
    let existing = state.skills.get(id, None).await?.ok_or_else(denied)?;
    if existing.owner != principal.subject {
        return Err(denied());
    }
    state
        .skills
        .delete(id, &principal.subject, revision, sharing_revision)
        .await
}

pub async fn defaults(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<aster_core::skills::SkillDefaults> {
    let p = principal(state, headers).await?;
    state.skills.defaults(&p.subject).await
}
pub async fn put_defaults(
    state: &AppState,
    headers: &HeaderMap,
    defaults: aster_core::skills::SkillDefaults,
) -> Result<aster_core::skills::SkillDefaults> {
    let p = principal(state, headers).await?;
    aster_core::skills::validate_selections(&defaults.skills)?;
    if defaults.revision < 0 {
        return Err(CoreError::Invalid("invalid defaults revision".into()));
    }
    for pin in &defaults.skills {
        resolve(state, headers, pin).await?;
    }
    state.skills.put_defaults(&p.subject, defaults).await
}
