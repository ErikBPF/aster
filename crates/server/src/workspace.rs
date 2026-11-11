//! Durable workspace identity is independent of browser authentication sessions.

use aster_core::{CoreError, Principal, Result};
use sha2::{Digest, Sha256};

use crate::store::PgStore;

pub const MAX_WORKSPACES: usize = 10;
pub const MAX_CHECKOUTS: usize = 8;

pub struct Workspaces {
    store: PgStore,
    checkouts: std::sync::Arc<dyn aster_core::workspace::WorkspaceCheckouts>,
    recovery: Option<std::sync::Arc<dyn aster_core::WorkspaceRecovery>>,
    targets: Option<std::sync::Arc<crate::team_git::TeamGitTargets>>,
    hosting: Option<std::sync::Arc<dyn aster_core::workspace::WorkspaceHosting>>,
}

#[derive(Clone)]
pub struct WriterContext {
    pub workspace: String,
    pub token: String,
    pub epoch: u64,
}

#[derive(serde::Serialize)]
pub struct CheckoutView {
    pub id: String,
    pub workspace: String,
    pub team: String,
    pub repository: String,
    pub branch: String,
    pub version: u64,
    pub head: String,
    pub dirty: bool,
    pub notebooks: Vec<String>,
    pub draft_revision: Option<String>,
}

#[derive(Clone)]
#[non_exhaustive]
pub enum CheckoutEdit {
    SaveDraft {
        notebook: aster_core::Notebook,
        expected: Option<String>,
    },
    Commit {
        notebooks: Vec<String>,
        message: String,
    },
    SwitchBranch {
        branch: String,
        create: bool,
    },
    Pull,
}

pub struct WorkspaceRemoteOutcome {
    pub checkout: Option<CheckoutView>,
    pub pull_request: Option<String>,
}

pub struct DraftView {
    pub notebook: aster_core::Notebook,
    pub revision: String,
    pub checkout_version: u64,
    pub branch: String,
    pub head: String,
}

#[derive(Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceLayout {
    #[serde(default)]
    pub tabs: Vec<WorkspaceTab>,
    pub active: Option<usize>,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceTab {
    pub checkout: String,
    pub notebook: String,
}

pub struct WorkspaceState {
    pub checkouts: Vec<String>,
    pub layout: WorkspaceLayout,
    pub layout_version: u64,
    pub recovery_bytes: u64,
    pub total_checkouts: u64,
}

pub struct WorkspaceView {
    pub id: String,
    pub name: String,
    pub epoch: u64,
    pub read_only: bool,
    pub writer_token: Option<String>,
    pub deleting: bool,
}

#[derive(sqlx::FromRow)]
struct WorkspaceRecord {
    id: String,
    subject: String,
    name: String,
    writer_hash: String,
    epoch: i64,
    deleted: bool,
}

#[derive(sqlx::FromRow)]
struct CheckoutRecord {
    id: String,
    workspace: String,
    team: String,
    repository_id: i64,
    repository: String,
    binding: serde_json::Value,
    branch: String,
    version: i64,
}

