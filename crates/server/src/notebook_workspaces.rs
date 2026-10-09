//! Team-scoped local notebook branches. A target is usable only when a
//! server-owned policy supplies a disposable local repository fixture. GitHub
//! activation belongs to the later scoped-App-credential slice.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use aster_core::{CoreError, NotebookStore, Principal, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::GitNotebookStore;

#[derive(Clone, Debug, Deserialize)]
pub struct TeamPolicy {
    pub member_claim: String,
    pub maintainer_claim: String,
    pub allowed_repositories: HashSet<String>,
    /// Only test fixtures set this. Production target configuration stays
    /// pending until a GitHub App installation verifies the default ref.
    #[serde(skip)]
    pub local_repositories: HashMap<String, PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TeamTarget {
    pub repository: String,
    pub default_branch: String,
    pub default_commit: Option<String>,
    pub version: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct TargetState {
    targets: HashMap<String, TeamTarget>,
    events: Vec<TargetEvent>,
}

#[derive(Serialize, Deserialize)]
struct TargetEvent {
    team: String,
    actor: String,
    before: Option<TeamTarget>,
    after: TeamTarget,
}

pub struct TeamWorkspaces {
    root: PathBuf,
    policy: HashMap<String, TeamPolicy>,
    active: Mutex<HashMap<String, Arc<GitNotebookStore>>>,
}

impl TeamWorkspaces {
    pub(crate) fn contains_team(&self, team: &str) -> bool {
        self.policy.contains_key(team)
    }

    pub fn new(root: impl Into<PathBuf>, policy: HashMap<String, TeamPolicy>) -> Result<Self> {
        let root = root.into();
        for (team, rule) in &policy {
            safe_team(team)?;
            if rule.member_claim.is_empty() || rule.maintainer_claim.is_empty() {
                return Err(CoreError::Invalid("team claim cannot be empty".into()));
            }
            if rule.allowed_repositories.is_empty()
                || rule
                    .allowed_repositories
                    .iter()
                    .any(|repo| !safe_repository(repo))
            {
                return Err(CoreError::Invalid("invalid allowed team repository".into()));
            }
            if rule
                .local_repositories
                .keys()
                .any(|repo| !rule.allowed_repositories.contains(repo))
            {
                return Err(CoreError::Invalid(
                    "fixture repository is not allowed".into(),
                ));
            }
        }
        std::fs::create_dir_all(&root).map_err(storage)?;
        read_targets(&root)?;
        Ok(Self {
            root,
            policy,
            active: Mutex::new(HashMap::new()),
        })
    }

    pub fn member(&self, team: &str, principal: &Principal) -> Result<()> {
        let rule = self
            .policy
            .get(team)
            .ok_or_else(|| CoreError::Unauthorized("unknown team".into()))?;
        if principal
            .groups
            .iter()
            .any(|claim| claim == &rule.member_claim)
        {
            Ok(())
        } else {
            Err(CoreError::Unauthorized("team membership required".into()))
        }
    }

    pub fn maintainer(&self, team: &str, principal: &Principal) -> Result<()> {
        self.member(team, principal)?;
        let rule = &self.policy[team];
        if principal
            .groups
            .iter()
            .any(|claim| claim == &rule.maintainer_claim)
        {
            Ok(())
        } else {
            Err(CoreError::Unauthorized(
                "team maintainer claim required".into(),
            ))
        }
    }

    pub fn configure(
        &self,
        team: &str,
        repository: &str,
        default_branch: &str,
        expected: Option<u64>,
        actor: &str,
    ) -> Result<TeamTarget> {
        let rule = self
            .policy
            .get(team)
            .ok_or_else(|| CoreError::Unauthorized("unknown team".into()))?;
        if !rule.allowed_repositories.contains(repository) || !safe_repository(repository) {
            return Err(CoreError::Unauthorized(
                "repository is not allowed for team".into(),
            ));
        }
        if !safe_ref(default_branch) {
            return Err(CoreError::Invalid("invalid default branch".into()));
        }
        let default_commit = rule
            .local_repositories
            .get(repository)
            .map(|path| {
                git(
                    path,
                    &[
                        "rev-parse",
                        "--verify",
                        &format!("refs/heads/{default_branch}^{{commit}}"),
                    ],
                )
            })
            .transpose()?
            .map(|value| value.trim().to_owned());
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join(".aster-targets.lock"))
            .map_err(storage)?;
        lock.lock().map_err(storage)?;
        let mut state = read_targets(&self.root)?;
        let before = state.targets.get(team).cloned();
        if before.as_ref().map(|target| target.version) != expected {
            return Err(CoreError::Conflict("team target version changed".into()));
        }
        let version = before.as_ref().map_or(1, |prior| prior.version + 1);
        let target = TeamTarget {
            repository: repository.into(),
            default_branch: default_branch.into(),
            default_commit,
            version,
        };
        state.targets.insert(team.into(), target.clone());
        state.events.push(TargetEvent {
            team: team.into(),
            actor: actor.into(),
            before,
            after: target.clone(),
        });
        write_targets(&self.root, &state)?;
        lock.unlock().map_err(storage)?;
        Ok(target)
    }

    pub fn target(&self, team: &str) -> Result<Option<TeamTarget>> {
        Ok(read_targets(&self.root)?.targets.get(team).cloned())
    }

    pub fn workspace(
        &self,
        team: &str,
        principal: &Principal,
        sid: &str,
        personal: bool,
    ) -> Result<Arc<GitNotebookStore>> {
        let branch = self.branch_for(team, principal, sid, personal)?;
        let key = format!("{team}/{branch}");
        let mut active = self
            .active
            .lock()
            .map_err(|_| CoreError::Storage("workspaces poisoned".into()))?;
        if let Some(store) = active.get(&key) {
            return Ok(store.clone());
        }
        let init_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join(".aster-workspaces.lock"))
            .map_err(storage)?;
        init_lock.lock().map_err(storage)?;
        let dir = self.root.join(team).join(&branch);
        let target = if dir.exists() {
            read_binding(&dir)?
        } else {
            self.target(team)?.ok_or_else(|| {
                CoreError::Conflict("team notebook target is not configured".into())
            })?
        };
        if target.default_commit.is_none() {
            return Err(CoreError::Conflict(
                "team notebook target is pending GitHub verification".into(),
            ));
        }
        let repo = self.policy[team]
            .local_repositories
            .get(&target.repository)
            .ok_or_else(|| CoreError::Conflict("team notebook target is not active".into()))?;
        if !dir.exists() {
            let parent = dir
                .parent()
                .ok_or_else(|| CoreError::Storage("invalid workspace path".into()))?;
            std::fs::create_dir_all(parent).map_err(storage)?;
            let destination = dir
                .to_str()
                .ok_or_else(|| CoreError::Invalid("invalid workspace path".into()))?;
            git(
                repo,
                &[
                    "clone",
                    "--no-checkout",
                    "--branch",
                    &target.default_branch,
                    repo.to_str()
                        .ok_or_else(|| CoreError::Invalid("invalid fixture path".into()))?,
                    destination,
                ],
            )?;
            let pinned = target.default_commit.as_deref().unwrap();
            git(&dir, &["cat-file", "-e", &format!("{pinned}^{{commit}}")])?;
            git(&dir, &["checkout", "-q", "--detach", pinned])?;
        }
        let store = Arc::new(GitNotebookStore::open(&dir, &branch)?);
        if !dir.join(".git/aster.workspace.json").exists() {
            write_binding(&dir, &target)?;
        }
        active.insert(key, store.clone());
        init_lock.unlock().map_err(storage)?;
        Ok(store)
    }

