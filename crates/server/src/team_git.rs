//! Verified team Git targets. Workspace activation remains a separate gate.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aster_core::{CoreError, Principal, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::store::{PgStore, TeamGitTargetChange, TeamGitTargetRecord};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamGitPolicy {
    pub member_group_uuid: String,
    pub maintainer_group_uuid: String,
    pub installation_id: u64,
    pub allowed_repositories: HashMap<String, u64>,
}

#[async_trait]
pub trait TeamGitVerifier: Send + Sync {
    async fn verified_default_commit(
        &self,
        repository: &str,
        installation_id: u64,
        repository_id: u64,
        default_branch: &str,
    ) -> Result<String>;
}

#[derive(Clone, Serialize)]
pub struct TeamGitTargetView {
    pub repository: String,
    pub default_branch: String,
    pub default_commit: String,
    pub version: u64,
    pub active: bool,
}

pub struct TeamGitTargets {
    store: Arc<PgStore>,
    policy: HashMap<String, TeamGitPolicy>,
    verifier: Arc<dyn TeamGitVerifier>,
}

#[async_trait]
impl TeamGitVerifier for crate::github_app::GithubApp {
    async fn verified_default_commit(
        &self,
        repository: &str,
        installation_id: u64,
        repository_id: u64,
        default_branch: &str,
    ) -> Result<String> {
        crate::github_app::GithubApp::verified_default_commit(
            self,
            repository,
            installation_id,
            repository_id,
            default_branch,
        )
        .await
    }
}

fn view(record: TeamGitTargetRecord) -> Result<TeamGitTargetView> {
    Ok(TeamGitTargetView {
        repository: record.repository,
        default_branch: record.default_branch,
        default_commit: record.default_commit,
        version: u64::try_from(record.version)
            .map_err(|_| CoreError::Storage("invalid team Git target version".into()))?,
        active: false,
    })
}

fn validate_policy(policy: &HashMap<String, TeamGitPolicy>) -> Result<()> {
    if policy.is_empty()
        || policy.iter().any(|(team, rule)| {
            team.is_empty()
                || team.len() > 128
                || !team
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                || crate::current_identity::uuid(&rule.member_group_uuid).as_deref()
                    != Some(rule.member_group_uuid.as_str())
                || crate::current_identity::uuid(&rule.maintainer_group_uuid).as_deref()
                    != Some(rule.maintainer_group_uuid.as_str())
                || rule.member_group_uuid == rule.maintainer_group_uuid
                || rule.installation_id == 0
                || rule.allowed_repositories.is_empty()
                || rule.allowed_repositories.values().any(|id| *id == 0)
        })
    {
        return Err(CoreError::Invalid("invalid team Git policy".into()));
    }
    let mut repositories = HashSet::new();
    if policy.values().any(|rule| {
        rule.allowed_repositories
            .values()
            .any(|id| !repositories.insert(*id))
    }) {
        return Err(CoreError::Invalid(
            "repository ID belongs to multiple team targets".into(),
        ));
    }
    Ok(())
}

impl TeamGitTargets {
    pub(crate) fn contains_team(&self, team: &str) -> bool {
        self.policy.contains_key(team)
    }

    pub async fn connect(
        database_url: &str,
        policy: HashMap<String, TeamGitPolicy>,
        verifier: Arc<dyn TeamGitVerifier>,
    ) -> Result<Self> {
        validate_policy(&policy)?;
        Ok(Self {
            store: Arc::new(PgStore::connect(database_url).await?),
            policy,
            verifier,
        })
    }

    pub async fn configure(
        &self,
        team: &str,
        principal: &Principal,
        repository: &str,
        default_branch: &str,
        expected: Option<u64>,
    ) -> Result<TeamGitTargetView> {
        let rule = self.maintainer_policy(team, principal)?;
        if !crate::github_app::safe_branch(default_branch) {
            return Err(CoreError::Invalid("invalid default branch".into()));
        }
        let repository_id = *rule
            .allowed_repositories
            .get(repository)
            .ok_or_else(|| CoreError::Unauthorized("team repository not allowed".into()))?;
        let commit = self
            .verifier
            .verified_default_commit(
                repository,
                rule.installation_id,
                repository_id,
                default_branch,
            )
            .await?;
        let change = TeamGitTargetChange {
            team: team.into(),
            repository: repository.into(),
            repository_id: i64::try_from(repository_id)
                .map_err(|_| CoreError::Invalid("invalid repository ID".into()))?,
            installation_id: i64::try_from(rule.installation_id)
                .map_err(|_| CoreError::Invalid("invalid installation ID".into()))?,
            default_branch: default_branch.into(),
            default_commit: commit,
        };
        let record = self
            .store
            .change_team_git_target(
                &change,
                expected
                    .map(i64::try_from)
                    .transpose()
                    .map_err(|_| CoreError::Invalid("invalid target version".into()))?,
                &principal.subject,
            )
            .await?;
        view(record)
    }

