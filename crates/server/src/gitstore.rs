use std::path::PathBuf;
use std::process::Command;

use aster_core::{CoreError, Notebook, NotebookStore, Result};
use async_trait::async_trait;

/// Git-backed notebook store. Branch-per-session (D4): each session works on
/// its own branch, so commits never contend on a shared branch.
///
/// ponytail: shells out to the `git` binary instead of linking git2/libgit2 —
/// no C toolchain in the image and remote push is not needed yet. Swap to git2
/// (or add a token-authenticated push) when per-user repos land in S1/D9.
pub struct GitNotebookStore {
    dir: PathBuf,
    branch: String,
}

impl GitNotebookStore {
    pub fn open(dir: impl Into<PathBuf>, branch: impl Into<String>) -> Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(storage)?;
        let store = Self {
            dir,
            branch: branch.into(),
        };

        if !store.dir.join(".git").exists() {
            store.git(&["init", "-q"])?;
            store.git(&["config", "user.name", "aster"])?;
            store.git(&["config", "user.email", "aster@localhost"])?;
        }

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
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .output()
            .map_err(storage)?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(CoreError::Storage(format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
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

fn storage(error: std::io::Error) -> CoreError {
    CoreError::Storage(format!("io error: {error}"))
}

#[async_trait]
impl NotebookStore for GitNotebookStore {
    async fn get(&self, id: &str) -> Result<Notebook> {
        let path = self.path_for(id)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|_| CoreError::NotFound(format!("notebook {id}")))?;
        let mut notebook = Notebook::from_text(&text)?;
        notebook.id = id.to_string();
        Ok(notebook)
    }

    async fn save(&self, notebook: &Notebook, actor: &str) -> Result<String> {
        let path = self.path_for(&notebook.id)?;
        std::fs::write(&path, notebook.to_text()).map_err(storage)?;

        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        self.git(&["add", "--", &name])?;
        let message = format!("{actor}: update {}", notebook.id);
        match self.git(&["commit", "-q", "-m", &message]) {
            Ok(_) => {}
            Err(error) if error.to_string().contains("nothing to commit") => {}
            Err(error) => return Err(error),
        }
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_string())
    }

    async fn list(&self, _actor: &str) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&self.dir).map_err(storage)? {
            let name = entry
                .map_err(storage)?
                .file_name()
                .to_string_lossy()
                .to_string();
            if let Some(id) = name.strip_suffix(".aster") {
                ids.push(id.to_string());
            }
        }
        ids.sort();
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_core::Cell;

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
            }],
        }
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
}
