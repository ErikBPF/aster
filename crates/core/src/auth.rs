use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Editor,
    Admin,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Role::Viewer => "viewer",
            Role::Editor => "editor",
            Role::Admin => "admin",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub subject: String,
    pub roles: Vec<Role>,
    /// Exact verified group claim strings, never request input. They may be
    /// names or paths; shared grants need a proven stable current mapping.
    #[serde(default)]
    pub groups: Vec<String>,
    /// Stable signed user UUID. Old sessions and development identities have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_uuid: Option<String>,
}

/// One administrator grant for use of a shared model. This only matches
/// claims: shared use must separately prove current membership or revocation.
/// Groups match their verified identifier exactly; roles use Aster's hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedModelGrant {
    Group(String),
    Role(Role),
}

impl SharedModelGrant {
    pub fn allows(&self, principal: &Principal) -> bool {
        match self {
            Self::Group(group) => {
                !group.is_empty() && principal.groups.iter().any(|id| id == group)
            }
            Self::Role(role) => principal.roles.iter().any(|held| held >= role),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ReadNotebook,
    WriteNotebook,
    RunQuery,
    Administer,
}

impl Action {
    fn minimum_role(self) -> Role {
        match self {
            Action::ReadNotebook => Role::Viewer,
            Action::WriteNotebook => Role::Editor,
            Action::RunQuery => Role::Editor,
            Action::Administer => Role::Admin,
        }
    }
}

/// Single centralized authorization check. Keep every policy decision here so
/// handlers never encode role logic themselves.
pub fn authorize(principal: &Principal, action: Action) -> Result<()> {
    let required = action.minimum_role();
    if principal.roles.iter().any(|role| *role >= required) {
        Ok(())
    } else {
        Err(CoreError::Unauthorized(format!(
            "{} may not perform {:?}",
            principal.subject, action
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_cannot_run_queries() {
        let principal = Principal {
            subject: "alice".into(),
            roles: vec![Role::Viewer],
            groups: vec![],
            user_uuid: None,
        };
        assert!(authorize(&principal, Action::ReadNotebook).is_ok());
        assert!(authorize(&principal, Action::RunQuery).is_err());
        assert!(authorize(&principal, Action::Administer).is_err());
    }

    #[test]
    fn admin_can_do_everything() {
        let principal = Principal {
            subject: "root".into(),
            roles: vec![Role::Admin],
            groups: vec![],
            user_uuid: None,
        };
        for action in [
            Action::ReadNotebook,
            Action::WriteNotebook,
            Action::RunQuery,
            Action::Administer,
        ] {
            assert!(authorize(&principal, action).is_ok());
        }
    }

    #[test]
    fn shared_model_grants_match_verified_groups_or_role_hierarchy() {
        let analyst = Principal {
            subject: "alice".into(),
            roles: vec![Role::Viewer],
            groups: vec!["/org/analysts".into()],
            user_uuid: None,
        };
        let group = SharedModelGrant::Group("/org/analysts".into());
        assert!(group.allows(&analyst));
        assert!(!SharedModelGrant::Group("analysts".into()).allows(&analyst));
        assert!(!SharedModelGrant::Group(String::new()).allows(&analyst));
        assert!(!SharedModelGrant::Role(Role::Editor).allows(&analyst));

        let admin = Principal {
            subject: "bob".into(),
            roles: vec![Role::Admin],
            groups: vec![],
            user_uuid: None,
        };
        assert!(!group.allows(&admin));
        assert!(SharedModelGrant::Role(Role::Editor).allows(&admin));

        let old: Principal = serde_json::from_value(serde_json::json!({
            "subject":"charlie", "roles":["editor"]
        }))
        .unwrap();
        assert!(old.groups.is_empty());
        assert!(!group.allows(&old));
    }
}
