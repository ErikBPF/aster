//! Recovery data is separate from Git history and publication.

use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

#[async_trait]
pub trait WorkspaceRecovery: Send + Sync {
    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()>;
    async fn get(&self, key: &str, max_bytes: usize) -> Result<Vec<u8>>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
}

#[derive(Clone, Debug)]
pub struct WorkspaceDraft {
    pub notebook: crate::Notebook,
    pub revision: String,
}

#[derive(Clone, Debug)]
pub struct WorkspaceGitStatus {
    pub branch: String,
    pub head: String,
    pub dirty: bool,
}

/// Draft persistence and explicit Git commits for an isolated checkout.
#[async_trait]
pub trait WorkspaceCheckout: Send + Sync {
    async fn draft(&self, id: &str) -> Result<WorkspaceDraft>;
    async fn save_draft(
        &self,
        notebook: &crate::Notebook,
        expected: Option<String>,
    ) -> Result<WorkspaceDraft>;
    async fn commit_drafts(&self, ids: Vec<String>, message: String) -> Result<String>;
    async fn draft_ids(&self) -> Result<Vec<String>>;
    async fn git_status(&self) -> Result<WorkspaceGitStatus>;
    async fn switch_branch(&self, branch: &str, create: bool)
        -> Result<Box<dyn WorkspaceCheckout>>;
    async fn pull(&self, credential: WorkspaceGitCredential) -> Result<String>;
    async fn publish(
        &self,
        credential: WorkspaceGitCredential,
        journal: PublicationJournal,
    ) -> Result<String>;
    async fn published(
        &self,
        credential: WorkspaceGitCredential,
        acknowledge: bool,
    ) -> Result<String>;
}

/// Short-lived transport material. Never serialize or log this value.
pub struct WorkspaceGitCredential {
    pub remote_url: String,
    pub token: String,
    pub ca_bundle: Option<std::path::PathBuf>,
}

#[async_trait]
pub trait WorkspaceHosting: Send + Sync {
    async fn credential(
        &self,
        repository: &str,
        installation_id: u64,
        repository_id: u64,
    ) -> Result<WorkspaceGitCredential>;
    async fn find_pull_request(
        &self,
        repository: &str,
        installation_id: u64,
        repository_id: u64,
        branch: &str,
        base: &str,
    ) -> Result<Option<String>>;
    async fn open_pull_request(
        &self,
        repository: &str,
        installation_id: u64,
        repository_id: u64,
        branch: &str,
        base: &str,
        title: &str,
    ) -> std::result::Result<String, WorkspacePullRequestFailure>;
}

pub type PublicationJournal = Box<dyn FnOnce(&str, Option<&str>) -> Result<()> + Send>;

/// Materialize a checkout using the selected Git provider.
#[async_trait]
pub trait WorkspaceCheckouts: Send + Sync {
    async fn clone_repository(
        &self,
        root: std::path::PathBuf,
        binding: NotebookWorkspaceBinding,
        credential: WorkspaceGitCredential,
    ) -> Result<Box<dyn WorkspaceCheckout>>;
    async fn open(
        &self,
        root: std::path::PathBuf,
        branch: String,
        binding: NotebookWorkspaceBinding,
    ) -> Result<Box<dyn WorkspaceCheckout>>;
}

/// Whether a failed PR operation could have created a remote request.
#[derive(Debug)]
pub struct WorkspacePullRequestFailure {
    pub error: crate::CoreError,
    pub may_have_written: bool,
}
