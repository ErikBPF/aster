use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use aster_core::{
    CoreError, Notebook, NotebookPrecondition, NotebookSave, NotebookSnapshot, NotebookStore,
    Result,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

const MAX_NOTEBOOK_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SyncMarker {
    destination: String,
    reference: String,
    local: String,
    pending: bool,
}

/// Immutable identity of a production team checkout. The token is deliberately
/// absent; the numeric IDs remain stable across repository rename or transfer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NotebookWorkspaceBinding {
    pub team: String,
    pub branch: String,
    pub installation_id: u64,
    pub repository_id: u64,
    pub default_branch: String,
    pub default_commit: String,
    pub target_version: u64,
    pub remote_url: String,
}

/// Git-backed notebook store for one checkout and branch. Clones share a lock
/// so local saves cannot interleave; an OS lock excludes other server processes.
///
/// ponytail: uses the installed Git CLI; explicit remote Sync will reuse this
/// checkout after branch ownership and credential boundaries are enforced.
#[derive(Clone)]
pub struct GitNotebookStore {
    dir: PathBuf,
    branch: String,
    source_id: String,
    transaction: Arc<Mutex<()>>,
    _checkout_lock: Arc<CheckoutLock>,
}

struct CheckoutLock(File);

impl Drop for CheckoutLock {
    fn drop(&mut self) {
        // Explicit unlock releases the open-file-description lock even if a
        // concurrently spawning child briefly inherited the descriptor.
        let _ = self.0.unlock();
    }
}

impl GitNotebookStore {
    /// Production team service entry. Check the durable binding before `open`
    /// can initialize metadata or check out any remote-controlled file.
    pub fn open_bound(
        dir: impl Into<PathBuf>,
        branch: impl Into<String>,
        expected: &NotebookWorkspaceBinding,
    ) -> Result<Self> {
        let dir = dir.into();
        let branch = branch.into();
        let binding = read_workspace_binding(&dir)?;
        if binding != *expected || binding.branch != branch {
            return Err(CoreError::Conflict(
                "notebook workspace binding changed".into(),
            ));
        }
        let configured_remote = git_in(&dir, &["config", "--local", "--get", "remote.origin.url"])?;
        if configured_remote.trim() != binding.remote_url {
            return Err(CoreError::Conflict(
                "notebook workspace remote changed".into(),
            ));
        }
        Self::open(dir, branch)
    }

    pub fn open(dir: impl Into<PathBuf>, branch: impl Into<String>) -> Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(storage)?;
        let dir = std::fs::canonicalize(dir).map_err(storage)?;
        let dot_git = dir.join(".git");
        let (admin_dir, needs_init) = match std::fs::symlink_metadata(&dot_git) {
            Ok(metadata) if metadata.is_dir() => (
                std::fs::canonicalize(&dot_git).map_err(storage)?,
                !dot_git.join("HEAD").exists(),
            ),
            Ok(metadata) if metadata.is_file() => {
                let path = git_in(&dir, &["rev-parse", "--absolute-git-dir"])?;
                (std::fs::canonicalize(path.trim()).map_err(storage)?, false)
            }
            Ok(_) => return Err(CoreError::Invalid("invalid Git administrative path".into())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&dot_git).map_err(storage)?;
                (std::fs::canonicalize(&dot_git).map_err(storage)?, true)
            }
            Err(error) => return Err(storage(error)),
        };
        // Git clean cannot remove this administrative file. Never unlink it:
        // a replacement inode could admit a second writer while this lock lives.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(admin_dir.join("aster.lock"))
            .map_err(storage)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(CoreError::Storage(
                    "notebook checkout already in use".into(),
                ));
            }
            Err(TryLockError::Error(error)) => return Err(storage(error)),
        }
        let source_path = admin_dir.join("aster.source");
        let source_id = match std::fs::read_to_string(&source_path) {
            Ok(value)
                if value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
            {
                value
            }
            Ok(_) => {
                return Err(CoreError::Storage(
                    "invalid notebook source identity".into(),
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let value = aster_core::new_sid()?;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&source_path)
                    .map_err(storage)?;
                file.write_all(value.as_bytes()).map_err(storage)?;
                value
            }
            Err(error) => return Err(storage(error)),
        };
        let store = Self {
            dir,
            branch: branch.into(),
            source_id,
            transaction: Arc::new(Mutex::new(())),
            _checkout_lock: Arc::new(CheckoutLock(lock)),
        };

        if needs_init {
            store.git(&["init", "-q"])?;
        }
        store.validate_checkout_config()?;
        // A cloned team checkout also needs a local service identity because
        // git_in deliberately ignores global Git configuration.
        store.git(&["config", "user.name", "aster"])?;
        store.git(&["config", "user.email", "aster@localhost"])?;

        let reference = format!("refs/heads/{}", store.branch);
        if store
            .git(&["rev-parse", "--verify", "-q", &reference])
            .is_err()
        {
            store.git(&["checkout", "-q", "-b", &store.branch])?;
        } else {
            store.git(&["checkout", "-q", &store.branch])?;
        }
        Ok(store)
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        git_in(&self.dir, args)
    }

    fn path_for(&self, id: &str) -> Result<PathBuf> {
        let safe = !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !safe {
            return Err(CoreError::Invalid(format!("invalid notebook id: {id}")));
        }
        Ok(self.dir.join(format!("{id}.aster")))
    }
}

struct GitAuth<'a> {
    token: &'a str,
    ca_bundle: Option<&'a Path>,
    allowed_host: String,
    allowed_path: String,
}

fn checked_https_remote<'a>(
    remote_url: &str,
    token: &'a str,
    ca_bundle: Option<&'a Path>,
) -> Result<(reqwest::Url, GitAuth<'a>)> {
    let url = reqwest::Url::parse(remote_url)
        .map_err(|_| CoreError::Invalid("invalid notebook HTTPS remote".into()))?;
    let test_loopback =
        ca_bundle.is_some() && matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    let production_github = url.host_str() == Some("github.com") && url.port().is_none();
    let safe_path = url.path_segments().is_some_and(|segments| {
        let parts = segments.collect::<Vec<_>>();
        (test_loopback && parts.len() == 1 || production_github && parts.len() == 2)
            && parts.iter().all(|part| {
                !part.is_empty()
                    && *part != "."
                    && *part != ".."
                    && part.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    })
            })
            && parts.last().is_some_and(|name| name.ends_with(".git"))
    });
    if url.scheme() != "https"
        || !(test_loopback || production_github)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !safe_path
        || token.is_empty()
        || token.len() > 8192
        || token.chars().any(char::is_control)
    {
        return Err(CoreError::Invalid("invalid notebook HTTPS remote".into()));
    }
    let allowed_host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_owned(),
    };
    let auth = GitAuth {
        token,
        ca_bundle,
        allowed_host,
        allowed_path: url.path().trim_start_matches('/').to_owned(),
    };
    Ok((url, auth))
}

