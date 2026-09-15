use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Editor,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub subject: String,
    pub roles: Vec<Role>,
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
}