fn storage(error: sqlx::Error) -> CoreError {
    CoreError::Storage(error.to_string())
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn view(row: WorkspaceRecord, token: Option<&str>) -> Result<WorkspaceView> {
    Ok(WorkspaceView {
        id: row.id,
        name: row.name,
        epoch: u64::try_from(row.epoch)
            .map_err(|_| CoreError::Storage("invalid workspace epoch".into()))?,
        deleting: row.deleted,
        read_only: row.deleted || token.is_none_or(|token| token_hash(token) != row.writer_hash),
        writer_token: None,
    })
}

async fn close_directory(temporary: tempfile::TempDir) {
    if !matches!(
        tokio::task::spawn_blocking(move || temporary.close()).await,
        Ok(Ok(()))
    ) {
        tracing::warn!("workspace temporary-directory cleanup failed");
    }
}

impl Workspaces {
    pub async fn connect(database_url: &str) -> Result<Self> {
        Ok(Self {
            store: PgStore::connect(database_url).await?,
            checkouts: crate::providers::workspace_checkouts("git")?,
            recovery: None,
            targets: None,
            hosting: None,
        })
    }

    pub fn with_checkout_provider(
        mut self,
        checkouts: std::sync::Arc<dyn aster_core::workspace::WorkspaceCheckouts>,
    ) -> Self {
        self.checkouts = checkouts;
        self
    }

    pub fn with_recovery(
        mut self,
        recovery: std::sync::Arc<dyn aster_core::WorkspaceRecovery>,
    ) -> Self {
        self.recovery = Some(recovery);
        self
    }

    async fn lock_writer<'a>(
        &'a self,
        principal: &Principal,
        writer: &WriterContext,
    ) -> Result<sqlx::Transaction<'a, sqlx::Postgres>> {
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        let (hash, epoch, pending): (String, i64, Option<serde_json::Value>) = sqlx::query_as(
            "SELECT writer_hash, epoch, pending_operation FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR UPDATE"
        ).bind(&writer.workspace).bind(&principal.subject).fetch_optional(&mut *tx).await.map_err(storage)?
            .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
        if hash != token_hash(&writer.token)
            || u64::try_from(epoch).ok() != Some(writer.epoch)
            || pending.is_some()
        {
            return Err(CoreError::Conflict(
                "workspace is read-only or awaiting remote reconciliation".into(),
            ));
        }
        Ok(tx)
    }

    #[cfg(test)]
    pub(crate) async fn checkpoint(
        &self,
        principal: &Principal,
        writer: &WriterContext,
        checkout: &str,
        root: std::path::PathBuf,
        expected: u64,
    ) -> Result<u64> {
        let mut tx = self.lock_writer(principal, writer).await?;
        let revision = self
            .record_checkpoint(&mut tx, &writer.workspace, checkout, root, expected)
            .await?;
        tx.commit().await.map_err(storage)?;
        Ok(revision)
    }

    async fn record_checkpoint(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        workspace: &str,
        checkout: &str,
        root: std::path::PathBuf,
        expected: u64,
    ) -> Result<u64> {
        let recovery = self
            .recovery
            .as_ref()
            .ok_or_else(|| CoreError::Storage("workspace recovery disabled".into()))?;
        let expected = i64::try_from(expected)
            .map_err(|_| CoreError::Invalid("invalid checkout revision".into()))?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT version FROM durable_workspace_checkouts WHERE id=$1 AND workspace=$2",
        )
        .bind(checkout)
        .bind(workspace)
        .fetch_optional(&mut **tx)
        .await
        .map_err(storage)?
        .ok_or_else(|| CoreError::Unauthorized("checkout unavailable".into()))?;
        if revision != expected {
            return Err(CoreError::Conflict(
                "checkout changed; reopen before saving".into(),
            ));
        }
        let current: Vec<String> = sqlx::query_scalar("SELECT snapshot_key FROM durable_workspace_checkouts WHERE workspace=$1 AND snapshot_key IS NOT NULL")
            .bind(workspace).fetch_all(&mut **tx).await.map_err(storage)?;
        for key in recovery.list(&format!("workspaces/{workspace}")).await? {
            if !current.contains(&key) {
                recovery.delete(&key).await?;
            }
        }
        let scope = format!("{}/{}", workspace, checkout);
        let bytes =
            tokio::task::spawn_blocking(move || super::workspace_snapshot::capture(&root, &scope))
                .await
                .map_err(|_| CoreError::Storage("workspace checkpoint task failed".into()))??;
        let key = format!(
            "workspaces/{}/{}/{}",
            workspace,
            checkout,
            aster_core::new_sid()?
        );
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let length = bytes.len() as i64;
        recovery.put(&key, bytes).await?;
        let revision: i64 = sqlx::query_scalar(
            "UPDATE durable_workspace_checkouts SET version=version+1, snapshot_key=$3, snapshot_hash=$4, snapshot_bytes=$5 WHERE id=$1 AND workspace=$2 RETURNING version"
        ).bind(checkout).bind(workspace).bind(&key).bind(&hash).bind(length)
            .fetch_one(&mut **tx).await.map_err(storage)?;
        u64::try_from(revision).map_err(|_| CoreError::Storage("invalid checkout revision".into()))
    }

    #[cfg(test)]
    pub(crate) async fn restore_checkout(
        &self,
        principal: &Principal,
        workspace: &str,
        checkout: &str,
        destination: std::path::PathBuf,
    ) -> Result<()> {
        let mut connection = self.store.pool.acquire().await.map_err(storage)?;
        if self.targets.is_some() {
            self.checked_checkout(&mut connection, principal, workspace, checkout)
                .await?;
        }
        self.restore_checkout_data(&mut connection, principal, workspace, checkout, destination)
            .await
    }

    async fn restore_checkout_data(
        &self,
        connection: &mut sqlx::PgConnection,
        principal: &Principal,
        workspace: &str,
        checkout: &str,
        destination: std::path::PathBuf,
    ) -> Result<()> {
        let recovery = self
            .recovery
            .as_ref()
            .ok_or_else(|| CoreError::Storage("workspace recovery disabled".into()))?;
        let (key, expected_hash): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT c.snapshot_key, c.snapshot_hash FROM durable_workspace_checkouts c JOIN durable_workspaces w ON c.workspace=w.id WHERE w.id=$1 AND w.subject=$2 AND NOT w.deleted AND c.id=$3"
        ).bind(workspace).bind(&principal.subject).bind(checkout).fetch_optional(&mut *connection).await.map_err(storage)?
            .ok_or_else(|| CoreError::Unauthorized("checkout unavailable".into()))?;
        let key = key
            .ok_or_else(|| CoreError::Conflict("checkout has no recoverable checkpoint".into()))?;
        let bytes = recovery
            .get(&key, super::workspace_snapshot::MAX_ARCHIVE_BYTES)
            .await?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if Some(hash) != expected_hash {
            return Err(CoreError::Storage(
                "workspace checkpoint integrity failure".into(),
            ));
        }
        let scope = format!("{workspace}/{checkout}");
        tokio::task::spawn_blocking(move || {
            super::workspace_snapshot::restore(&destination, &scope, &bytes)
        })
        .await
        .map_err(|_| CoreError::Storage("workspace restore task failed".into()))?
    }

    pub fn with_git(
        mut self,
        targets: std::sync::Arc<crate::team_git::TeamGitTargets>,
        hosting: std::sync::Arc<dyn aster_core::workspace::WorkspaceHosting>,
    ) -> Self {
        self.targets = Some(targets);
        self.hosting = Some(hosting);
        self
    }

    pub async fn clone_repository(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        writer: &WriterContext,
        team: &str,
        repository_id: u64,
        branch: &str,
    ) -> Result<CheckoutView> {
        let service = self.clone();
        let principal = principal.clone();
        let writer = writer.clone();
        let team = team.to_owned();
        let branch = branch.to_owned();
        // The operation owns its transaction and directory even if the HTTP request is cancelled.
        tokio::spawn(async move {
            service
                .clone_repository_inner(&principal, &writer, &team, repository_id, &branch)
                .await
        })
        .await
        .map_err(|_| CoreError::Storage("workspace clone task failed".into()))?
    }

    async fn clone_repository_inner(
        &self,
        principal: &Principal,
        writer: &WriterContext,
        team: &str,
        repository_id: u64,
        branch: &str,
    ) -> Result<CheckoutView> {
        use aster_core::workspace::NotebookWorkspaceBinding;
        aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
        if branch.len() > 32 || !crate::github_app::safe_branch(branch) {
            return Err(CoreError::Invalid(
                "branch name must be a valid name of at most 32 bytes".into(),
            ));
        }
        let targets = self
            .targets
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("workspace Git disabled".into()))?;
        let hosting = self
            .hosting
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("workspace hosting disabled".into()))?;
        let target = targets
            .repositories(team, principal)
            .await?
            .into_iter()
            .find(|target| u64::try_from(target.repository_id).ok() == Some(repository_id))
            .ok_or_else(|| CoreError::Unauthorized("repository unavailable".into()))?;
        let mut tx = self.lock_writer(principal, writer).await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM durable_workspace_checkouts WHERE workspace=$1",
        )
        .bind(&writer.workspace)
        .fetch_one(&mut *tx)
        .await
        .map_err(storage)?;
        if count >= MAX_CHECKOUTS as i64 {
            return Err(CoreError::Conflict(
                "workspace repository quota exceeded".into(),
            ));
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM durable_workspace_checkouts WHERE workspace=$1 AND repository_id=$2)")
            .bind(&writer.workspace).bind(target.repository_id).fetch_one(&mut *tx).await.map_err(storage)?;
        if exists {
            return Err(CoreError::Conflict(
                "repository already has a checkout in this workspace".into(),
            ));
        }
        let credential = hosting
            .credential(
                &target.repository,
                target.installation_id as u64,
                repository_id,
            )
            .await?;
        let id = aster_core::new_sid()?;
        let subject_hash = format!("{:x}", Sha256::digest(principal.subject.as_bytes()));
        let branch = format!("aster/{}/{id}/{branch}", &subject_hash[..16]);
        let binding = NotebookWorkspaceBinding {
            team: team.into(),
            branch: branch.clone(),
            installation_id: target.installation_id as u64,
            repository_id,
            default_branch: target.default_branch.clone(),
            default_commit: target.default_commit.clone(),
            target_version: target.version as u64,
            remote_url: credential.remote_url.clone(),
        };
        let temporary = tokio::task::spawn_blocking(tempfile::tempdir)
            .await
            .map_err(|_| CoreError::Storage("workspace directory task failed".into()))?
            .map_err(|_| CoreError::Storage("workspace directory unavailable".into()))?;
        let root = temporary.path().join("checkout");
        let result = async {
            let store = self.checkouts.clone_repository(root.clone(), binding.clone(), credential).await?;
            sqlx::query("INSERT INTO durable_workspace_checkouts(id,workspace,team,repository_id,binding,branch) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(&id).bind(&writer.workspace).bind(team).bind(target.repository_id)
                .bind(serde_json::to_value(&binding).map_err(|_| CoreError::Storage("invalid checkout binding".into()))?).bind(&branch)
                .execute(&mut *tx).await.map_err(storage)?;
            let status = store.git_status().await?;
            let notebooks = store.draft_ids().await?;
            drop(store);
            let version = self.record_checkpoint(&mut tx, &writer.workspace, &id, root, 0).await?;
            tx.commit().await.map_err(storage)?;
            Ok(CheckoutView { id, workspace:writer.workspace.clone(), team:team.into(), repository:target.repository,
                branch, version, head:status.head, dirty:status.dirty, notebooks, draft_revision:None })
        }.await;
        close_directory(temporary).await;
        result
    }

    async fn checked_checkout(
        &self,
        connection: &mut sqlx::PgConnection,
        principal: &Principal,
        workspace: &str,
        checkout: &str,
    ) -> Result<CheckoutRecord> {
        aster_core::authorize(principal, aster_core::Action::ReadNotebook)?;
        let record: CheckoutRecord = sqlx::query_as(
            "SELECT c.id,c.workspace,c.team,c.repository_id,r.repository,c.binding,c.branch,c.version FROM durable_workspace_checkouts c JOIN durable_workspaces w ON w.id=c.workspace JOIN workspace_repositories r ON r.team=c.team AND r.repository_id=c.repository_id WHERE c.id=$1 AND w.id=$2 AND w.subject=$3 AND NOT w.deleted"
        ).bind(checkout).bind(workspace).bind(&principal.subject).fetch_optional(&mut *connection).await.map_err(storage)?
            .ok_or_else(|| CoreError::Unauthorized("checkout unavailable".into()))?;
        let targets = self
            .targets
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("workspace Git disabled".into()))?;
        let target = targets
            .repositories(&record.team, principal)
            .await?
            .into_iter()
            .find(|target| target.repository_id == record.repository_id)
            .ok_or_else(|| CoreError::Unauthorized("repository unavailable".into()))?;
        let binding: crate::gitstore::NotebookWorkspaceBinding =
            serde_json::from_value(record.binding.clone())
                .map_err(|_| CoreError::Storage("invalid checkout binding".into()))?;
        if binding.team != record.team
            || binding.repository_id != record.repository_id as u64
            || binding.installation_id != target.installation_id as u64
        {
            return Err(CoreError::Unauthorized(
                "repository binding no longer authorized".into(),
            ));
        }
        Ok(record)
    }

    async fn materialize(
        &self,
        connection: &mut sqlx::PgConnection,
        principal: &Principal,
        record: &CheckoutRecord,
    ) -> Result<(
        tempfile::TempDir,
        Box<dyn aster_core::workspace::WorkspaceCheckout>,
    )> {
        let temporary = tokio::task::spawn_blocking(tempfile::tempdir)
            .await
            .map_err(|_| CoreError::Storage("workspace directory task failed".into()))?
            .map_err(|_| CoreError::Storage("workspace directory unavailable".into()))?;
        let root = temporary.path().join("checkout");
        let result = async {
            self.restore_checkout_data(
                connection,
                principal,
                &record.workspace,
                &record.id,
                root.clone(),
            )
            .await?;
            let binding = serde_json::from_value(record.binding.clone())
                .map_err(|_| CoreError::Storage("invalid checkout binding".into()))?;
            let branch = record.branch.clone();
            self.checkouts.open(root, branch, binding).await
        }
        .await;
        match result {
            Ok(store) => Ok((temporary, store)),
            Err(error) => {
                close_directory(temporary).await;
                Err(error)
            }
        }
    }

    pub async fn export(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        workspace: &str,
        checkout: &str,
    ) -> Result<Vec<u8>> {
        let service = self.clone();
        let principal = principal.clone();
        let workspace = workspace.to_owned();
        let checkout = checkout.to_owned();
        tokio::spawn(async move {
            let mut tx = service.store.pool.begin().await.map_err(storage)?;
            sqlx::query_scalar::<_, i32>("SELECT 1 FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR SHARE")
                .bind(&workspace).bind(&principal.subject).fetch_optional(&mut *tx).await.map_err(storage)?
                .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
            let record = service.checked_checkout(&mut tx, &principal, &workspace, &checkout).await?;
            let (temporary, store) = service.materialize(&mut tx, &principal, &record).await?;
            let root = temporary.path().join("checkout");
            let result = tokio::task::spawn_blocking(move || crate::workspace_snapshot::portable_export(&root)).await
                .map_err(|_| CoreError::Storage("workspace export task failed".into()));
            drop(store);
            close_directory(temporary).await;
            tx.commit().await.map_err(storage)?;
            result?
        }).await.map_err(|_| CoreError::Storage("workspace export task failed".into()))?
    }

    pub async fn reconcile(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        workspace: &str,
    ) -> Result<WorkspaceRemoteOutcome> {
        let service = self.clone();
        let principal = principal.clone();
        let workspace = workspace.to_owned();
        tokio::spawn(async move { service.reconcile_inner(&principal, &workspace).await })
            .await
            .map_err(|_| CoreError::Storage("workspace reconciliation task failed".into()))?
    }

    async fn reconcile_inner(
        &self,
        principal: &Principal,
        workspace: &str,
    ) -> Result<WorkspaceRemoteOutcome> {
        aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        let pending: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT pending_operation FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR UPDATE"
        ).bind(workspace).bind(&principal.subject).fetch_optional(&mut *tx).await.map_err(storage)?
            .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
        let Some(pending) = pending else {
            return Ok(WorkspaceRemoteOutcome {
                checkout: None,
                pull_request: None,
            });
        };
        if pending["phase"] != "uncertain" {
            return Err(CoreError::Conflict(
                "remote operation may still be running; reconciliation is fenced".into(),
            ));
        }
        let checkout = pending["checkout"]
            .as_str()
            .ok_or_else(|| CoreError::Storage("invalid remote intent".into()))?;
        let record = self
            .checked_checkout(&mut tx, principal, workspace, checkout)
            .await?;
        if pending["branch"] != record.branch
            || pending["version"].as_u64() != Some(record.version as u64)
        {
            return Err(CoreError::Conflict(
                "remote intent no longer matches the checkout".into(),
            ));
        }
        let binding: crate::gitstore::NotebookWorkspaceBinding =
            serde_json::from_value(record.binding.clone())
                .map_err(|_| CoreError::Storage("invalid checkout binding".into()))?;
        let hosting = self
            .hosting
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("workspace hosting disabled".into()))?;
        let outcome = if pending["kind"] == "pr" {
            if pending["base"] != binding.default_branch {
                return Err(CoreError::Conflict("pull request base changed".into()));
            }
            let url = hosting
                .find_pull_request(
                    &record.repository,
                    binding.installation_id,
                    binding.repository_id,
                    &record.branch,
                    &binding.default_branch,
                )
                .await?
                .ok_or_else(|| {
                    CoreError::Conflict(
                        "pull request outcome is unresolved; retry lookup later".into(),
                    )
                })?;
            WorkspaceRemoteOutcome {
                checkout: None,
                pull_request: Some(url),
            }
        } else if pending["kind"] == "push" {
            let credential = hosting
                .credential(
                    &record.repository,
                    binding.installation_id,
                    binding.repository_id,
                )
                .await?;
            let (temporary, store) = self.materialize(&mut tx, principal, &record).await?;
            let result = async {
                let status = store.git_status().await?;
                if pending["local"] != status.head {
                    return Err(CoreError::Conflict("publication commit changed".into()));
                }
                store.published(credential, true).await?;
                let notebooks = store.draft_ids().await?;
                let version = self
                    .record_checkpoint(
                        &mut tx,
                        workspace,
                        checkout,
                        temporary.path().join("checkout"),
                        record.version as u64,
                    )
                    .await?;
                Ok(CheckoutView {
                    id: record.id,
                    workspace: record.workspace,
                    team: record.team,
                    repository: record.repository,
                    branch: status.branch,
                    version,
                    head: status.head,
                    dirty: status.dirty,
                    notebooks,
                    draft_revision: None,
                })
            }
            .await;
            drop(store);
            close_directory(temporary).await;
            WorkspaceRemoteOutcome {
                checkout: Some(result?),
                pull_request: None,
            }
        } else {
            return Err(CoreError::Conflict(
                "unknown remote operation requires operator review".into(),
            ));
        };
        sqlx::query("UPDATE durable_workspaces SET pending_operation=NULL WHERE id=$1")
            .bind(workspace)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(outcome)
    }

    pub async fn open_pull_request(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        writer: &WriterContext,
        checkout: &str,
        expected: u64,
        title: &str,
    ) -> Result<String> {
        let service = self.clone();
        let principal = principal.clone();
        let writer = writer.clone();
        let checkout = checkout.to_owned();
        let title = title.to_owned();
        tokio::spawn(async move {
            service
                .open_pull_request_inner(&principal, &writer, &checkout, expected, &title)
                .await
        })
        .await
        .map_err(|_| CoreError::Storage("workspace pull request task failed".into()))?
    }

    async fn open_pull_request_inner(
        &self,
        principal: &Principal,
        writer: &WriterContext,
        checkout: &str,
        expected: u64,
        title: &str,
    ) -> Result<String> {
        aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
        if title.trim().is_empty() || title.len() > 256 || title.chars().any(char::is_control) {
            return Err(CoreError::Invalid(
                "enter a pull request title of at most 256 bytes".into(),
            ));
        }
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        let (hash, epoch, pending): (String, i64, Option<serde_json::Value>) = sqlx::query_as(
            "SELECT writer_hash,epoch,pending_operation FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR UPDATE"
        ).bind(&writer.workspace).bind(&principal.subject).fetch_optional(&mut *tx).await.map_err(storage)?
            .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
        if hash != token_hash(&writer.token) || epoch as u64 != writer.epoch {
            return Err(CoreError::Conflict("workspace writer changed".into()));
        }
        let record = self
            .checked_checkout(&mut tx, principal, &writer.workspace, checkout)
            .await?;
        if record.version as u64 != expected {
            return Err(CoreError::Conflict(
                "checkout changed; reopen before opening a PR".into(),
            ));
        }
        let binding: crate::gitstore::NotebookWorkspaceBinding =
            serde_json::from_value(record.binding.clone())
                .map_err(|_| CoreError::Storage("invalid checkout binding".into()))?;
        let target = self
            .targets
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("workspace Git disabled".into()))?
            .repositories(&record.team, principal)
            .await?
            .into_iter()
            .find(|target| target.repository_id == record.repository_id)
            .ok_or_else(|| CoreError::Unauthorized("repository unavailable".into()))?;
        if target.default_branch != binding.default_branch {
            return Err(CoreError::Conflict(
                "approved base branch changed; review the repository configuration".into(),
            ));
        }
        let hosting = self
            .hosting
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("workspace hosting disabled".into()))?;
        if let Some(pending) = pending {
            if pending["kind"] != "pr"
                || pending["phase"] != "uncertain"
                || pending["checkout"] != checkout
                || pending["branch"] != record.branch
                || pending["base"] != binding.default_branch
            {
                return Err(CoreError::Conflict(
                    "another remote operation needs reconciliation".into(),
                ));
            }
            let url = hosting
                .find_pull_request(
                    &record.repository,
                    binding.installation_id,
                    binding.repository_id,
                    &record.branch,
                    &binding.default_branch,
                )
                .await?
                .ok_or_else(|| {
                    CoreError::Conflict(
                        "pull request outcome is unresolved; retry lookup later".into(),
                    )
                })?;
            sqlx::query("UPDATE durable_workspaces SET pending_operation=NULL WHERE id=$1")
                .bind(&writer.workspace)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            return Ok(url);
        }
        let credential = hosting
            .credential(
                &record.repository,
                binding.installation_id,
                binding.repository_id,
            )
            .await?;
        let (temporary, store) = self.materialize(&mut tx, principal, &record).await?;
        let published = store.published(credential, false).await;
        drop(store);
        close_directory(temporary).await;
        let head = published?;
        let operation = aster_core::new_sid()?;
        sqlx::query("UPDATE durable_workspaces SET pending_operation=$2 WHERE id=$1")
            .bind(&writer.workspace).bind(serde_json::json!({"id":operation,"kind":"pr","phase":"running","checkout":checkout,"branch":record.branch,"base":binding.default_branch,"local":head,"version":expected}))
            .execute(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        let mut may_have_written = true;
        let result = async {
            let url = hosting.open_pull_request(&record.repository, binding.installation_id, binding.repository_id, &record.branch, &binding.default_branch, title).await.map_err(|failure| {
                may_have_written = failure.may_have_written;
                failure.error
            })?;
            let changed = sqlx::query("UPDATE durable_workspaces SET pending_operation=NULL WHERE id=$1 AND pending_operation->>'id'=$2")
                .bind(&writer.workspace).bind(&operation).execute(&self.store.pool).await.map_err(storage)?;
            if changed.rows_affected() != 1 { return Err(CoreError::Conflict("pull request ownership changed".into())); }
            Ok(url)
        }.await;
        if result.is_err() && sqlx::query("UPDATE durable_workspaces SET pending_operation=CASE WHEN $3 THEN pending_operation || '{\"phase\":\"uncertain\"}'::jsonb ELSE NULL END WHERE id=$1 AND pending_operation->>'id'=$2")
            .bind(&writer.workspace).bind(&operation).bind(may_have_written).execute(&self.store.pool).await.is_err() {
            tracing::warn!("workspace pull request requires reconciliation");
        }
        result
    }

    pub async fn push_checkout(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        writer: &WriterContext,
        checkout: &str,
        expected: u64,
    ) -> Result<CheckoutView> {
        let service = self.clone();
        let principal = principal.clone();
        let writer = writer.clone();
        let checkout = checkout.to_owned();
        tokio::spawn(async move {
            service
                .push_checkout_inner(&principal, &writer, &checkout, expected)
                .await
        })
        .await
        .map_err(|_| CoreError::Storage("workspace publication task failed".into()))?
    }

    async fn push_checkout_inner(
        &self,
        principal: &Principal,
        writer: &WriterContext,
        checkout: &str,
        expected: u64,
    ) -> Result<CheckoutView> {
        aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
        let mut tx = self.lock_writer(principal, writer).await?;
        let record = self
            .checked_checkout(&mut tx, principal, &writer.workspace, checkout)
            .await?;
        if record.version as u64 != expected {
            return Err(CoreError::Conflict(
                "checkout changed; reopen before pushing".into(),
            ));
        }
        let binding: crate::gitstore::NotebookWorkspaceBinding =
            serde_json::from_value(record.binding.clone())
                .map_err(|_| CoreError::Storage("invalid checkout binding".into()))?;
        let hosting = self
            .hosting
            .as_ref()
            .ok_or_else(|| CoreError::Unauthorized("workspace hosting disabled".into()))?;
        let credential = hosting
            .credential(
                &record.repository,
                binding.installation_id,
                binding.repository_id,
            )
            .await?;
        let (temporary, store) = self.materialize(&mut tx, principal, &record).await?;
        let root = temporary.path().join("checkout");
        let operation = aster_core::new_sid()?;
        let mut published = false;
        let result = async {
            let status = store.git_status().await?;
            let intent = serde_json::json!({"id":operation,"kind":"push","phase":"running","checkout":checkout,"branch":record.branch,"local":status.head,"version":expected});
            sqlx::query("UPDATE durable_workspaces SET pending_operation=$2 WHERE id=$1")
                .bind(&writer.workspace).bind(intent).execute(&mut *tx).await.map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            let pool = self.store.pool.clone();
            let workspace = writer.workspace.clone();
            let nonce = operation.clone();
            let runtime = tokio::runtime::Handle::current();
            store.publish(credential, Box::new(move |local, remote| {
                runtime.block_on(async {
                    let updated = sqlx::query("UPDATE durable_workspaces SET pending_operation=pending_operation || $3::jsonb WHERE id=$1 AND pending_operation->>'id'=$2")
                        .bind(&workspace).bind(&nonce).bind(serde_json::json!({"local":local,"expected_remote":remote,"prepared":true}))
                        .execute(&pool).await.map_err(storage)?;
                    if updated.rows_affected() != 1 { return Err(CoreError::Conflict("publication ownership changed".into())); }
                    Ok(())
                })
            })).await?;
            published = true;
            let mut tx = self.store.pool.begin().await.map_err(storage)?;
            let pending: Option<serde_json::Value> = sqlx::query_scalar("SELECT pending_operation FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR UPDATE")
                .bind(&writer.workspace).bind(&principal.subject).fetch_one(&mut *tx).await.map_err(storage)?;
            if pending.as_ref().and_then(|value| value.get("id")).and_then(serde_json::Value::as_str) != Some(operation.as_str()) {
                return Err(CoreError::Conflict("publication ownership changed".into()));
            }
            let status = store.git_status().await?;
            let notebooks = store.draft_ids().await?;
            let version = self.record_checkpoint(&mut tx, &writer.workspace, checkout, root, expected).await?;
            sqlx::query("UPDATE durable_workspaces SET pending_operation=NULL WHERE id=$1")
                .bind(&writer.workspace).execute(&mut *tx).await.map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            Ok(CheckoutView { id:record.id, workspace:record.workspace, team:record.team, repository:record.repository,
                branch:status.branch, version, head:status.head, dirty:status.dirty, notebooks, draft_revision:None })
        }.await;
        if result.is_err() {
            // Only a completed preflight with no remote-write intent can release the fence without reconciliation.
            if sqlx::query("UPDATE durable_workspaces SET pending_operation=CASE WHEN $3 OR pending_operation->>'prepared'='true' THEN pending_operation || '{\"phase\":\"uncertain\"}'::jsonb ELSE NULL END WHERE id=$1 AND pending_operation->>'id'=$2")
                .bind(&writer.workspace).bind(&operation).bind(published).execute(&self.store.pool).await.is_err() {
                tracing::warn!("workspace publication requires remote reconciliation");
            }
        }
        drop(store);
        close_directory(temporary).await;
        result
    }

    pub async fn edit_checkout(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        writer: &WriterContext,
        checkout: &str,
        expected: u64,
        edit: CheckoutEdit,
    ) -> Result<CheckoutView> {
        let service = self.clone();
        let principal = principal.clone();
        let writer = writer.clone();
        let checkout = checkout.to_owned();
        tokio::spawn(async move {
            service
                .edit_checkout_inner(&principal, &writer, &checkout, expected, edit)
                .await
        })
        .await
        .map_err(|_| CoreError::Storage("workspace edit task failed".into()))?
    }

    async fn edit_checkout_inner(
        &self,
        principal: &Principal,
        writer: &WriterContext,
        checkout: &str,
        expected: u64,
        edit: CheckoutEdit,
    ) -> Result<CheckoutView> {
        aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
        let mut tx = self.lock_writer(principal, writer).await?;
        let record = self
            .checked_checkout(&mut tx, principal, &writer.workspace, checkout)
            .await?;
        if u64::try_from(record.version).ok() != Some(expected) {
            return Err(CoreError::Conflict(
                "checkout changed; reopen before editing".into(),
            ));
        }
        let (temporary, mut store) = self.materialize(&mut tx, principal, &record).await?;
        let root = temporary.path().join("checkout");
        let result = async {
            let mut draft_revision = None;
            match edit {
                CheckoutEdit::SaveDraft { notebook, expected } => {
                    draft_revision = Some(store.save_draft(&notebook, expected).await?.revision);
                }
                CheckoutEdit::Commit { notebooks, message } => {
                    store.commit_drafts(notebooks, message).await?;
                }
                CheckoutEdit::SwitchBranch { branch, create } => {
                    store = store.switch_branch(&branch, create).await?;
                }
                CheckoutEdit::Pull => {
                    let binding: crate::gitstore::NotebookWorkspaceBinding =
                        serde_json::from_value(record.binding.clone())
                            .map_err(|_| CoreError::Storage("invalid checkout binding".into()))?;
                    let hosting = self.hosting.as_ref().ok_or_else(|| {
                        CoreError::Unauthorized("workspace hosting disabled".into())
                    })?;
                    let credential = hosting
                        .credential(
                            &record.repository,
                            binding.installation_id,
                            binding.repository_id,
                        )
                        .await?;
                    store.pull(credential).await?;
                }
            }
            let status = store.git_status().await?;
            let notebooks = store.draft_ids().await?;
            sqlx::query("UPDATE durable_workspace_checkouts SET branch=$2 WHERE id=$1")
                .bind(checkout)
                .bind(&status.branch)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            let version = self
                .record_checkpoint(&mut tx, &writer.workspace, checkout, root, expected)
                .await?;
            tx.commit().await.map_err(storage)?;
            Ok(CheckoutView {
                id: record.id,
                workspace: record.workspace,
                team: record.team,
                repository: record.repository,
                branch: status.branch,
                version,
                head: status.head,
                dirty: status.dirty,
                notebooks,
                draft_revision,
            })
        }
        .await;
        drop(store);
        close_directory(temporary).await;
        result
    }

    pub async fn draft(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        workspace: &str,
        checkout: &str,
        notebook: &str,
    ) -> Result<DraftView> {
        let service = self.clone();
        let principal = principal.clone();
        let workspace = workspace.to_owned();
        let checkout = checkout.to_owned();
        let notebook = notebook.to_owned();
        tokio::spawn(async move {
            service
                .draft_inner(&principal, &workspace, &checkout, &notebook)
                .await
        })
        .await
        .map_err(|_| CoreError::Storage("workspace read task failed".into()))?
    }

    async fn draft_inner(
        &self,
        principal: &Principal,
        workspace: &str,
        checkout: &str,
        notebook: &str,
    ) -> Result<DraftView> {
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR SHARE",
        )
        .bind(workspace)
        .bind(&principal.subject)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
        let record = self
            .checked_checkout(&mut tx, principal, workspace, checkout)
            .await?;
        let (temporary, store) = self.materialize(&mut tx, principal, &record).await?;
        let result =
            async { Ok((store.draft(notebook).await?, store.git_status().await?.head)) }.await;
        drop(store);
        close_directory(temporary).await;
        tx.commit().await.map_err(storage)?;
        result.map(|(draft, head)| DraftView {
            notebook: draft.notebook,
            revision: draft.revision,
            checkout_version: record.version as u64,
            branch: record.branch,
            head,
        })
    }

    pub async fn checkout(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        workspace: &str,
        checkout: &str,
    ) -> Result<CheckoutView> {
        let service = self.clone();
        let principal = principal.clone();
        let workspace = workspace.to_owned();
        let checkout = checkout.to_owned();
        tokio::spawn(async move {
            service
                .checkout_inner(&principal, &workspace, &checkout)
                .await
        })
        .await
        .map_err(|_| CoreError::Storage("workspace read task failed".into()))?
    }

    async fn checkout_inner(
        &self,
        principal: &Principal,
        workspace: &str,
        checkout: &str,
    ) -> Result<CheckoutView> {
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR SHARE",
        )
        .bind(workspace)
        .bind(&principal.subject)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?
        .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
        let record = self
            .checked_checkout(&mut tx, principal, workspace, checkout)
            .await?;
        let (temporary, store) = self.materialize(&mut tx, principal, &record).await?;
        let result = async {
            let status = store.git_status().await?;
            let notebooks = store.draft_ids().await?;
            Ok(CheckoutView {
                id: record.id,
                workspace: record.workspace,
                team: record.team,
                repository: record.repository,
                branch: status.branch,
                version: record.version as u64,
                head: status.head,
                dirty: status.dirty,
                notebooks,
                draft_revision: None,
            })
        }
        .await;
        drop(store);
        close_directory(temporary).await;
        tx.commit().await.map_err(storage)?;
        result
    }

    pub async fn state(&self, principal: &Principal, workspace: &str) -> Result<WorkspaceState> {
        aster_core::authorize(principal, aster_core::Action::ReadNotebook)?;
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        let (layout, version): (serde_json::Value, i64) = sqlx::query_as("SELECT layout,layout_version FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR SHARE")
            .bind(workspace).bind(&principal.subject).fetch_optional(&mut *tx).await.map_err(storage)?
            .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
        let mut layout: WorkspaceLayout = serde_json::from_value(layout)
            .map_err(|_| CoreError::Storage("invalid workspace layout".into()))?;
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM durable_workspace_checkouts WHERE workspace=$1 ORDER BY id",
        )
        .bind(workspace)
        .fetch_all(&mut *tx)
        .await
        .map_err(storage)?;
        let (total_checkouts, bytes): (i64,i64) = sqlx::query_as("SELECT count(*)::bigint,COALESCE(sum(snapshot_bytes),0)::bigint FROM durable_workspace_checkouts WHERE workspace=$1")
            .bind(workspace).fetch_one(&mut *tx).await.map_err(storage)?;
        let mut checkouts = Vec::new();
        for id in ids {
            match self
                .checked_checkout(&mut tx, principal, workspace, &id)
                .await
            {
                Ok(_) => checkouts.push(id),
                Err(CoreError::Unauthorized(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let active = layout
            .active
            .and_then(|index| layout.tabs.get(index))
            .cloned();
        layout.tabs.retain(|tab| checkouts.contains(&tab.checkout));
        layout.active = active.and_then(|active| layout.tabs.iter().position(|tab| *tab == active));
        tx.commit().await.map_err(storage)?;
        Ok(WorkspaceState {
            checkouts,
            layout,
            layout_version: version as u64,
            recovery_bytes: bytes as u64,
            total_checkouts: total_checkouts as u64,
        })
    }

    pub async fn save_layout(
        &self,
        principal: &Principal,
        writer: &WriterContext,
        expected: u64,
        layout: WorkspaceLayout,
    ) -> Result<u64> {
        aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
        if layout.tabs.len() > 32
            || layout
                .active
                .is_some_and(|index| index >= layout.tabs.len())
        {
            return Err(CoreError::Invalid(
                "workspace layout supports at most 32 tabs and an existing active tab".into(),
            ));
        }
        let mut unique = std::collections::HashSet::new();
        let mut tx = self.lock_writer(principal, writer).await?;
        for tab in &layout.tabs {
            if tab.notebook.is_empty()
                || tab.notebook.len() > 64
                || !tab
                    .notebook
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                || !unique.insert((&tab.checkout, &tab.notebook))
            {
                return Err(CoreError::Invalid("invalid workspace tab".into()));
            }
            self.checked_checkout(&mut tx, principal, &writer.workspace, &tab.checkout)
                .await?;
        }
        let expected = i64::try_from(expected)
            .map_err(|_| CoreError::Invalid("invalid layout version".into()))?;
        let layout = serde_json::to_value(layout)
            .map_err(|_| CoreError::Invalid("invalid workspace layout".into()))?;
        let version: i64 = sqlx::query_scalar("UPDATE durable_workspaces SET layout=$2,layout_version=layout_version+1 WHERE id=$1 AND layout_version=$3 RETURNING layout_version")
            .bind(&writer.workspace).bind(layout).bind(expected).fetch_optional(&mut *tx).await.map_err(storage)?
            .ok_or_else(|| CoreError::Conflict("workspace layout changed; reopen before saving".into()))?;
        tx.commit().await.map_err(storage)?;
        Ok(version as u64)
    }

    pub async fn list(&self, principal: &Principal) -> Result<Vec<WorkspaceView>> {
        aster_core::authorize(principal, aster_core::Action::ReadNotebook)?;
        let rows = sqlx::query_as::<_, WorkspaceRecord>("SELECT id,subject,name,writer_hash,epoch,deleted FROM durable_workspaces WHERE subject=$1 AND (NOT deleted OR cleanup_at IS NULL) ORDER BY created_at,id")
            .bind(&principal.subject).fetch_all(&self.store.pool).await.map_err(storage)?;
        rows.into_iter().map(|row| view(row, None)).collect()
    }

    pub async fn delete(
        self: &std::sync::Arc<Self>,
        principal: &Principal,
        writer: &WriterContext,
    ) -> Result<()> {
        let service = self.clone();
        let principal = principal.clone();
        let writer = writer.clone();
        tokio::spawn(async move {
            aster_core::authorize(&principal, aster_core::Action::WriteNotebook)?;
            let mut tx = service.store.pool.begin().await.map_err(storage)?;
            let (hash, epoch, pending): (String, i64, Option<serde_json::Value>) = sqlx::query_as(
                "SELECT writer_hash,epoch,pending_operation FROM durable_workspaces WHERE id=$1 AND subject=$2 FOR UPDATE"
            ).bind(&writer.workspace).bind(&principal.subject).fetch_optional(&mut *tx).await.map_err(storage)?
                .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
            if hash != token_hash(&writer.token) || epoch as u64 != writer.epoch || pending.is_some() {
                return Err(CoreError::Conflict("workspace writer changed or a remote operation needs reconciliation".into()));
            }
            sqlx::query("UPDATE durable_workspaces SET deleted=TRUE,cleanup_at=NULL,layout='{}' WHERE id=$1")
                .bind(&writer.workspace).execute(&mut *tx).await.map_err(storage)?;
            tx.commit().await.map_err(storage)?;
            service.collect_deleted_workspace(&writer.workspace).await
        }).await.map_err(|_| CoreError::Storage("workspace deletion task failed".into()))?
    }

    async fn collect_deleted_workspace(&self, workspace: &str) -> Result<()> {
        let recovery = self
            .recovery
            .as_ref()
            .ok_or_else(|| CoreError::Storage("workspace recovery disabled".into()))?;
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        let deleted: Option<bool> =
            sqlx::query_scalar("SELECT deleted FROM durable_workspaces WHERE id=$1 FOR UPDATE")
                .bind(workspace)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
        if deleted != Some(true) {
            return Err(CoreError::Conflict("workspace is not deleted".into()));
        }
        for key in recovery.list(&format!("workspaces/{workspace}")).await? {
            recovery.delete(&key).await?;
        }
        sqlx::query("DELETE FROM durable_workspace_checkouts WHERE workspace=$1")
            .bind(workspace)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        sqlx::query("UPDATE durable_workspaces SET cleanup_at=now(),name='' WHERE id=$1")
            .bind(workspace)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        tx.commit().await.map_err(storage)
    }

    pub fn start_maintenance(self: &std::sync::Arc<Self>) {
        let service = std::sync::Arc::downgrade(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let Some(service) = service.upgrade() else {
                    break;
                };
                let rows = sqlx::query_scalar::<_, String>("SELECT id FROM durable_workspaces WHERE deleted AND (cleanup_at IS NULL OR cleanup_at < now() - interval '1 hour') ORDER BY cleanup_at NULLS FIRST,id LIMIT 100")
                    .fetch_all(&service.store.pool).await;
                match rows {
                    Ok(rows) => {
                        for id in rows {
                            if service.collect_deleted_workspace(&id).await.is_err() {
                                tracing::warn!("workspace deletion cleanup will retry");
                            }
                        }
                    }
                    Err(_) => tracing::warn!("workspace deletion cleanup unavailable"),
                }
            }
        });
    }

    pub async fn create(&self, principal: &Principal, name: &str) -> Result<WorkspaceView> {
        aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
        if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
            return Err(CoreError::Invalid(
                "workspace name must contain 1–128 printable bytes".into(),
            ));
        }
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(&principal.subject)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM durable_workspaces WHERE subject=$1 AND (NOT deleted OR cleanup_at IS NULL)")
            .bind(&principal.subject).fetch_one(&mut *tx).await.map_err(storage)?;
        if count >= MAX_WORKSPACES as i64 {
            return Err(CoreError::Conflict(
                "workspace quota exceeded; delete a workspace before creating another".into(),
            ));
        }
        let id = aster_core::new_sid()?;
        let token = aster_core::new_sid()?;
        let row = sqlx::query_as::<_, WorkspaceRecord>(
            "INSERT INTO durable_workspaces (id, subject, name, writer_hash, epoch) VALUES ($1,$2,$3,$4,1) RETURNING id, subject, name, writer_hash, epoch, deleted",
        ).bind(&id).bind(&principal.subject).bind(name).bind(token_hash(&token))
            .fetch_one(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        let mut result = view(row, Some(&token))?;
        result.writer_token = Some(token);
        Ok(result)
    }

    pub async fn takeover(
        &self,
        principal: &Principal,
        id: &str,
        expected_epoch: u64,
    ) -> Result<WorkspaceView> {
        aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
        let expected = i64::try_from(expected_epoch)
            .map_err(|_| CoreError::Invalid("invalid workspace epoch".into()))?;
        let mut tx = self.store.pool.begin().await.map_err(storage)?;
        // Every workspace mutation holds this row lock; ambiguous remote operations keep it fenced.
        let pending = sqlx::query_scalar::<_, Option<serde_json::Value>>(
            "SELECT pending_operation FROM durable_workspaces WHERE id=$1 AND subject=$2 AND NOT deleted FOR UPDATE"
        ).bind(id).bind(&principal.subject).fetch_optional(&mut *tx).await.map_err(storage)?
            .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
        if pending.is_some() {
            return Err(CoreError::Conflict(
                "remote operation must be reconciled before takeover".into(),
            ));
        }
        let token = aster_core::new_sid()?;
        let row = sqlx::query_as::<_, WorkspaceRecord>(
            "UPDATE durable_workspaces SET writer_hash=$3, epoch=epoch+1 WHERE id=$1 AND subject=$2 AND epoch=$4 RETURNING id, subject, name, writer_hash, epoch, deleted"
        ).bind(id).bind(&principal.subject).bind(token_hash(&token)).bind(expected)
            .fetch_optional(&mut *tx).await.map_err(storage)?
            .ok_or_else(|| CoreError::Conflict("workspace writer changed; reopen before takeover".into()))?;
        tx.commit().await.map_err(storage)?;
        let mut result = view(row, Some(&token))?;
        result.writer_token = Some(token);
        Ok(result)
    }

    pub async fn open(
        &self,
        principal: &Principal,
        id: &str,
        token: Option<&str>,
    ) -> Result<WorkspaceView> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CoreError::Invalid("invalid workspace ID".into()));
        }
        let row = sqlx::query_as::<_, WorkspaceRecord>(
            "SELECT id, subject, name, CASE WHEN pending_operation IS NULL THEN writer_hash ELSE '' END AS writer_hash, epoch, deleted FROM durable_workspaces WHERE id = $1 AND subject = $2 AND NOT deleted",
        ).bind(id).bind(&principal.subject).fetch_optional(&self.store.pool).await.map_err(storage)?
            .ok_or_else(|| CoreError::Unauthorized("workspace unavailable".into()))?;
        if row.subject != principal.subject {
            return Err(CoreError::Unauthorized("workspace unavailable".into()));
        }
        let mut result = view(row, token)?;
        result.read_only |=
            aster_core::authorize(principal, aster_core::Action::WriteNotebook).is_err();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_core::WorkspaceRecovery;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    struct UnavailableRecovery {
        inner: crate::workspace_recovery::ObjectRecovery,
        fail: AtomicBool,
        fail_cleanup: AtomicBool,
    }
    #[async_trait::async_trait]
    impl WorkspaceRecovery for UnavailableRecovery {
        async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(CoreError::Storage("fixture unavailable".into()));
            }
            self.inner.put(key, bytes).await
        }
        async fn get(&self, key: &str, limit: usize) -> Result<Vec<u8>> {
            self.inner.get(key, limit).await
        }
        async fn delete(&self, key: &str) -> Result<()> {
            self.inner.delete(key).await
        }
        async fn list(&self, prefix: &str) -> Result<Vec<String>> {
            if self.fail_cleanup.load(Ordering::SeqCst) {
                return Err(CoreError::Storage("fixture cleanup unavailable".into()));
            }
            self.inner.list(prefix).await
        }
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL from workspace-postgres.sh"]
    async fn postgres_workspace_failed_deletion_keeps_quota_until_cleanup() {
        let principal = Principal {
            subject: aster_core::new_sid().unwrap(),
            roles: vec![aster_core::Role::Editor],
            groups: vec![],
            user_uuid: None,
        };
        let recovery = Arc::new(UnavailableRecovery {
            inner: crate::workspace_recovery::ObjectRecovery {
                store: crate::workspace_recovery::fixture_store(),
            },
            fail: AtomicBool::new(false),
            fail_cleanup: AtomicBool::new(false),
        });
        let service = Arc::new(
            Workspaces::connect(&std::env::var("ASTER_TEST_METADATA_URL").unwrap())
                .await
                .unwrap()
                .with_recovery(recovery.clone()),
        );
        let first = service
            .create(&principal, "Pending deletion")
            .await
            .unwrap();
        let writer = WriterContext {
            workspace: first.id,
            token: first.writer_token.unwrap(),
            epoch: first.epoch,
        };
        for _ in 1..MAX_WORKSPACES {
            service.create(&principal, "Retained").await.unwrap();
        }
        let key = format!("workspaces/{}/orphan/data", writer.workspace);
        recovery.put(&key, vec![1]).await.unwrap();
        recovery.fail_cleanup.store(true, Ordering::SeqCst);
        assert!(service.delete(&principal, &writer).await.is_err());
        assert!(
            service.create(&principal, "Quota bypass").await.is_err(),
            "failed deletion must remain charged against owner quota"
        );
        recovery.fail_cleanup.store(false, Ordering::SeqCst);
        service.delete(&principal, &writer).await.unwrap();
        service.create(&principal, "After cleanup").await.unwrap();
        recovery.put(&key, vec![2]).await.unwrap();
        service
            .collect_deleted_workspace(&writer.workspace)
            .await
            .unwrap();
        assert!(
            recovery.get(&key, 100).await.is_err(),
            "tombstones must collect late uploads"
        );
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL from workspace-postgres.sh"]
    async fn postgres_workspace_owner_quota_is_atomic() {
        let principal = Principal {
            subject: aster_core::new_sid().unwrap(),
            roles: vec![aster_core::Role::Editor],
            groups: vec![],
            user_uuid: None,
        };
        let service = Arc::new(
            Workspaces::connect(&std::env::var("ASTER_TEST_METADATA_URL").unwrap())
                .await
                .unwrap(),
        );
        let mut tasks = Vec::new();
        for _ in 0..12 {
            let service = service.clone();
            let principal = principal.clone();
            tasks.push(tokio::spawn(async move {
                service.create(&principal, "Quota").await
            }));
        }
        let mut created = 0;
        for task in tasks {
            if task.await.unwrap().is_ok() {
                created += 1;
            }
        }
        assert_eq!(
            created, 10,
            "concurrent creation must enforce the owner workspace quota"
        );
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL from workspace-postgres.sh"]
    async fn postgres_workspace_lifecycle_lists_only_owner_and_deletes_recovery_data() {
        let principal = Principal {
            subject: aster_core::new_sid().unwrap(),
            roles: vec![aster_core::Role::Editor],
            groups: vec![],
            user_uuid: None,
        };
        let stranger = Principal {
            subject: aster_core::new_sid().unwrap(),
            ..principal.clone()
        };
        let recovery = Arc::new(crate::workspace_recovery::ObjectRecovery {
            store: crate::workspace_recovery::fixture_store(),
        });
        let service = Arc::new(
            Workspaces::connect(&std::env::var("ASTER_TEST_METADATA_URL").unwrap())
                .await
                .unwrap()
                .with_recovery(recovery.clone()),
        );
        let created = service
            .create(&principal, "Keep until deleted")
            .await
            .unwrap();
        let writer = WriterContext {
            workspace: created.id.clone(),
            token: created.writer_token.unwrap(),
            epoch: created.epoch,
        };
        let listed = service.list(&principal).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, writer.workspace);
        assert!(listed[0].writer_token.is_none());
        assert!(service.list(&stranger).await.unwrap().is_empty());
        let key = format!("workspaces/{}/orphan/recovery", writer.workspace);
        recovery
            .put(&key, b"recoverable data".to_vec())
            .await
            .unwrap();
        assert!(service.delete(&stranger, &writer).await.is_err());
        assert!(recovery.get(&key, 100).await.is_ok());
        service.delete(&principal, &writer).await.unwrap();
        assert!(recovery
            .list(&format!("workspaces/{}", writer.workspace))
            .await
            .unwrap()
            .is_empty());
        assert!(service.list(&principal).await.unwrap().is_empty());
        assert!(service
            .open(&principal, &writer.workspace, Some(&writer.token))
            .await
            .is_err());
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL from workspace-postgres.sh"]
    async fn postgres_workspace_checkpoint_acknowledges_only_recoverable_data() {
        let recovery = Arc::new(UnavailableRecovery {
            inner: crate::workspace_recovery::ObjectRecovery {
                store: crate::workspace_recovery::fixture_store(),
            },
            fail: AtomicBool::new(false),
            fail_cleanup: AtomicBool::new(false),
        });
        let service = Workspaces::connect(&std::env::var("ASTER_TEST_METADATA_URL").unwrap())
            .await
            .unwrap()
            .with_recovery(recovery.clone());
        let principal = Principal {
            subject: "checkpoint-alice".into(),
            roles: vec![aster_core::Role::Editor],
            groups: vec![],
            user_uuid: None,
        };
        let created = service.create(&principal, "Durability").await.unwrap();
        let writer = WriterContext {
            workspace: created.id,
            token: created.writer_token.unwrap(),
            epoch: created.epoch,
        };
        let checkout = aster_core::new_sid().unwrap();
        sqlx::query("INSERT INTO durable_workspace_checkouts(id, workspace, team, repository_id, binding, branch) VALUES ($1,$2,'alpha',1,'{}','alice/analysis')")
            .bind(&checkout).bind(&writer.workspace).execute(&service.store.pool).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        std::fs::write(
            root.path().join(".git/HEAD"),
            "ref: refs/heads/alice/analysis\n",
        )
        .unwrap();
        std::fs::write(root.path().join("draft.aster"), "SELECT 1").unwrap();
        assert_eq!(
            service
                .checkpoint(&principal, &writer, &checkout, root.path().to_path_buf(), 0)
                .await
                .unwrap(),
            1
        );
        std::fs::write(root.path().join("draft.aster"), "SELECT 2").unwrap();
        recovery.fail.store(true, Ordering::SeqCst);
        assert!(service
            .checkpoint(&principal, &writer, &checkout, root.path().to_path_buf(), 1)
            .await
            .is_err());
        let restored_parent = tempfile::tempdir().unwrap();
        let destination = restored_parent.path().join("restored");
        service
            .restore_checkout(
                &principal,
                &writer.workspace,
                &checkout,
                destination.clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(destination.join("draft.aster")).unwrap(),
            b"SELECT 1"
        );
        recovery.fail.store(false, Ordering::SeqCst);
        sqlx::query("ALTER TABLE durable_workspace_checkouts ADD CONSTRAINT fixture_version CHECK (version < 2)")
            .execute(&service.store.pool).await.unwrap();
        assert!(service
            .checkpoint(&principal, &writer, &checkout, root.path().to_path_buf(), 1)
            .await
            .is_err());
        sqlx::query("ALTER TABLE durable_workspace_checkouts DROP CONSTRAINT fixture_version")
            .execute(&service.store.pool)
            .await
            .unwrap();
        let after_failed_pointer = restored_parent.path().join("failed-pointer");
        service
            .restore_checkout(
                &principal,
                &writer.workspace,
                &checkout,
                after_failed_pointer.clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(after_failed_pointer.join("draft.aster")).unwrap(),
            b"SELECT 1"
        );
        assert_eq!(
            service
                .checkpoint(&principal, &writer, &checkout, root.path().to_path_buf(), 1)
                .await
                .unwrap(),
            2
        );
        assert_eq!(recovery.list(&format!("workspaces/{}/{}", writer.workspace, checkout)).await.unwrap().len(), 2,
            "checkpoint must collect orphan generations while retaining the prior durable generation");
        assert!(service
            .checkpoint(&principal, &writer, &checkout, root.path().to_path_buf(), 0)
            .await
            .is_err());
        service
            .takeover(&principal, &writer.workspace, 1)
            .await
            .unwrap();
        assert!(service
            .checkpoint(&principal, &writer, &checkout, root.path().to_path_buf(), 1)
            .await
            .is_err());
        let bob = Principal {
            subject: "checkpoint-bob".into(),
            ..principal.clone()
        };
        assert!(service
            .restore_checkout(&bob, &writer.workspace, &checkout, root.path().join("bob"))
            .await
            .is_err());
        sqlx::query("UPDATE durable_workspaces SET pending_operation='{}' WHERE id=$1")
            .bind(&writer.workspace)
            .execute(&service.store.pool)
            .await
            .unwrap();
        assert!(service
            .takeover(&principal, &writer.workspace, 2)
            .await
            .is_err());
        let key: String =
            sqlx::query_scalar("SELECT snapshot_key FROM durable_workspace_checkouts WHERE id=$1")
                .bind(&checkout)
                .fetch_one(&service.store.pool)
                .await
                .unwrap();
        use object_store::ObjectStoreExt;
        recovery
            .inner
            .store
            .put(
                &object_store::path::Path::from(key),
                b"tampered".to_vec().into(),
            )
            .await
            .unwrap();
        let corrupt = restored_parent.path().join("corrupt");
        assert!(service
            .restore_checkout(&principal, &writer.workspace, &checkout, corrupt.clone())
            .await
            .is_err());
        assert!(!corrupt.exists());
    }
}

#[cfg(test)]
mod checkout_tests {
    use super::*;
    use crate::team_git::{TeamGitPolicy, TeamGitTargets, TeamGitVerifier};
    use aster_core::workspace::{WorkspaceGitCredential, WorkspaceHosting};
    use std::{collections::HashMap, sync::Arc};

    #[derive(Default)]
    struct FixtureHosting {
        requests: std::sync::Mutex<HashMap<String, String>>,
        posts: std::sync::atomic::AtomicUsize,
        lose_ack: std::sync::atomic::AtomicBool,
        reject_pr: std::sync::atomic::AtomicBool,
    }
    #[async_trait::async_trait]
    impl TeamGitVerifier for FixtureHosting {
        async fn verified_default_commit(
            &self,
            _repository: &str,
            _installation: u64,
            _repository_id: u64,
            _branch: &str,
        ) -> Result<String> {
            Ok(std::env::var("ASTER_TEST_HTTPS_SEED_OID").unwrap())
        }
    }
    #[async_trait::async_trait]
    impl WorkspaceHosting for FixtureHosting {
        async fn credential(
            &self,
            repository: &str,
            installation: u64,
            repository_id: u64,
        ) -> Result<WorkspaceGitCredential> {
            assert_eq!(installation, 73);
            assert!([("acme/analytics", 101), ("acme/reports", 102)]
                .contains(&(repository, repository_id)));
            Ok(WorkspaceGitCredential {
                remote_url: std::env::var("ASTER_TEST_HTTPS_URL").unwrap(),
                token: std::env::var("ASTER_TEST_HTTPS_TOKEN").unwrap(),
                ca_bundle: Some(std::env::var("ASTER_TEST_HTTPS_CA").unwrap().into()),
            })
        }
        async fn find_pull_request(
            &self,
            _: &str,
            _: u64,
            _: u64,
            branch: &str,
            _: &str,
        ) -> Result<Option<String>> {
            Ok(self.requests.lock().unwrap().get(branch).cloned())
        }
        async fn open_pull_request(
            &self,
            repository: &str,
            _: u64,
            _: u64,
            branch: &str,
            base: &str,
            _: &str,
        ) -> std::result::Result<String, aster_core::workspace::WorkspacePullRequestFailure>
        {
            use std::sync::atomic::Ordering;
            assert_eq!(base, "main");
            if self.reject_pr.swap(false, Ordering::SeqCst) {
                return Err(aster_core::workspace::WorkspacePullRequestFailure {
                    error: CoreError::Invalid("fixture PR preflight rejected".into()),
                    may_have_written: false,
                });
            }
            let mut requests = self.requests.lock().unwrap();
            if let Some(url) = requests.get(branch) {
                return Ok(url.clone());
            }
            self.posts.fetch_add(1, Ordering::SeqCst);
            let url = format!("https://github.com/{repository}/pull/1");
            requests.insert(branch.into(), url.clone());
            if self.lose_ack.swap(false, Ordering::SeqCst) {
                return Err(aster_core::workspace::WorkspacePullRequestFailure {
                    error: CoreError::Storage("fixture lost PR acknowledgement".into()),
                    may_have_written: true,
                });
            }
            Ok(url)
        }
    }

    #[tokio::test]
    #[ignore = "requires workspace-recovery.py with HTTPS Git fixture"]
    async fn workspace_https_two_approved_repositories_have_independent_recoverable_checkouts() {
        let member = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let maintainer = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let principal = Principal {
            subject: "workspace-clone-alice".into(),
            roles: vec![aster_core::Role::Editor],
            groups: vec![member.into(), maintainer.into()],
            user_uuid: Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".into()),
        };
        let database = std::env::var("ASTER_TEST_METADATA_URL").unwrap();
        let team = format!("clone-{}", aster_core::new_sid().unwrap());
        let hosting = Arc::new(FixtureHosting::default());
        let targets = Arc::new(
            TeamGitTargets::connect(
                &database,
                HashMap::from([(
                    team.clone(),
                    TeamGitPolicy {
                        member_group_uuid: member.into(),
                        maintainer_group_uuid: maintainer.into(),
                        installation_id: 73,
                        allowed_repositories: HashMap::from([
                            ("acme/analytics".into(), 101),
                            ("acme/reports".into(), 102),
                        ]),
                    },
                )]),
                hosting.clone(),
            )
            .await
            .unwrap(),
        );
        for repository in ["acme/analytics", "acme/reports"] {
            targets
                .configure_repository(&team, &principal, repository, "main", None)
                .await
                .unwrap();
        }
        let service = Arc::new(
            Workspaces::connect(&database)
                .await
                .unwrap()
                .with_recovery(Arc::new(crate::workspace_recovery::ObjectRecovery {
                    store: crate::workspace_recovery::fixture_store(),
                }))
                .with_git(targets, hosting.clone()),
        );
        let created = service
            .create(&principal, "Multiple repositories")
            .await
            .unwrap();
        let writer = WriterContext {
            workspace: created.id,
            token: created.writer_token.unwrap(),
            epoch: created.epoch,
        };
        let first = service
            .clone_repository(&principal, &writer, &team, 101, "analysis")
            .await
            .unwrap();
        let second = service
            .clone_repository(&principal, &writer, &team, 102, "reports")
            .await
            .unwrap();
        assert_ne!(first.id, second.id);
        assert_ne!(first.branch, second.branch);
        assert_eq!(
            first.head,
            std::env::var("ASTER_TEST_HTTPS_SEED_OID").unwrap()
        );
        assert_eq!(first.version, 1);
        for checkout in [&first, &second] {
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("restored");
            service
                .restore_checkout(&principal, &writer.workspace, &checkout.id, path.clone())
                .await
                .unwrap();
            assert!(path.join(".git/shallow").exists());
            assert!(path.join("a.aster").exists());
        }
        assert!(service
            .clone_repository(&principal, &writer, &team, 999, "forbidden")
            .await
            .is_err());
        assert!(service
            .clone_repository(&principal, &writer, &team, 101, "duplicate")
            .await
            .is_err());
        let denied = Principal {
            groups: vec![],
            ..principal.clone()
        };
        let destination = tempfile::tempdir().unwrap();
        assert!(
            service
                .restore_checkout(
                    &denied,
                    &writer.workspace,
                    &first.id,
                    destination.path().join("revoked")
                )
                .await
                .is_err(),
            "revoked membership must deny recovery"
        );
        let notebook = aster_core::Notebook {
            engine: None,
            context: None,
            id: "workspace-sales".into(),
            title: "Uncommitted analysis".into(),
            cells: vec![],
        };
        let saved = service
            .edit_checkout(
                &principal,
                &writer,
                &first.id,
                1,
                CheckoutEdit::SaveDraft {
                    notebook: notebook.clone(),
                    expected: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(saved.head, first.head);
        assert!(saved.dirty);
        assert_eq!(
            service
                .draft(&principal, &writer.workspace, &first.id, "workspace-sales")
                .await
                .unwrap()
                .notebook
                .title,
            notebook.title
        );
        assert!(service
            .draft(&principal, &writer.workspace, &second.id, "workspace-sales")
            .await
            .is_err());
        let state = service.state(&principal, &writer.workspace).await.unwrap();
        assert_eq!(state.checkouts.len(), 2);
        assert!(state.recovery_bytes > 0);
        let layout = WorkspaceLayout {
            tabs: vec![WorkspaceTab {
                checkout: first.id.clone(),
                notebook: "workspace-sales".into(),
            }],
            active: Some(0),
        };
        let version = service
            .save_layout(&principal, &writer, state.layout_version, layout.clone())
            .await
            .unwrap();
        assert!(service
            .save_layout(&principal, &writer, state.layout_version, layout.clone())
            .await
            .is_err());
        let restored = service.state(&principal, &writer.workspace).await.unwrap();
        assert!(restored.layout == layout);
        assert_eq!(restored.layout_version, version);
        assert!(service
            .state(&denied, &writer.workspace)
            .await
            .unwrap()
            .checkouts
            .is_empty());
        assert!(service
            .state(&denied, &writer.workspace)
            .await
            .unwrap()
            .layout
            .tabs
            .is_empty());
        assert!(serde_json::from_value::<WorkspaceLayout>(
            serde_json::json!({"tabs":[],"active":null,"results":["secret"]})
        )
        .is_err());

        let single_connection = Arc::new(Workspaces {
            store: PgStore {
                pool: sqlx::postgres::PgPoolOptions::new()
                    .max_connections(1)
                    .connect(&database)
                    .await
                    .unwrap(),
            },
            checkouts: service.checkouts.clone(),
            recovery: service.recovery.clone(),
            targets: service.targets.clone(),
            hosting: service.hosting.clone(),
        });
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            single_connection.draft(&principal, &writer.workspace, &first.id, "workspace-sales"),
        )
        .await
        .expect("workspace reads must not reserve a second metadata connection")
        .unwrap();
        assert!(service
            .draft(&denied, &writer.workspace, &first.id, "workspace-sales")
            .await
            .is_err());
        let viewer = Principal {
            roles: vec![aster_core::Role::Viewer],
            ..principal.clone()
        };
        assert!(
            service
                .open(&viewer, &writer.workspace, Some(&writer.token))
                .await
                .unwrap()
                .read_only,
            "current Viewer authority must make an old writer browser read-only"
        );
        assert!(service
            .edit_checkout(
                &viewer,
                &writer,
                &first.id,
                saved.version,
                CheckoutEdit::Commit {
                    notebooks: vec!["workspace-sales".into()],
                    message: "Analysis".into()
                }
            )
            .await
            .is_err());
        let committed = service
            .edit_checkout(
                &principal,
                &writer,
                &first.id,
                saved.version,
                CheckoutEdit::Commit {
                    notebooks: vec!["workspace-sales".into()],
                    message: "Analysis".into(),
                },
            )
            .await
            .unwrap();
        assert_ne!(committed.head, first.head);
        assert!(service
            .export(&denied, &writer.workspace, &first.id)
            .await
            .is_err());
        let export = service
            .export(&principal, &writer.workspace, &first.id)
            .await
            .unwrap();
        let exported_head = tokio::task::spawn_blocking(move || {
            let directory = tempfile::tempdir().unwrap();
            let archive = directory.path().join("checkout.tar");
            std::fs::write(&archive, export).unwrap();
            let destination = directory.path().join("checkout");
            std::fs::create_dir(&destination).unwrap();
            assert!(std::process::Command::new("tar")
                .args(["-xf"])
                .arg(archive)
                .arg("-C")
                .arg(&destination)
                .status()
                .unwrap()
                .success());
            assert!(destination.join("workspace-sales.aster").exists());
            let output = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&destination)
                .output()
                .unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        })
        .await
        .unwrap();
        assert_eq!(
            exported_head, committed.head,
            "export must retain unpublished local commits for external resolution"
        );
        assert!(!committed.dirty);
        assert!(service
            .edit_checkout(
                &principal,
                &writer,
                &first.id,
                saved.version,
                CheckoutEdit::Commit {
                    notebooks: vec!["workspace-sales".into()],
                    message: "Stale".into()
                }
            )
            .await
            .is_err());
        assert!(service
            .open_pull_request(
                &principal,
                &writer,
                &first.id,
                committed.version,
                "Analysis"
            )
            .await
            .is_err());
        assert_eq!(hosting.posts.load(std::sync::atomic::Ordering::SeqCst), 0);
        let pause = std::env::var("ASTER_TEST_HTTPS_PAUSE_PUSH").unwrap();
        let started = std::env::var("ASTER_TEST_HTTPS_PUSH_STARTED").unwrap();
        tokio::fs::write(&pause, b"pause").await.unwrap();
        let request = tokio::spawn({
            let service = service.clone();
            let principal = principal.clone();
            let writer = writer.clone();
            let checkout = first.id.clone();
            async move {
                service
                    .push_checkout(&principal, &writer, &checkout, committed.version)
                    .await
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            while tokio::fs::metadata(&started).await.is_err() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("remote Push must reach the pause barrier");
        request.abort();
        assert!(
            service
                .takeover(&principal, &writer.workspace, writer.epoch)
                .await
                .is_err(),
            "takeover must not race a delayed remote Push"
        );
        assert!(
            service
                .reconcile(&principal, &writer.workspace)
                .await
                .is_err(),
            "a delayed Push must not be unfenced"
        );
        tokio::fs::remove_file(pause).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            while service
                .open(&principal, &writer.workspace, Some(&writer.token))
                .await
                .unwrap()
                .read_only
            {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("owned Push must finish after its HTTP caller is cancelled");
        let pushed = service
            .checkout(&principal, &writer.workspace, &first.id)
            .await
            .unwrap();
        assert_eq!(pushed.head, committed.head);
        assert_eq!(pushed.version, committed.version + 1);
        hosting
            .reject_pr
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(service
            .open_pull_request(&principal, &writer, &first.id, pushed.version, "Analysis")
            .await
            .is_err());
        assert!(
            !service
                .open(&principal, &writer.workspace, Some(&writer.token))
                .await
                .unwrap()
                .read_only,
            "a definitively rejected PR must leave the workspace editable"
        );
        hosting
            .lose_ack
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(service
            .open_pull_request(&principal, &writer, &first.id, pushed.version, "Analysis")
            .await
            .is_err());
        assert!(
            service
                .open(&principal, &writer.workspace, Some(&writer.token))
                .await
                .unwrap()
                .read_only,
            "uncertain PR creation must fence another mutation"
        );
        assert!(service
            .takeover(&principal, &writer.workspace, writer.epoch)
            .await
            .is_err());
        let url = service
            .open_pull_request(&principal, &writer, &first.id, pushed.version, "Analysis")
            .await
            .unwrap();
        assert_eq!(url, "https://github.com/acme/analytics/pull/1");
        assert_eq!(
            hosting.posts.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "retry must only look up the uncertain PR"
        );
        assert!(
            !service
                .open(&principal, &writer.workspace, Some(&writer.token))
                .await
                .unwrap()
                .read_only
        );

        assert!(service
            .push_checkout(&denied, &writer, &first.id, pushed.version)
            .await
            .is_err());
        let next_branch = format!("{}/second", first.branch.rsplit_once('/').unwrap().0);
        let switched = service
            .edit_checkout(
                &principal,
                &writer,
                &first.id,
                pushed.version,
                CheckoutEdit::SwitchBranch {
                    branch: next_branch.clone(),
                    create: true,
                },
            )
            .await
            .unwrap();
        assert_eq!(switched.branch, next_branch);
        assert_eq!(
            service
                .checkout(&principal, &writer.workspace, &first.id)
                .await
                .unwrap()
                .branch,
            next_branch
        );
        assert_eq!(
            service
                .draft(&principal, &writer.workspace, &first.id, "workspace-sales")
                .await
                .unwrap()
                .branch,
            next_branch
        );
        let returned = service
            .edit_checkout(
                &principal,
                &writer,
                &first.id,
                switched.version,
                CheckoutEdit::SwitchBranch {
                    branch: first.branch.clone(),
                    create: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(returned.head, committed.head);
        let draft = service
            .draft(&principal, &writer.workspace, &first.id, "workspace-sales")
            .await
            .unwrap();
        let changed = aster_core::Notebook {
            title: "Pending publication".into(),
            ..notebook.clone()
        };
        let changed = service
            .edit_checkout(
                &principal,
                &writer,
                &first.id,
                returned.version,
                CheckoutEdit::SaveDraft {
                    notebook: changed,
                    expected: Some(draft.revision),
                },
            )
            .await
            .unwrap();
        let committed = service
            .edit_checkout(
                &principal,
                &writer,
                &first.id,
                changed.version,
                CheckoutEdit::Commit {
                    notebooks: vec!["workspace-sales".into()],
                    message: "Pending publication".into(),
                },
            )
            .await
            .unwrap();
        sqlx::query(&format!("ALTER TABLE durable_workspace_checkouts ADD CONSTRAINT fixture_push_checkpoint CHECK (id <> '{}' OR version <= {})", first.id, committed.version))
            .execute(&service.store.pool).await.unwrap();
        let failed = service
            .push_checkout(&principal, &writer, &first.id, committed.version)
            .await;
        sqlx::query(
            "ALTER TABLE durable_workspace_checkouts DROP CONSTRAINT fixture_push_checkpoint",
        )
        .execute(&service.store.pool)
        .await
        .unwrap();
        assert!(failed.is_err());
        assert!(
            service
                .open(&principal, &writer.workspace, Some(&writer.token))
                .await
                .unwrap()
                .read_only
        );
        let recovered_service = Arc::new(
            Workspaces::connect(&database)
                .await
                .unwrap()
                .with_recovery(service.recovery.clone().unwrap())
                .with_git(
                    service.targets.clone().unwrap(),
                    service.hosting.clone().unwrap(),
                ),
        );
        assert!(recovered_service
            .reconcile(&denied, &writer.workspace)
            .await
            .is_err());
        sqlx::query("UPDATE durable_workspaces SET pending_operation=pending_operation || '{\"phase\":\"running\"}'::jsonb WHERE id=$1")
            .bind(&writer.workspace).execute(&service.store.pool).await.unwrap();
        assert!(
            recovered_service
                .reconcile(&principal, &writer.workspace)
                .await
                .is_err(),
            "a possibly running child must remain fenced"
        );
        sqlx::query("UPDATE durable_workspaces SET pending_operation=pending_operation || '{\"phase\":\"uncertain\"}'::jsonb WHERE id=$1")
            .bind(&writer.workspace).execute(&service.store.pool).await.unwrap();
        let outcome = recovered_service
            .reconcile(&principal, &writer.workspace)
            .await
            .unwrap();
        let acknowledged = outcome.checkout.unwrap();
        assert_eq!(acknowledged.head, committed.head);
        assert_eq!(acknowledged.version, committed.version + 1);
        assert!(
            !service
                .open(&principal, &writer.workspace, Some(&writer.token))
                .await
                .unwrap()
                .read_only
        );
        let takeover = service
            .takeover(&principal, &writer.workspace, writer.epoch)
            .await
            .unwrap();
        assert!(service
            .edit_checkout(
                &principal,
                &writer,
                &first.id,
                acknowledged.version,
                CheckoutEdit::SaveDraft {
                    notebook,
                    expected: None
                }
            )
            .await
            .is_err());
        let replacement = WriterContext {
            workspace: writer.workspace.clone(),
            token: takeover.writer_token.unwrap(),
            epoch: takeover.epoch,
        };
        let reference = format!("refs/heads/{}", first.branch);
        let bare = std::env::var("ASTER_TEST_HTTPS_BARE").unwrap();
        let advanced = tokio::task::spawn_blocking({
            let bare = bare.clone();
            let head = acknowledged.head.clone();
            let reference = reference.clone();
            move || {
                let git = |args: &[&str]| {
                    let output = std::process::Command::new("git")
                        .args([
                            "-c",
                            "user.name=Fixture",
                            "-c",
                            "user.email=fixture@example.invalid",
                        ])
                        .args(args)
                        .current_dir(&bare)
                        .output()
                        .unwrap();
                    assert!(output.status.success());
                    String::from_utf8(output.stdout).unwrap().trim().to_owned()
                };
                let tree = git(&["rev-parse", &format!("{head}^{{tree}}")]);
                let advanced = git(&["commit-tree", &tree, "-p", &head, "-m", "External advance"]);
                git(&["update-ref", &reference, &advanced]);
                advanced
            }
        })
        .await
        .unwrap();
        assert!(service
            .push_checkout(&principal, &replacement, &first.id, acknowledged.version)
            .await
            .is_err());
        assert!(
            !service
                .open(&principal, &writer.workspace, Some(&replacement.token))
                .await
                .unwrap()
                .read_only,
            "a rejected preflight must not fence the Pull required for recovery"
        );
        let pulled = service
            .edit_checkout(
                &principal,
                &replacement,
                &first.id,
                acknowledged.version,
                CheckoutEdit::Pull,
            )
            .await
            .unwrap();
        assert_eq!(pulled.head, advanced);
        assert!(std::process::Command::new("git")
            .args(["update-ref", "-d", &reference])
            .current_dir(&bare)
            .status()
            .unwrap()
            .success());
        assert!(
            service
                .push_checkout(&principal, &replacement, &first.id, pulled.version)
                .await
                .is_err(),
            "restored publication history must refuse remote branch recreation"
        );
        assert!(!std::process::Command::new("git")
            .args(["show-ref", "--verify", "--quiet", &reference])
            .current_dir(&bare)
            .status()
            .unwrap()
            .success());
    }
}

#[cfg(test)]
mod read_cancellation_tests {
    use super::*;
    use crate::team_git::{TeamGitPolicy, TeamGitTargets, TeamGitVerifier};
    use aster_core::{
        workspace::{WorkspaceGitCredential, WorkspaceHosting},
        WorkspaceRecovery,
    };
    use std::{collections::HashMap, sync::Arc, time::Duration};

    struct Hosting;
    #[async_trait::async_trait]
    impl TeamGitVerifier for Hosting {
        async fn verified_default_commit(
            &self,
            _: &str,
            _: u64,
            _: u64,
            _: &str,
        ) -> Result<String> {
            Ok(std::env::var("ASTER_TEST_HTTPS_SEED_OID").unwrap())
        }
    }
    #[async_trait::async_trait]
    impl WorkspaceHosting for Hosting {
        async fn credential(&self, _: &str, _: u64, _: u64) -> Result<WorkspaceGitCredential> {
            Ok(WorkspaceGitCredential {
                remote_url: std::env::var("ASTER_TEST_HTTPS_URL").unwrap(),
                token: std::env::var("ASTER_TEST_HTTPS_TOKEN").unwrap(),
                ca_bundle: Some(std::env::var("ASTER_TEST_HTTPS_CA").unwrap().into()),
            })
        }
        async fn find_pull_request(
            &self,
            _: &str,
            _: u64,
            _: u64,
            _: &str,
            _: &str,
        ) -> Result<Option<String>> {
            Ok(None)
        }
        async fn open_pull_request(
            &self,
            _: &str,
            _: u64,
            _: u64,
            _: &str,
            _: &str,
            _: &str,
        ) -> std::result::Result<String, aster_core::workspace::WorkspacePullRequestFailure>
        {
            unreachable!()
        }
    }

    struct PausedRecovery {
        inner: Arc<dyn WorkspaceRecovery>,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    #[async_trait::async_trait]
    impl WorkspaceRecovery for PausedRecovery {
        async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
            self.inner.put(key, bytes).await
        }
        async fn get(&self, key: &str, limit: usize) -> Result<Vec<u8>> {
            self.entered.notify_one();
            self.release.notified().await;
            self.inner.get(key, limit).await
        }
        async fn delete(&self, key: &str) -> Result<()> {
            self.inner.delete(key).await
        }
        async fn list(&self, prefix: &str) -> Result<Vec<String>> {
            self.inner.list(prefix).await
        }
    }

    async fn cancelled_read_retains_operation_until_cleanup(read_draft: bool) {
        let database = std::env::var("ASTER_TEST_METADATA_URL").unwrap();
        let team = format!("read-cancel-{}", aster_core::new_sid().unwrap());
        let member = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let maintainer = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let principal = Principal {
            subject: team.clone(),
            roles: vec![aster_core::Role::Editor],
            groups: vec![member.into(), maintainer.into()],
            user_uuid: Some("cccccccc-cccc-4ccc-8ccc-cccccccccccc".into()),
        };
        let hosting = Arc::new(Hosting);
        let targets = Arc::new(
            TeamGitTargets::connect(
                &database,
                HashMap::from([(
                    team.clone(),
                    TeamGitPolicy {
                        member_group_uuid: member.into(),
                        maintainer_group_uuid: maintainer.into(),
                        installation_id: 73,
                        allowed_repositories: HashMap::from([("acme/analytics".into(), 101)]),
                    },
                )]),
                hosting.clone(),
            )
            .await
            .unwrap(),
        );
        targets
            .configure_repository(&team, &principal, "acme/analytics", "main", None)
            .await
            .unwrap();
        let recovery = Arc::new(crate::workspace_recovery::ObjectRecovery {
            store: crate::workspace_recovery::fixture_store(),
        });
        let initial = Arc::new(
            Workspaces::connect(&database)
                .await
                .unwrap()
                .with_recovery(recovery.clone())
                .with_git(targets.clone(), hosting.clone()),
        );
        let workspace = initial
            .create(&principal, "Cancellation fixture")
            .await
            .unwrap();
        let writer = WriterContext {
            workspace: workspace.id.clone(),
            token: workspace.writer_token.unwrap(),
            epoch: workspace.epoch,
        };
        let checkout = initial
            .clone_repository(&principal, &writer, &team, 101, "analysis")
            .await
            .unwrap();
        let paused = Arc::new(PausedRecovery {
            inner: recovery,
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let service = Arc::new(
            Workspaces::connect(&database)
                .await
                .unwrap()
                .with_recovery(paused.clone())
                .with_git(targets, hosting),
        );
        let reader_service = service.clone();
        let reader_principal = principal.clone();
        let workspace_id = workspace.id.clone();
        let reader = tokio::spawn(async move {
            if read_draft {
                reader_service
                    .draft(&reader_principal, &workspace_id, &checkout.id, "a")
                    .await
                    .map(|_| ())
            } else {
                reader_service
                    .checkout(&reader_principal, &workspace_id, &checkout.id)
                    .await
                    .map(|_| ())
            }
        });
        tokio::time::timeout(Duration::from_secs(5), paused.entered.notified())
            .await
            .expect("read must reach recovery after acquiring its workspace lock");
        reader.abort();
        assert!(reader.await.unwrap_err().is_cancelled());
        let takeover_service = service.clone();
        let mut takeover = tokio::spawn(async move {
            takeover_service
                .takeover(&principal, &workspace.id, workspace.epoch)
                .await
        });
        let early = tokio::time::timeout(Duration::from_millis(150), &mut takeover).await;
        let retained_ownership = early.is_err();
        paused.release.notify_one();
        let result = match early {
            Ok(result) => result,
            Err(_) => tokio::time::timeout(Duration::from_secs(5), takeover)
                .await
                .expect("completed read must release ownership"),
        };
        result.unwrap().unwrap();
        assert!(retained_ownership, "cancelled read released workspace ownership before materialization and cleanup finished");
    }

    #[tokio::test]
    #[ignore = "requires workspace-recovery.py with HTTPS Git fixture"]
    async fn workspace_https_cancelled_checkout_read_retains_cleanup_ownership() {
        cancelled_read_retains_operation_until_cleanup(false).await;
    }

    #[tokio::test]
    #[ignore = "requires workspace-recovery.py with HTTPS Git fixture"]
    async fn workspace_https_cancelled_draft_read_retains_cleanup_ownership() {
        cancelled_read_retains_operation_until_cleanup(true).await;
    }
}