fn credential_helper_script() -> &'static str {
    "!f() { test \"$1\" = get || exit; protocol= host= path=; while IFS='=' read -r key value; do test -n \"$key\" || break; case \"$key\" in protocol) protocol=$value ;; host) host=$value ;; path) path=$value ;; esac; done; test \"$protocol\" = https && test \"$host\" = \"$ASTER_GITHUB_GIT_HOST\" && test \"$path\" = \"$ASTER_GITHUB_GIT_PATH\" || exit; printf \"username=x-access-token\\npassword=%s\\n\" \"$ASTER_GITHUB_GIT_TOKEN\"; }; f"
}

fn git_in(dir: &Path, args: &[&str]) -> Result<String> {
    git_in_with_auth(dir, args, None)
}

fn validate_checkout_config_in(dir: &Path) -> Result<()> {
    let config = git_in(
        dir,
        &[
            "config",
            "--local",
            "--no-includes",
            "--name-only",
            "--list",
        ],
    )?;
    for name in config.lines() {
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("url.")
            || lower == "extensions.worktreeconfig"
            || lower.starts_with("include.")
            || lower.starts_with("includeif.")
            || lower.starts_with("credential.")
            || lower.starts_with("http.")
            || lower.starts_with("filter.")
            || lower.starts_with("alias.")
            || lower == "push.gpgsign"
            || lower.starts_with("gpg.")
            || lower == "user.signingkey"
            || matches!(
                lower.as_str(),
                "core.hookspath" | "core.fsmonitor" | "core.sshcommand"
            )
        {
            return Err(CoreError::Storage(
                "unsafe notebook Git configuration".into(),
            ));
        }
    }
    Ok(())
}