    /// Internal metadata namespace. The caller chooses only personal/session;
    /// the team, subject and session are validated and resolved server-side.
    fn context_key(
        &self,
        team: &str,
        principal: &Principal,
        sid: &str,
        personal: bool,
    ) -> Result<String> {
        let branch = self.branch_for(team, principal, sid, personal)?;
        Ok(opaque_key(
            b"aster-notebook-context-v1\0",
            format!("{team}\0{branch}").as_bytes(),
        ))
    }

    pub fn notebook_context_key(
        &self,
        team: &str,
        principal: &Principal,
        sid: &str,
        personal: bool,
        notebook: &str,
    ) -> Result<String> {
        if !aster_core::llm::valid_id(notebook) {
            return Err(CoreError::Invalid("invalid notebook ID".into()));
        }
        let context = self.context_key(team, principal, sid, personal)?;
        Ok(opaque_key(
            b"aster-notebook-metadata-v1\0",
            format!("{context}\0{notebook}").as_bytes(),
        ))
    }

    fn branch_for(
        &self,
        team: &str,
        principal: &Principal,
        sid: &str,
        personal: bool,
    ) -> Result<String> {
        self.member(team, principal)?;
        if sid.len() != 32 || !sid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CoreError::Unauthorized("invalid session".into()));
        }
        if principal.subject.is_empty() || principal.subject.len() > 256 {
            return Err(CoreError::Invalid(
                "invalid subject for notebook workspace".into(),
            ));
        }
        let subject = opaque_key(b"aster-notebook-subject-v1\0", principal.subject.as_bytes());
        Ok(if personal {
            format!("personal/{subject}")
        } else {
            format!(
                "sessions/{subject}/{}",
                opaque_key(b"aster-notebook-session-v1\0", sid.as_bytes())
            )
        })
    }

    /// Lists opaque prior session keys for this verified subject. The current
    /// session is excluded; recovery never accepts a Git ref from the client.
    pub fn recoverable(
        &self,
        team: &str,
        principal: &Principal,
        current_sid: &str,
    ) -> Result<Vec<String>> {
        self.member(team, principal)?;
        let subject = opaque_key(b"aster-notebook-subject-v1\0", principal.subject.as_bytes());
        let current = opaque_key(b"aster-notebook-session-v1\0", current_sid.as_bytes());
        let directory = self.root.join(team).join("sessions").join(subject);
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(storage(error)),
        };
        let mut sessions = Vec::new();
        for entry in entries {
            let entry = entry.map_err(storage)?;
            let key = entry.file_name().to_string_lossy().to_string();
            if key != current && safe_workspace_key(&key) && entry.path().is_dir() {
                read_binding(&entry.path())?;
                sessions.push(key);
            }
        }
        sessions.sort();
        Ok(sessions)
    }

    pub fn recovery_workspace(
        &self,
        team: &str,
        principal: &Principal,
        key: &str,
    ) -> Result<Arc<GitNotebookStore>> {
        self.member(team, principal)?;
        if !safe_workspace_key(key) {
            return Err(CoreError::Invalid("invalid recovery key".into()));
        }
        let subject = opaque_key(b"aster-notebook-subject-v1\0", principal.subject.as_bytes());
        let branch = format!("sessions/{subject}/{key}");
        let cache_key = format!("{team}/{branch}");
        let mut active = self
            .active
            .lock()
            .map_err(|_| CoreError::Storage("workspaces poisoned".into()))?;
        if let Some(store) = active.get(&cache_key) {
            return Ok(store.clone());
        }
        let dir = self.root.join(team).join(&branch);
        if !dir.is_dir() {
            return Err(CoreError::NotFound("recovery workspace not found".into()));
        }
        read_binding(&dir)?;
        let store = Arc::new(GitNotebookStore::open(&dir, &branch)?);
        active.insert(cache_key, store.clone());
        Ok(store)
    }

    /// Sync only the branch derived from this authenticated session or its
    /// explicit Personal choice. The remote comes from the checkout's original
    /// server-owned binding, never from the request or current team target.
    pub async fn sync_local_fixture(
        &self,
        team: &str,
        principal: &Principal,
        sid: &str,
        personal: bool,
        notebook_id: &str,
    ) -> Result<String> {
        let store = self.workspace(team, principal, sid, personal)?;
        store.snapshot(notebook_id).await?;
        let subject = opaque_key(b"aster-notebook-subject-v1\0", principal.subject.as_bytes());
        let branch = if personal {
            format!("personal/{subject}")
        } else {
            format!(
                "sessions/{subject}/{}",
                opaque_key(b"aster-notebook-session-v1\0", sid.as_bytes())
            )
        };
        let binding = read_binding(&self.root.join(team).join(branch))?;
        let remote = self.policy[team]
            .local_repositories
            .get(&binding.repository)
            .ok_or_else(|| CoreError::Conflict("bound notebook repository is not active".into()))?;
        store.sync_local_fixture(remote).await
    }
}