    pub async fn get(&self, team: &str, principal: &Principal) -> Result<TeamGitTargetView> {
        self.member_policy(team, principal)?;
        let record = self
            .store
            .team_git_target(team)
            .await?
            .ok_or_else(|| CoreError::NotFound("team notebook target not configured".into()))?;
        view(record)
    }

    pub fn maintainer(&self, team: &str, principal: &Principal) -> Result<()> {
        self.maintainer_policy(team, principal).map(|_| ())
    }

    pub fn member(&self, team: &str, principal: &Principal) -> Result<()> {
        self.member_policy(team, principal).map(|_| ())
    }

    fn maintainer_policy(&self, team: &str, principal: &Principal) -> Result<&TeamGitPolicy> {
        let rule = self.member_policy(team, principal)?;
        if !principal
            .groups
            .iter()
            .any(|group| group == &rule.maintainer_group_uuid)
        {
            return Err(CoreError::Unauthorized("team maintainer required".into()));
        }
        Ok(rule)
    }

    fn member_policy(&self, team: &str, principal: &Principal) -> Result<&TeamGitPolicy> {
        let rule = self
            .policy
            .get(team)
            .ok_or_else(|| CoreError::NotFound("unknown team".into()))?;
        if principal
            .user_uuid
            .as_deref()
            .and_then(crate::current_identity::uuid)
            .is_none()
            || !principal
                .groups
                .iter()
                .any(|group| group == &rule.member_group_uuid)
        {
            return Err(CoreError::Unauthorized(
                "current team membership required".into(),
            ));
        }
        Ok(rule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_core::Role;

    struct FakeVerifier;

    #[test]
    fn one_repository_id_cannot_be_assigned_to_two_team_policies() {
        let rule = TeamGitPolicy {
            member_group_uuid: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
            maintainer_group_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            installation_id: 41,
            allowed_repositories: HashMap::from([("example/shared".into(), 73)]),
        };
        let policy = HashMap::from([("alpha".into(), rule.clone()), ("beta".into(), rule)]);
        assert!(matches!(
            validate_policy(&policy),
            Err(CoreError::Invalid(_))
        ));
    }

    #[async_trait]
    impl TeamGitVerifier for FakeVerifier {
        async fn verified_default_commit(
            &self,
            repository: &str,
            installation_id: u64,
            repository_id: u64,
            default_branch: &str,
        ) -> Result<String> {
            assert_eq!(repository, "example/alpha");
            assert_eq!(installation_id, 41);
            assert_eq!(repository_id, 73);
            assert_eq!(default_branch, "main");
            Ok("a".repeat(40))
        }
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL from notebook-team-target-postgres.sh"]
    async fn postgres_team_target_verified_maintainer_configures_pending_target() {
        let url = std::env::var("ASTER_TEST_METADATA_URL").unwrap();
        let targets = TeamGitTargets::connect(
            &url,
            HashMap::from([(
                "alpha".into(),
                TeamGitPolicy {
                    member_group_uuid: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
                    maintainer_group_uuid: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
                    installation_id: 41,
                    allowed_repositories: HashMap::from([("example/alpha".into(), 73)]),
                },
            )]),
            Arc::new(FakeVerifier),
        )
        .await
        .unwrap();
        let principal = Principal {
            subject: "alice".into(),
            roles: vec![Role::Viewer],
            groups: vec![
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            ],
            user_uuid: Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".into()),
        };
        let target = targets
            .configure("alpha", &principal, "example/alpha", "main", None)
            .await
            .unwrap();
        assert_eq!(target.repository, "example/alpha");
        assert_eq!(target.default_commit, "a".repeat(40));
        assert_eq!(target.version, 1);
        assert!(!target.active);
        assert!(matches!(
            targets
                .configure("alpha", &principal, "example/alpha", "main?bad", Some(1))
                .await,
            Err(CoreError::Invalid(_))
        ));
        let read = targets.get("alpha", &principal).await.unwrap();
        assert_eq!(read.default_commit, target.default_commit);
        assert!(matches!(
            targets
                .configure("alpha", &principal, "example/alpha", "main", None)
                .await,
            Err(CoreError::Conflict(_))
        ));
        let member = Principal {
            groups: vec!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()],
            ..principal.clone()
        };
        assert!(matches!(
            targets
                .configure("alpha", &member, "example/alpha", "main", Some(1))
                .await,
            Err(CoreError::Unauthorized(_))
        ));
        assert!(matches!(
            targets
                .configure("alpha", &principal, "example/other", "main", Some(1))
                .await,
            Err(CoreError::Unauthorized(_))
        ));
    }
}