fn git_in_with_auth(dir: &Path, args: &[&str], auth: Option<&GitAuth<'_>>) -> Result<String> {
    let mut command = Command::new("git");
    let safe_directory = format!("safe.directory={}", dir.display());
    command.args([
        "-c",
        &safe_directory,
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "push.gpgSign=false",
    ]);
    if let Some(auth) = auth {
        let helper = credential_helper_script();
        command.args([
            "-c",
            "credential.helper=",
            "-c",
            &format!("credential.helper={helper}"),
            "-c",
            "credential.useHttpPath=true",
            "-c",
            "http.followRedirects=false",
            "-c",
            "http.sslVerify=true",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.https.allow=always",
        ]);
        if let Some(ca) = auth.ca_bundle {
            command.args(["-c", &format!("http.sslCAInfo={}", ca.to_string_lossy())]);
        }
        command.env("ASTER_GITHUB_GIT_TOKEN", auth.token);
        command.env("ASTER_GITHUB_GIT_HOST", &auth.allowed_host);
        command.env("ASTER_GITHUB_GIT_PATH", &auth.allowed_path);
    }
    command.args(args).current_dir(dir);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    if auth.is_some() {
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "/bin/false");
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    let output = command.output().map_err(storage)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else if auth.is_some() {
        // Git and credential helpers may echo a remote response. Never relay
        // their stderr while an installation token is in the child process.
        Err(CoreError::Storage(format!(
            "HTTPS Git operation failed ({})",
            output.status
        )))
    } else {
        Err(CoreError::Storage(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn storage(error: std::io::Error) -> CoreError {
    CoreError::Storage(format!("io error: {error}"))
}

fn parse_remote_ref(output: &str, reference: &str) -> Result<Option<String>> {
    let mut lines = output.lines();
    let Some(line) = lines.next() else {
        return Ok(None);
    };
    let mut fields = line.split_whitespace();
    let oid = fields.next().unwrap_or_default();
    if fields.next() != Some(reference) || fields.next().is_some() || lines.next().is_some() {
        return Err(CoreError::Storage("unexpected remote notebook ref".into()));
    }
    if (oid.len() != 40 && oid.len() != 64) || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Storage(
            "invalid remote notebook revision".into(),
        ));
    }
    Ok(Some(oid.to_owned()))
}

fn replace_file(path: &Path, bytes: &[u8]) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_extension(format!(
        "aster.tmp.{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(storage)?;
    let outcome = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .and_then(|_| std::fs::rename(&temporary, path))
        .and_then(|_| File::open(path.parent().unwrap_or_else(|| Path::new(".")))?.sync_all());
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    outcome.map_err(storage)
}

fn validate_workspace_binding(binding: &NotebookWorkspaceBinding) -> Result<()> {
    if binding.team.is_empty()
        || binding.team.len() > 64
        || !binding
            .team
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        || !crate::github_app::safe_branch(&binding.branch)
        || !crate::github_app::safe_branch(&binding.default_branch)
        || (binding.default_commit.len() != 40 && binding.default_commit.len() != 64)
        || !binding
            .default_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || binding.installation_id == 0
        || binding.repository_id == 0
        || binding.target_version == 0
    {
        return Err(CoreError::Invalid(
            "invalid notebook workspace binding".into(),
        ));
    }
    Ok(())
}

fn read_workspace_binding(dir: &Path) -> Result<NotebookWorkspaceBinding> {
    let bytes = std::fs::read(dir.join(".git/aster.workspace.json")).map_err(|_| {
        CoreError::Storage("workspace binding missing; administrator recovery required".into())
    })?;
    let binding: NotebookWorkspaceBinding = serde_json::from_slice(&bytes).map_err(|_| {
        CoreError::Storage("workspace binding invalid; administrator recovery required".into())
    })?;
    validate_workspace_binding(&binding)?;
    Ok(binding)
}

/// Shelling out to `git` and touching the filesystem both block, so every call
/// from async code goes through here instead of stalling a tokio worker.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| CoreError::Storage(format!("blocking task failed: {error}")))?
}

impl GitNotebookStore {
    pub async fn clone_https_no_checkout(
        dir: &Path,
        binding: &NotebookWorkspaceBinding,
        token: &str,
        ca_bundle: Option<&Path>,
    ) -> Result<()> {
        let dir = dir.to_path_buf();
        let binding = binding.clone();
        let token = token.to_owned();
        let ca_bundle = ca_bundle.map(Path::to_path_buf);
        blocking(move || {
            Self::clone_https_no_checkout_sync(&dir, &binding, &token, ca_bundle.as_deref())
        })
        .await
    }

    fn clone_https_no_checkout_sync(
        dir: &Path,
        binding: &NotebookWorkspaceBinding,
        token: &str,
        ca_bundle: Option<&Path>,
    ) -> Result<()> {
        validate_workspace_binding(binding)?;
        let (url, auth) = checked_https_remote(&binding.remote_url, token, ca_bundle)?;
        let parent = dir
            .parent()
            .ok_or_else(|| CoreError::Invalid("invalid notebook checkout path".into()))?;
        std::fs::create_dir_all(parent).map_err(storage)?;
        match std::fs::symlink_metadata(dir) {
            Ok(_) => {
                return Err(CoreError::Conflict(
                    "notebook checkout already exists".into(),
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage(error)),
        }
        // Never expose an unbound partial clone at the service's final path.
        // A crash can leave only this private staging directory, which the
        // binding-required open refuses.
        let staging = parent.join(format!(".aster-clone-{}.tmp", aster_core::new_sid()?));
        std::fs::create_dir(&staging).map_err(storage)?;
        let destination = staging
            .to_str()
            .ok_or_else(|| CoreError::Invalid("invalid notebook checkout path".into()))?;
        let result = (|| {
            git_in_with_auth(
                parent,
                &[
                    "clone",
                    "--no-checkout",
                    "--branch",
                    &binding.default_branch,
                    "--",
                    url.as_str(),
                    destination,
                ],
                Some(&auth),
            )?;
            let metadata = std::fs::symlink_metadata(staging.join(".git")).map_err(storage)?;
            if !metadata.is_dir() {
                return Err(CoreError::Storage("invalid cloned Git directory".into()));
            }
            validate_checkout_config_in(&staging)?;
            git_in(
                &staging,
                &[
                    "cat-file",
                    "-e",
                    &format!("{}^{{commit}}", binding.default_commit),
                ],
            )?;
            git_in(
                &staging,
                &[
                    "merge-base",
                    "--is-ancestor",
                    &binding.default_commit,
                    &format!("refs/remotes/origin/{}", binding.default_branch),
                ],
            )?;
            let bytes = serde_json::to_vec(binding)
                .map_err(|error| CoreError::Storage(error.to_string()))?;
            replace_file(&staging.join(".git/aster.workspace.json"), &bytes)?;
            // Checkout is safe only after the binding is durable. Start the
            // workspace branch at the recorded commit, even if the default
            // branch moved between configuration and this clone.
            git_in(
                &staging,
                &[
                    "checkout",
                    "-q",
                    "-b",
                    &binding.branch,
                    &binding.default_commit,
                ],
            )?;
            std::fs::rename(&staging, dir).map_err(storage)?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(storage)?;
            Ok(())
        })();
        if result.is_err() {
            // Only the nonce-named directory created by this call is removed.
            let _ = std::fs::remove_dir_all(&staging);
        }
        result
    }

    pub async fn sync_https(
        &self,
        remote_url: &str,
        token: &str,
        ca_bundle: Option<&Path>,
    ) -> Result<String> {
        let store = self.clone();
        let remote_url = remote_url.to_owned();
        let token = token.to_owned();
        let ca_bundle = ca_bundle.map(Path::to_path_buf);
        blocking(move || store.sync_https_sync(&remote_url, &token, ca_bundle.as_deref())).await
    }

    fn sync_https_sync(
        &self,
        remote_url: &str,
        token: &str,
        ca_bundle: Option<&Path>,
    ) -> Result<String> {
        let (url, auth) = checked_https_remote(remote_url, token, ca_bundle)?;
        self.sync_with_remote(url.as_str(), Some(&auth), || Ok(()), || Ok(()))
    }

    /// Push this checkout's own branch to a server-selected disposable bare
    /// repository. GitHub credential injection and destination activation are
    /// separate V7 work; callers must never pass a request-selected path.
    pub async fn sync_local_fixture(&self, remote: &Path) -> Result<String> {
        let store = self.clone();
        let remote = remote.to_path_buf();
        blocking(move || store.sync_local_fixture_sync(&remote)).await
    }

    fn sync_local_fixture_sync(&self, remote: &Path) -> Result<String> {
        self.sync_local_fixture_sync_with(remote, || Ok(()))
    }

    fn sync_local_fixture_sync_with(
        &self,
        remote: &Path,
        before_push: impl FnOnce() -> Result<()>,
    ) -> Result<String> {
        self.sync_local_fixture_sync_with_hooks(remote, before_push, || Ok(()))
    }

    fn sync_local_fixture_sync_with_hooks(
        &self,
        remote: &Path,
        before_push: impl FnOnce() -> Result<()>,
        after_push: impl FnOnce() -> Result<()>,
    ) -> Result<String> {
        let destination = remote
            .to_str()
            .ok_or_else(|| CoreError::Invalid("invalid local remote path".into()))?;
        self.sync_with_remote(destination, None, before_push, after_push)
    }

    fn sync_with_remote(
        &self,
        destination: &str,
        auth: Option<&GitAuth<'_>>,
        before_push: impl FnOnce() -> Result<()>,
        after_push: impl FnOnce() -> Result<()>,
    ) -> Result<String> {
        let _transaction = self.transaction.lock().map_err(poisoned)?;
        if auth.is_some() {
            self.validate_checkout_config()?;
        }
        let reference = format!("refs/heads/{}", self.branch);
        let local = self
            .git(&["rev-parse", "--verify", "HEAD"])?
            .trim()
            .to_owned();
        let before = self.remote_ref(destination, &reference, auth)?;
        let lease = format!(
            "--force-with-lease={reference}:{}",
            before.as_deref().unwrap_or("")
        );
        let marker = self.sync_marker_path()?;
        let prior = self.read_sync_marker(&marker)?;
        if let Some(prior) = &prior {
            if prior.destination != destination || prior.reference != reference {
                return Err(CoreError::Conflict(
                    "notebook Sync destination changed".into(),
                ));
            }
            if prior.pending && prior.local != local {
                return Err(CoreError::Conflict(
                    "notebook Sync intent needs recovery".into(),
                ));
            }
        }
        if before.is_none() && prior.is_some() {
            return Err(CoreError::Conflict(
                "remote notebook branch was deleted".into(),
            ));
        }
        if before.as_deref() == Some(local.as_str()) {
            self.write_sync_marker(&marker, destination, &reference, &local, false)?;
            return Ok(local);
        }
        if prior.as_ref().is_some_and(|record| record.pending) {
            return Err(CoreError::Conflict(
                "notebook Sync intent needs recovery".into(),
            ));
        }
        if let Some(remote_oid) = before.as_deref() {
            self.git_remote(&["fetch", "--no-tags", destination, &reference], auth)?;
            let fetched = self
                .git(&["rev-parse", "--verify", "FETCH_HEAD"])?
                .trim()
                .to_owned();
            if fetched != remote_oid
                || self
                    .git(&["merge-base", "--is-ancestor", &fetched, &local])
                    .is_err()
            {
                return Err(CoreError::Conflict(
                    "remote notebook branch diverged or advanced".into(),
                ));
            }
        }
        self.write_sync_marker(&marker, destination, &reference, &local, true)?;
        before_push()?;
        // The lease is an exact ref CAS. The ancestry check above still forbids
        // a non-fast-forward update; a concurrent deletion or creation must
        // never be silently repaired by this push.
        if let Err(error) = self.git_remote(
            &["push", &lease, destination, &format!("HEAD:{reference}")],
            auth,
        ) {
            if self.remote_ref(destination, &reference, auth)? != before {
                return Err(CoreError::Conflict(
                    "remote notebook branch changed during Sync".into(),
                ));
            }
            return Err(error);
        }
        if self.remote_ref(destination, &reference, auth)?.as_deref() != Some(local.as_str()) {
            return Err(CoreError::Conflict(
                "remote notebook revision did not match pushed commit".into(),
            ));
        }
        after_push()?;
        self.write_sync_marker(&marker, destination, &reference, &local, false)?;
        Ok(local)
    }

    fn sync_marker_path(&self) -> Result<PathBuf> {
        let admin = self.git(&["rev-parse", "--absolute-git-dir"])?;
        Ok(PathBuf::from(admin.trim()).join("aster.sync"))
    }

    fn read_sync_marker(&self, marker: &Path) -> Result<Option<SyncMarker>> {
        let content = match std::fs::read_to_string(marker) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(storage(error)),
        };
        if content.len() > 8192 {
            return Err(CoreError::Storage("invalid notebook sync marker".into()));
        }
        let record: SyncMarker = serde_json::from_str(&content)
            .map_err(|_| CoreError::Storage("invalid notebook sync marker".into()))?;
        if (record.local.len() != 40 && record.local.len() != 64)
            || !record.local.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !record.reference.starts_with("refs/heads/")
            || record.destination.is_empty()
        {
            return Err(CoreError::Storage("invalid notebook sync marker".into()));
        }
        Ok(Some(record))
    }

    fn write_sync_marker(
        &self,
        marker: &Path,
        destination: &str,
        reference: &str,
        local: &str,
        pending: bool,
    ) -> Result<()> {
        let record = SyncMarker {
            destination: destination.into(),
            reference: reference.into(),
            local: local.into(),
            pending,
        };
        replace_file(
            marker,
            &serde_json::to_vec(&record).map_err(|error| CoreError::Storage(error.to_string()))?,
        )
    }

    fn remote_ref(
        &self,
        destination: &str,
        reference: &str,
        auth: Option<&GitAuth<'_>>,
    ) -> Result<Option<String>> {
        let output = self.git_remote(&["ls-remote", destination, reference], auth)?;
        parse_remote_ref(&output, reference)
    }

    fn git_remote(&self, args: &[&str], auth: Option<&GitAuth<'_>>) -> Result<String> {
        git_in_with_auth(&self.dir, args, auth)
    }

    fn validate_checkout_config(&self) -> Result<()> {
        validate_checkout_config_in(&self.dir)
    }

    fn get_sync(&self, id: &str) -> Result<Notebook> {
        let _transaction = self.transaction.lock().map_err(poisoned)?;
        if self.blob_sync_locked(id)?.is_none() {
            return Err(CoreError::NotFound(format!("notebook {id}")));
        }
        let path = self.path_for(id)?;
        let name = path.file_name().unwrap().to_string_lossy();
        let text = self.git(&["show", &format!("HEAD:{name}")])?;
        let mut notebook = Notebook::from_text(&text)?;
        notebook.id = id.to_string();
        Ok(notebook)
    }

    fn save_sync(&self, notebook: &Notebook, actor: &str) -> Result<String> {
        let _transaction = self.transaction.lock().map_err(poisoned)?;
        self.save_sync_locked(notebook, actor)
    }

    fn save_sync_locked(&self, notebook: &Notebook, actor: &str) -> Result<String> {
        self.validate_checkout_config()?;
        let path = self.path_for(&notebook.id)?;
        if notebook.title.contains('\r')
            || notebook.title.contains('\n')
            || notebook.cells.iter().any(|cell| {
                cell.id.is_empty()
                    || cell.id.chars().any(char::is_whitespace)
                    || cell.engine.as_ref().is_some_and(|engine| {
                        engine.0.is_empty() || engine.0.chars().any(char::is_whitespace)
                    })
            })
        {
            return Err(CoreError::Invalid("invalid notebook metadata".into()));
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if !self
            .git(&[
                "diff",
                "--cached",
                "--no-ext-diff",
                "--no-textconv",
                "--name-only",
                "--",
                &name,
            ])?
            .trim()
            .is_empty()
        {
            return Err(CoreError::Conflict("notebook has staged changes".into()));
        }
        let text = notebook.to_text();
        if text.len() as u64 > MAX_NOTEBOOK_BYTES {
            return Err(CoreError::Invalid("notebook exceeds size limit".into()));
        }
        let previous = match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(CoreError::Invalid("notebook path is a symlink".into()));
            }
            Ok(_) => Some(std::fs::read(&path).map_err(storage)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(storage(error)),
        };
        let committed = if self.blob_sync_locked(&notebook.id)?.is_some() {
            Some(self.git(&["show", &format!("HEAD:{name}")])?)
        } else {
            None
        };
        if committed.as_deref() == Some(&text) {
            if previous.as_deref() != Some(text.as_bytes()) {
                replace_file(&path, text.as_bytes())?;
            }
            return Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_string());
        }
        replace_file(&path, text.as_bytes())?;
        let message = format!("{actor}: update {}", notebook.id);
        let commit = (|| {
            if committed.is_none() {
                self.git(&["add", "--", &name])?;
            }
            self.git(&["commit", "--only", "-q", "-m", &message, "--", &name])
        })();
        if let Err(error) = commit {
            match previous {
                Some(previous) => replace_file(&path, &previous)?,
                None => std::fs::remove_file(&path).map_err(storage)?,
            }
            if committed.is_none() {
                self.git(&["rm", "--cached", "--ignore-unmatch", "--", &name])?;
            }
            return Err(error);
        }
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_string())
    }

    fn head_exists_locked(&self) -> Result<bool> {
        if self.git(&["rev-parse", "--verify", "HEAD"]).is_err() {
            // An unborn branch has no commit or blob. Require an ordinary
            // symbolic branch and no matching ref; a broken HEAD fails closed.
            let reference = format!("refs/heads/{}", self.branch);
            if self.git(&["symbolic-ref", "HEAD"])?.trim() != reference {
                return Err(CoreError::Storage("notebook branch changed".into()));
            }
            let existing = self.git(&["for-each-ref", "--format=%(objectname)", &reference])?;
            if !existing.trim().is_empty() {
                return Err(CoreError::Storage("invalid notebook HEAD".into()));
            }
            return Ok(false);
        }
        Ok(true)
    }

    fn blob_sync_locked(&self, id: &str) -> Result<Option<String>> {
        let path = self.path_for(id)?;
        let name = path.file_name().unwrap().to_string_lossy();
        if !self.head_exists_locked()? {
            return Ok(None);
        }
        let entry = self.git(&["ls-tree", "HEAD", "--", &name])?;
        if entry.is_empty() {
            return Ok(None);
        }
        let (mode, oid_and_name) = entry
            .trim_end()
            .split_once(' ')
            .ok_or_else(|| CoreError::Storage("invalid notebook tree entry".into()))?;
        let (_, oid_and_name) = oid_and_name
            .split_once(' ')
            .ok_or_else(|| CoreError::Storage("invalid notebook tree entry".into()))?;
        let (oid, found) = oid_and_name
            .split_once('\t')
            .ok_or_else(|| CoreError::Storage("invalid notebook tree entry".into()))?;
        if mode != "100644"
            || found != name
            || !matches!(oid.len(), 40 | 64)
            || !oid.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(CoreError::Storage("invalid notebook tree entry".into()));
        }
        let size = self
            .git(&["cat-file", "-s", oid])?
            .trim()
            .parse::<u64>()
            .map_err(|_| CoreError::Storage("invalid notebook blob size".into()))?;
        if size > MAX_NOTEBOOK_BYTES {
            return Err(CoreError::Invalid("notebook exceeds size limit".into()));
        }
        Ok(Some(oid.to_string()))
    }

    fn snapshot_sync(&self, id: &str) -> Result<NotebookSnapshot> {
        let _transaction = self.transaction.lock().map_err(poisoned)?;
        let content_revision = self
            .blob_sync_locked(id)?
            .ok_or_else(|| CoreError::NotFound(format!("notebook {id}")))?;
        let path = self.path_for(id)?;
        let name = path.file_name().unwrap().to_string_lossy();
        let text = self.git(&["show", &format!("HEAD:{name}")])?;
        let mut notebook = Notebook::from_text(&text)?;
        notebook.id = id.to_string();
        Ok(NotebookSnapshot {
            notebook,
            content_revision,
        })
    }

    fn save_if_sync(
        &self,
        notebook: &Notebook,
        actor: &str,
        expected: NotebookPrecondition,
    ) -> Result<NotebookSave> {
        let _transaction = self.transaction.lock().map_err(poisoned)?;
        let current = self.blob_sync_locked(&notebook.id)?;
        let matches = match expected {
            NotebookPrecondition::Absent => current.is_none(),
            NotebookPrecondition::Blob(oid) => current.as_deref() == Some(oid.as_str()),
        };
        if !matches {
            return Err(CoreError::Conflict("notebook content changed".into()));
        }
        let revision = self.save_sync_locked(notebook, actor)?;
        let content_revision = self
            .blob_sync_locked(&notebook.id)?
            .ok_or_else(|| CoreError::Storage("saved notebook blob missing".into()))?;
        Ok(NotebookSave {
            revision,
            content_revision,
        })
    }

    fn list_sync(&self) -> Result<Vec<String>> {
        let _transaction = self.transaction.lock().map_err(poisoned)?;
        if !self.head_exists_locked()? {
            return Ok(Vec::new());
        }
        let mut ids = self
            .git(&["ls-tree", "--name-only", "HEAD"])?
            .lines()
            .filter_map(|name| name.strip_suffix(".aster"))
            .filter(|id| self.path_for(id).is_ok())
            .map(str::to_string)
            .collect::<Vec<_>>();
        ids.sort();
        Ok(ids)
    }
}