fn safe_workspace_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn read_binding(dir: &Path) -> Result<TeamTarget> {
    let bytes = std::fs::read(dir.join(".git/aster.workspace.json")).map_err(|_| {
        CoreError::Storage("workspace binding missing; administrator recovery required".into())
    })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        CoreError::Storage("workspace binding invalid; administrator recovery required".into())
    })
}

fn write_binding(dir: &Path, target: &TeamTarget) -> Result<()> {
    let bytes =
        serde_json::to_vec(target).map_err(|error| CoreError::Storage(error.to_string()))?;
    let admin = dir.join(".git");
    let temp = admin.join(format!("aster.workspace-{}.tmp", aster_core::new_sid()?));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(storage)?;
    file.write_all(&bytes).map_err(storage)?;
    file.sync_all().map_err(storage)?;
    std::fs::rename(&temp, admin.join("aster.workspace.json")).map_err(storage)?;
    File::open(admin)
        .and_then(|directory| directory.sync_all())
        .map_err(storage)?;
    Ok(())
}

fn read_targets(root: &Path) -> Result<TargetState> {
    match std::fs::read(root.join(".aster-targets.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| CoreError::Storage("invalid team target metadata".into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(TargetState::default()),
        Err(error) => Err(storage(error)),
    }
}

fn write_targets(root: &Path, state: &TargetState) -> Result<()> {
    let bytes = serde_json::to_vec(state).map_err(|error| CoreError::Storage(error.to_string()))?;
    let temp = root.join(format!(".aster-targets-{}.tmp", aster_core::new_sid()?));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(storage)?;
    file.write_all(&bytes).map_err(storage)?;
    file.sync_all().map_err(storage)?;
    std::fs::rename(&temp, root.join(".aster-targets.json")).map_err(storage)?;
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(storage)?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn opaque_key(domain: &[u8], value: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(value);
    hex(&digest.finalize())
}

fn safe_team(team: &str) -> Result<()> {
    if team.is_empty()
        || team.len() > 64
        || !team
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(CoreError::Invalid("invalid team id".into()));
    }
    Ok(())
}

fn safe_repository(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !name.is_empty()
        && repo.len() <= 160
        && owner
            .bytes()
            .chain(name.bytes())
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

fn safe_ref(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= 128
        && !branch.starts_with('-')
        && !branch.ends_with('/')
        && branch.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with('.')
                && !part.ends_with(".lock")
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        })
}

fn storage(error: std::io::Error) -> CoreError {
    CoreError::Storage(error.to_string())
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    let output = command.output().map_err(storage)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(CoreError::Storage(format!(
            "git {}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}
