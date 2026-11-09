//! Request-time policy for personal model registration and generation.
use crate::AppState;
use aster_core::{authorize, Action, CoreError, PersonalModelPolicy, Principal, Result};
use axum::http::HeaderMap;

pub(crate) async fn current(state: &AppState, headers: &HeaderMap) -> Result<Principal> {
    crate::current_identity::current_principal(state, headers, true)
        .await
        .map_err(|error| error.0)
}

pub(crate) fn team_member(state: &AppState, principal: &Principal, team: &str) -> bool {
    if let Some(teams) = &state.team_git_targets {
        teams.member(team, principal).is_ok()
    } else {
        state
            .team_workspaces
            .as_ref()
            .is_some_and(|teams| teams.member(team, principal).is_ok())
    }
}
pub(crate) async fn allowed(state: &AppState, principal: &Principal) -> Result<bool> {
    if !state.personal_models_enabled || authorize(principal, Action::WriteNotebook).is_err() {
        return Ok(false);
    }
    let policy = state.personal_model_policy.get().await?;
    Ok(policy.subjects.contains(&principal.subject)
        || policy
            .teams
            .iter()
            .any(|team| team_member(state, principal, team)))
}
pub(crate) async fn require(state: &AppState, headers: &HeaderMap) -> Result<Principal> {
    let principal = current(state, headers).await?;
    if !allowed(state, &principal).await? {
        return Err(CoreError::Unauthorized(
            "personal model permission required".into(),
        ));
    }
    Ok(principal)
}
pub(crate) fn destination(state: &AppState, url: &str) -> Result<()> {
    if let Some(policy) = &state.personal_model_destinations {
        return policy.validate(url);
    }
    if state.identity.is_none() && state.dev_login {
        return Ok(());
    }
    Err(CoreError::Unauthorized(
        "personal model destination is not approved".into(),
    ))
}
pub(crate) async fn replace(
    state: &AppState,
    headers: &HeaderMap,
    expected: i64,
    subjects: Vec<String>,
    teams: Vec<String>,
) -> Result<PersonalModelPolicy> {
    let principal = current(state, headers).await?;
    authorize(&principal, Action::Administer)?;
    for team in &teams {
        let exists = state
            .team_git_targets
            .as_ref()
            .is_some_and(|registry| registry.contains_team(team))
            || state
                .team_workspaces
                .as_ref()
                .is_some_and(|registry| registry.contains_team(team));
        if !exists {
            return Err(CoreError::Invalid("unknown personal-model team".into()));
        }
    }
    state
        .personal_model_policy
        .replace(expected, subjects, teams, &principal.subject)
        .await
}