fn poisoned<T>(error: std::sync::PoisonError<T>) -> CoreError {
    CoreError::Storage(format!("notebook transaction lock poisoned: {error}"))
}

#[async_trait]
impl NotebookStore for GitNotebookStore {
    async fn get(&self, id: &str) -> Result<Notebook> {
        let store = self.clone();
        let id = id.to_string();
        blocking(move || {
            let result = store.get_sync(&id);
            drop(store);
            result
        })
        .await
    }

    async fn save(&self, notebook: &Notebook, actor: &str) -> Result<String> {
        let store = self.clone();
        let notebook = notebook.clone();
        let actor = actor.to_string();
        blocking(move || {
            let result = store.save_sync(&notebook, &actor);
            drop(store);
            result
        })
        .await
    }

    async fn list(&self, _actor: &str) -> Result<Vec<String>> {
        let store = self.clone();
        blocking(move || {
            let result = store.list_sync();
            drop(store);
            result
        })
        .await
    }

    async fn snapshot(&self, id: &str) -> Result<NotebookSnapshot> {
        let store = self.clone();
        let id = id.to_string();
        blocking(move || {
            let result = store.snapshot_sync(&id);
            drop(store);
            result
        })
        .await
    }

    async fn save_if(
        &self,
        notebook: &Notebook,
        actor: &str,
        expected: NotebookPrecondition,
    ) -> Result<NotebookSave> {
        let store = self.clone();
        let notebook = notebook.clone();
        let actor = actor.to_string();
        blocking(move || {
            let result = store.save_if_sync(&notebook, &actor, expected);
            drop(store);
            result
        })
        .await
    }

    fn source_key(&self) -> String {
        format!("{}:{}", self.source_id, self.branch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_core::Cell;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aster-gitstore-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn notebook(id: &str) -> Notebook {
        Notebook {
            id: id.into(),
            title: "T".into(),
            cells: vec![Cell {
                id: "c1".into(),
                sql: "SELECT 1".into(),
                engine: None,
                metadata: Default::default(),
            }],
        }
    }

    #[test]
    fn remote_ref_parser_requires_the_exact_single_ref() {
        let oid = "a".repeat(40);
        let wanted = "refs/heads/sessions/alice";
        assert_eq!(
            parse_remote_ref(&format!("{oid}\t{wanted}\n"), wanted).unwrap(),
            Some(oid.clone())
        );
        assert!(parse_remote_ref(&format!("{oid}\trefs/heads/other\n"), wanted).is_err());
        assert!(parse_remote_ref(&format!("{oid}\t{wanted}\n{oid}\t{wanted}\n"), wanted).is_err());
    }

    #[test]
    fn credential_helper_refuses_a_different_origin() {
        use std::io::Write;

        let script = credential_helper_script().strip_prefix('!').unwrap();
        let invocation = format!("{script} get");
        for request in [
            b"protocol=https\nhost=attacker.example\npath=owner/repo.git\n\n".as_slice(),
            b"protocol=https\nhost=github.com\npath=other/repo.git\n\n".as_slice(),
        ] {
            let mut child = Command::new("sh")
                .args(["-c", &invocation])
                .env("ASTER_GITHUB_GIT_HOST", "github.com")
                .env("ASTER_GITHUB_GIT_PATH", "owner/repo.git")
                .env("ASTER_GITHUB_GIT_TOKEN", "fixture-token")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(request).unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(!result.status.success());
            assert!(!String::from_utf8_lossy(&result.stdout).contains("fixture-token"));
        }
    }

    #[tokio::test]
    async fn remote_ref_deleted_during_sync_is_not_recreated() {
        let root = tempdir("sync-delete-race");
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git_in(&remote, &["init", "--bare", "-q"]).unwrap();
        let store = GitNotebookStore::open(root.join("checkout"), "sessions/alice").unwrap();
        store.save(&notebook("nb1"), "alice").await.unwrap();
        let reference = "refs/heads/sessions/alice";
        store.sync_local_fixture(&remote).await.unwrap();
        let mut changed = notebook("nb1");
        changed.cells[0].sql = "SELECT 2".into();
        store.save(&changed, "alice").await.unwrap();

        assert!(matches!(
            store.sync_local_fixture_sync_with(&remote, || {
                git_in(&remote, &["update-ref", "-d", reference])?;
                Ok(())
            }),
            Err(CoreError::Conflict(_))
        ));
        assert!(git_in(&remote, &["rev-parse", "--verify", reference]).is_err());
    }

    #[tokio::test]
    async fn pending_push_survives_lost_ack_and_ref_deletion() {
        let root = tempdir("sync-lost-ack-delete");
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git_in(&remote, &["init", "--bare", "-q"]).unwrap();
        let store = GitNotebookStore::open(root.join("checkout"), "sessions/alice").unwrap();
        store.save(&notebook("nb1"), "alice").await.unwrap();
        let local = store.git(&["rev-parse", "HEAD"]).unwrap().trim().to_owned();
        let reference = "refs/heads/sessions/alice";

        assert!(store
            .sync_local_fixture_sync_with_hooks(
                &remote,
                || Ok(()),
                || Err(CoreError::Storage("simulated lost acknowledgement".into())),
            )
            .is_err());
        assert_eq!(
            git_in(&remote, &["rev-parse", "--verify", reference])
                .unwrap()
                .trim(),
            local
        );
        git_in(&remote, &["update-ref", "-d", reference]).unwrap();
        assert!(matches!(
            store.sync_local_fixture(&remote).await,
            Err(CoreError::Conflict(_))
        ));
        assert_eq!(store.git(&["rev-parse", "HEAD"]).unwrap().trim(), local);
        assert!(git_in(&remote, &["rev-parse", "--verify", reference]).is_err());
    }

    #[tokio::test]
    async fn pending_push_reconciles_only_the_exact_bound_remote() {
        let root = tempdir("sync-pending-bound");
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("remote.git");
        let other = root.join("other.git");
        for path in [&remote, &other] {
            std::fs::create_dir_all(path).unwrap();
            git_in(path, &["init", "--bare", "-q"]).unwrap();
        }
        let store = GitNotebookStore::open(root.join("checkout"), "sessions/alice").unwrap();
        store.save(&notebook("nb1"), "alice").await.unwrap();
        let local = store.git(&["rev-parse", "HEAD"]).unwrap().trim().to_owned();
        let reference = "refs/heads/sessions/alice";

        assert!(store
            .sync_local_fixture_sync_with_hooks(
                &remote,
                || Ok(()),
                || Err(CoreError::Storage("simulated lost acknowledgement".into())),
            )
            .is_err());
        assert!(matches!(
            store.sync_local_fixture(&other).await,
            Err(CoreError::Conflict(_))
        ));
        assert!(git_in(&other, &["rev-parse", "--verify", reference]).is_err());
        assert_eq!(store.sync_local_fixture(&remote).await.unwrap(), local);
        let marker = store
            .read_sync_marker(&store.sync_marker_path().unwrap())
            .unwrap()
            .unwrap();
        assert!(!marker.pending);
        assert_eq!(marker.destination, remote.to_str().unwrap());
        assert_eq!(marker.reference, reference);
        assert_eq!(marker.local, local);
    }

    #[tokio::test]
    async fn remote_ref_created_during_sync_requires_a_new_observation() {
        let root = tempdir("sync-create-race");
        std::fs::create_dir_all(&root).unwrap();
        let remote = root.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git_in(&remote, &["init", "--bare", "-q"]).unwrap();
        let store = GitNotebookStore::open(root.join("checkout"), "sessions/alice").unwrap();
        store.save(&notebook("nb1"), "alice").await.unwrap();
        let mut changed = notebook("nb1");
        changed.cells[0].sql = "SELECT 2".into();
        store.save(&changed, "alice").await.unwrap();
        let prior = store.git(&["rev-parse", "HEAD^"]).unwrap();
        let reference = "refs/heads/sessions/alice";

        assert!(matches!(
            store.sync_local_fixture_sync_with(&remote, || {
                store.git(&[
                    "push",
                    remote.to_str().unwrap(),
                    &format!("HEAD^:{reference}"),
                ])?;
                Ok(())
            }),
            Err(CoreError::Conflict(_))
        ));
        assert_eq!(
            git_in(&remote, &["rev-parse", "--verify", reference])
                .unwrap()
                .trim(),
            prior.trim()
        );
    }

    #[tokio::test]
    async fn saves_and_reads_back_on_session_branch() {
        let store = GitNotebookStore::open(tempdir("save"), "session/alice").unwrap();
        let revision = store.save(&notebook("nb1"), "alice").await.unwrap();
        assert!(!revision.is_empty());

        let loaded = store.get("nb1").await.unwrap();
        assert_eq!(loaded.cells[0].sql, "SELECT 1");
        assert_eq!(store.list("alice").await.unwrap(), vec!["nb1".to_string()]);

        let branch = store.git(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(branch.trim(), "session/alice");
    }

    #[tokio::test]
    async fn rejects_path_traversal_ids() {
        let store = GitNotebookStore::open(tempdir("traversal"), "session").unwrap();
        assert!(store.save(&notebook("../evil"), "alice").await.is_err());
        assert!(store.get("../evil").await.is_err());
    }

    #[tokio::test]
    async fn identical_save_returns_the_existing_commit() {
        let store = GitNotebookStore::open(tempdir("noop"), "session/alice").unwrap();
        let first = store.save(&notebook("nb1"), "alice").await.unwrap();
        let second = store.save(&notebook("nb1"), "alice").await.unwrap();
        assert_eq!(second, first);
        assert_eq!(
            store.git(&["rev-list", "--count", "HEAD"]).unwrap().trim(),
            "1"
        );
    }

    #[tokio::test]
    async fn identical_save_repairs_a_dirty_working_file_without_a_commit() {
        let store = GitNotebookStore::open(tempdir("dirty-noop"), "session/alice").unwrap();
        let original = notebook("nb1");
        let first = store.save(&original, "alice").await.unwrap();
        std::fs::write(store.dir.join("nb1.aster"), "unsaved drift").unwrap();
        assert_eq!(store.save(&original, "alice").await.unwrap(), first);
        assert_eq!(
            std::fs::read_to_string(store.dir.join("nb1.aster")).unwrap(),
            original.to_text()
        );
        assert!(store.git(&["status", "--porcelain"]).unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_commit_restores_the_working_notebook() {
        let store = GitNotebookStore::open(tempdir("failed"), "session/alice").unwrap();
        let first = store.save(&notebook("nb1"), "alice").await.unwrap();
        let ref_lock = store.dir.join(".git/refs/heads/session/alice.lock");
        std::fs::write(&ref_lock, "locked").unwrap();
        let mut changed = notebook("nb1");
        changed.cells[0].sql = "SELECT 2".into();

        assert!(store.save(&changed, "alice").await.is_err());
        assert_eq!(store.git(&["rev-parse", "HEAD"]).unwrap().trim(), first);
        assert_eq!(store.get("nb1").await.unwrap().cells[0].sql, "SELECT 1");
        assert!(store.git(&["status", "--porcelain"]).unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_save_preserves_another_staged_version_of_the_same_notebook() {
        let store = GitNotebookStore::open(tempdir("staged-target"), "session/alice").unwrap();
        let path = store.dir.join("nb1.aster");
        std::fs::write(&path, "another tool's staged draft").unwrap();
        store.git(&["add", "--", "nb1.aster"]).unwrap();
        let staged = store.git(&["show", ":nb1.aster"]).unwrap();
        let ref_lock = store.dir.join(".git/refs/heads/session/alice.lock");
        std::fs::create_dir_all(ref_lock.parent().unwrap()).unwrap();
        std::fs::write(&ref_lock, "locked").unwrap();

        assert!(store.save(&notebook("nb1"), "alice").await.is_err());
        assert_eq!(store.git(&["show", ":nb1.aster"]).unwrap(), staged);
        assert_eq!(std::fs::read_to_string(path).unwrap(), staged);
        assert!(store.git(&["rev-parse", "HEAD"]).is_err());
    }

    #[tokio::test]
    async fn save_does_not_run_checkout_hooks() {
        let store = GitNotebookStore::open(tempdir("hooks"), "session/alice").unwrap();
        let marker = store.dir.join("hook-ran");
        let hook = store.dir.join(".git/hooks/pre-commit");
        std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
        store.save(&notebook("nb1"), "alice").await.unwrap();
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn opening_an_existing_checkout_does_not_run_hooks() {
        let dir = tempdir("open-hooks");
        let store = GitNotebookStore::open(&dir, "session/alice").unwrap();
        store.save(&notebook("nb1"), "alice").await.unwrap();
        let marker = dir.join("hook-ran");
        let hook = dir.join(".git/hooks/post-checkout");
        std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
        drop(store);
        GitNotebookStore::open(&dir, "session/alice").unwrap();
        assert!(!marker.exists());
    }

    #[test]
    fn checkout_lock_probe() {
        let Ok(dir) = std::env::var("ASTER_GIT_CHECKOUT_PROBE_DIR") else {
            return;
        };
        let branch = std::env::var("ASTER_GIT_CHECKOUT_PROBE_BRANCH").unwrap();
        let blocked = std::env::var("ASTER_GIT_CHECKOUT_PROBE_BLOCKED").unwrap() == "1";
        let result = GitNotebookStore::open(dir, branch);
        if blocked {
            assert!(result.is_err(), "second process opened a held checkout");
        } else {
            assert!(
                result.is_ok(),
                "checkout stayed locked after release: {:?}",
                result.err()
            );
        }
    }

    fn probe_checkout(dir: &Path, branch: &str, blocked: bool) {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "gitstore::tests::checkout_lock_probe",
                "--nocapture",
            ])
            .env("ASTER_GIT_CHECKOUT_PROBE_DIR", dir)
            .env("ASTER_GIT_CHECKOUT_PROBE_BRANCH", branch)
            .env(
                "ASTER_GIT_CHECKOUT_PROBE_BLOCKED",
                if blocked { "1" } else { "0" },
            )
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child probe failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn checkout_lock_refuses_before_init_and_releases_after_owner_exit() {
        let dir = tempdir("lock-init");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(".git/aster.lock"))
            .unwrap();
        lock.try_lock().unwrap();

        probe_checkout(&dir, "session/alice", true);
        assert!(
            !dir.join(".git/HEAD").exists(),
            "blocked open initialized Git"
        );

        drop(lock);
        probe_checkout(&dir, "session/alice", false);
        assert!(dir.join(".git/HEAD").exists());
        assert!(
            dir.join(".git/aster.lock").exists(),
            "release deleted the lock inode"
        );
    }

    #[test]
    fn checkout_lock_is_shared_by_clones_and_prevents_branch_switch() {
        let dir = tempdir("lock-branch");
        let store = GitNotebookStore::open(&dir, "session/alice").unwrap();
        let clone = store.clone();
        drop(store);

        probe_checkout(&dir, "session/bob", true);
        assert_eq!(
            clone
                .git(&["symbolic-ref", "--short", "HEAD"])
                .unwrap()
                .trim(),
            "session/alice",
            "blocked open switched the branch"
        );
        assert!(clone.git(&["status", "--porcelain"]).unwrap().is_empty());
        assert!(!dir.join(".aster.lock").exists());
        assert!(dir.join(".git/aster.lock").exists());

        drop(clone);
        probe_checkout(&dir, "session/bob", false);
        let reopened = GitNotebookStore::open(&dir, "session/bob").unwrap();
        assert_eq!(
            reopened
                .git(&["symbolic-ref", "--short", "HEAD"])
                .unwrap()
                .trim(),
            "session/bob"
        );
    }

    #[test]
    fn checkout_lock_survives_git_clean() {
        let dir = tempdir("lock-clean");
        let store = GitNotebookStore::open(&dir, "session/alice").unwrap();
        store.save_sync(&notebook("nb1"), "alice").unwrap();
        store.git(&["clean", "-fdx"]).unwrap();

        probe_checkout(&dir, "session/bob", true);
        assert_eq!(
            store
                .git(&["symbolic-ref", "--short", "HEAD"])
                .unwrap()
                .trim(),
            "session/alice"
        );
        assert!(dir.join(".git/aster.lock").exists());
    }

    #[test]
    fn checkout_lock_resolves_linked_worktree_admin_dir() {
        let dir = tempdir("linked-primary");
        let linked = tempdir("linked-secondary");
        let store = GitNotebookStore::open(&dir, "session/alice").unwrap();
        store.save_sync(&notebook("nb1"), "alice").unwrap();
        drop(store);
        let add = Command::new("git")
            .args([
                "-C",
                dir.to_str().unwrap(),
                "worktree",
                "add",
                "-q",
                "-b",
                "session/bob",
                linked.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            add.status.success(),
            "{}",
            String::from_utf8_lossy(&add.stderr)
        );

        let linked_store = GitNotebookStore::open(&linked, "session/bob").unwrap();
        let admin = linked_store
            .git(&["rev-parse", "--absolute-git-dir"])
            .unwrap();
        assert!(PathBuf::from(admin.trim()).join("aster.lock").exists());
        assert!(!linked.join(".aster.lock").exists());
    }

    #[tokio::test]
    async fn save_rejects_a_notebook_symlink() {
        let store = GitNotebookStore::open(tempdir("symlink"), "session/alice").unwrap();
        let outside = tempdir("outside").with_extension("txt");
        std::fs::write(&outside, "keep me").unwrap();
        std::os::unix::fs::symlink(&outside, store.dir.join("nb1.aster")).unwrap();
        assert!(store.save(&notebook("nb1"), "alice").await.is_err());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep me");
    }

    #[tokio::test]
    async fn save_does_not_modify_a_hard_link_target() {
        let store = GitNotebookStore::open(tempdir("hardlink"), "session/alice").unwrap();
        let outside = tempdir("hardlink-outside").with_extension("txt");
        std::fs::write(&outside, "keep me").unwrap();
        std::fs::hard_link(&outside, store.dir.join("nb1.aster")).unwrap();
        store.save(&notebook("nb1"), "alice").await.unwrap();
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep me");
    }

    #[tokio::test]
    async fn save_rejects_metadata_that_changes_the_notebook_structure() {
        let store = GitNotebookStore::open(tempdir("metadata"), "session/alice").unwrap();
        let mut bad_title = notebook("nb1");
        bad_title.title = "Sales\n-- cell injected".into();
        assert!(store.save(&bad_title, "alice").await.is_err());
        let mut bad_cell = notebook("nb1");
        bad_cell.cells[0].id = "c1 injected".into();
        assert!(store.save(&bad_cell, "alice").await.is_err());
        assert!(store.list("alice").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn committed_symlink_is_not_read_as_a_notebook() {
        let store = GitNotebookStore::open(tempdir("committed-symlink"), "session/alice").unwrap();
        let target = notebook("nb1").to_text();
        std::os::unix::fs::symlink(target, store.dir.join("nb1.aster")).unwrap();
        store.git(&["add", "--", "nb1.aster"]).unwrap();
        store.git(&["commit", "-m", "fixture symlink"]).unwrap();
        assert!(
            store.get("nb1").await.is_err(),
            "a committed symlink target must not parse as notebook content"
        );
    }

    #[tokio::test]
    async fn oversized_committed_notebook_is_refused_before_read() {
        let store = GitNotebookStore::open(tempdir("oversized-blob"), "session/alice").unwrap();
        let mut large = notebook("nb1");
        large.cells[0].sql = "x".repeat(2_100_000);
        std::fs::write(store.dir.join("nb1.aster"), large.to_text()).unwrap();
        store.git(&["add", "--", "nb1.aster"]).unwrap();
        store
            .git(&["commit", "-m", "fixture oversized blob"])
            .unwrap();
        assert!(
            store.get("nb1").await.is_err(),
            "a committed notebook blob must have a read limit"
        );
    }

    #[tokio::test]
    async fn save_refuses_a_local_clean_filter_before_it_executes() {
        let store = GitNotebookStore::open(tempdir("clean-filter"), "session/alice").unwrap();
        std::fs::write(store.dir.join(".gitattributes"), "*.aster filter=evil\n").unwrap();
        store.git(&["add", "--", ".gitattributes"]).unwrap();
        store.git(&["commit", "-m", "fixture attributes"]).unwrap();
        let marker = store.dir.join(".git/filter-invoked");
        let script = store.dir.join(".git/filter.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch {}\ncat\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        store
            .git(&[
                "config",
                "--local",
                "filter.evil.clean",
                script.to_str().unwrap(),
            ])
            .unwrap();
        let error = store.save(&notebook("nb1"), "alice").await.unwrap_err();
        assert!(error
            .to_string()
            .contains("unsafe notebook Git configuration"));
        assert!(
            !marker.exists(),
            "Git executed a checkout-local clean filter"
        );
    }

    #[test]
    fn reopen_refuses_local_filter_configuration_before_checkout() {
        let dir = tempdir("reopen-filter");
        let store = GitNotebookStore::open(&dir, "session/alice").unwrap();
        store
            .git(&["config", "--local", "filter.evil.smudge", "/bin/false"])
            .unwrap();
        drop(store);
        let error = GitNotebookStore::open(&dir, "session/alice")
            .err()
            .expect("unsafe checkout configuration must be refused");
        assert!(error
            .to_string()
            .contains("unsafe notebook Git configuration"));
    }

    #[tokio::test]
    async fn concurrent_saves_commit_only_their_own_notebook() {
        let store = GitNotebookStore::open(tempdir("concurrent"), "session/alice").unwrap();
        std::fs::write(store.dir.join("unrelated.txt"), "staged by another tool\n").unwrap();
        store.git(&["add", "--", "unrelated.txt"]).unwrap();
        let left = store.clone();
        let right = store.clone();
        let (a, b) = tokio::join!(
            async move { left.save(&notebook("a"), "alice").await.unwrap() },
            async move { right.save(&notebook("b"), "alice").await.unwrap() }
        );
        assert_ne!(a, b);
        for (revision, id) in [(a, "a"), (b, "b")] {
            let files = store
                .git(&[
                    "diff-tree",
                    "--root",
                    "--no-commit-id",
                    "--name-only",
                    "-r",
                    &revision,
                ])
                .unwrap();
            assert_eq!(files.trim(), format!("{id}.aster"));
            assert_eq!(
                store
                    .git(&["show", &format!("{revision}:{id}.aster")])
                    .unwrap(),
                notebook(id).to_text()
            );
        }
        assert_eq!(
            store
                .git(&["diff", "--cached", "--name-only"])
                .unwrap()
                .trim(),
            "unrelated.txt"
        );
    }
}
