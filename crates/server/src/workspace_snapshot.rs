//! Recovery archives contain data only; executable Git configuration is rebuilt.

use aster_core::{CoreError, Result};
use std::path::Path;

pub(crate) const MAX_FILES: usize = 10_000;
pub(crate) const MAX_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_ARCHIVE_BYTES: usize = 96 * 1024 * 1024;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    version: u8,
    scope: String,
    object_format: String,
    files: std::collections::BTreeMap<String, Entry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    content: String,
    executable: bool,
}

fn invalid(message: &str) -> CoreError {
    CoreError::Invalid(message.into())
}
fn io(error: std::io::Error) -> CoreError {
    CoreError::Storage(error.to_string())
}

fn excluded(path: &str) -> bool {
    matches!(
        path,
        ".git/config"
            | ".git/config.worktree"
            | ".git/commondir"
            | ".git/gitdir"
            | ".git/objects/info/alternates"
            | ".git/objects/info/http-alternates"
            | ".git/aster.checkout.lock"
    ) || path.starts_with(".git/hooks/")
        || (path.starts_with(".git/") && path.ends_with(".lock"))
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn object_format(root: &Path) -> Result<String> {
    let config = root.join(".git/config");
    match std::fs::symlink_metadata(&config) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok("sha1".into()),
        Ok(meta) if meta.is_file() && meta.len() <= 65536 => {}
        _ => return Err(invalid("unsupported Git configuration")),
    }
    let mut command = std::process::Command::new("git");
    command
        .args(["config", "--no-includes", "--file"])
        .arg(config)
        .args(["--get", "extensions.objectFormat"]);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    let output = command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map_err(io)?;
    match (output.status.code(), output.stdout.as_slice()) {
        (Some(1), _) => Ok("sha1".into()),
        (Some(0), b"sha1\n") => Ok("sha1".into()),
        (Some(0), b"sha256\n") => Ok("sha256".into()),
        _ => Err(invalid("unsupported Git object format")),
    }
}

pub(crate) fn capture(root: &Path, scope: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    use std::os::unix::fs::PermissionsExt;
    if !std::fs::symlink_metadata(root).map_err(io)?.is_dir() {
        return Err(invalid("workspace must be a directory"));
    }
    if !std::fs::symlink_metadata(root.join(".git"))
        .map_err(io)?
        .is_dir()
    {
        return Err(invalid("workspace requires an in-root Git directory"));
    }
    for dependency in [
        ".git/commondir",
        ".git/gitdir",
        ".git/objects/info/alternates",
        ".git/objects/info/http-alternates",
    ] {
        match std::fs::symlink_metadata(root.join(dependency)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(invalid("workspace uses external Git metadata or objects")),
        }
    }
    let object_format = object_format(root)?;
    let mut files = std::collections::BTreeMap::new();
    let mut directories = vec![root.to_path_buf()];
    let mut bytes = 0_usize;
    let mut count = 0_usize;
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).map_err(io)? {
            let entry = entry.map_err(io)?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| invalid("workspace path escaped"))?
                .to_str()
                .ok_or_else(|| invalid("workspace paths must be UTF-8"))?
                .to_owned();
            if !valid_path(&relative) {
                return Err(invalid("unsupported workspace path"));
            }
            let meta = std::fs::symlink_metadata(&path).map_err(io)?;
            count += 1;
            if count > MAX_FILES {
                return Err(invalid("workspace file quota exceeded"));
            }
            if !meta.is_dir() && !meta.is_file() {
                return Err(invalid(
                    "workspace links and special files cannot be checkpointed",
                ));
            }
            if excluded(&relative) || relative == ".git/hooks" {
                continue;
            }
            if meta.is_dir() {
                directories.push(path);
                continue;
            }
            let length = usize::try_from(meta.len())
                .map_err(|_| invalid("workspace byte quota exceeded"))?;
            bytes = bytes
                .checked_add(length)
                .ok_or_else(|| invalid("workspace byte quota exceeded"))?;
            if bytes > MAX_BYTES {
                return Err(invalid("workspace byte quota exceeded"));
            }
            let content = std::fs::read(&path).map_err(io)?;
            if content.len() != length {
                return Err(CoreError::Conflict(
                    "workspace changed during checkpoint".into(),
                ));
            }
            files.insert(
                relative,
                Entry {
                    content: base64::engine::general_purpose::STANDARD.encode(content),
                    executable: meta.permissions().mode() & 0o111 != 0,
                },
            );
        }
    }
    let bytes = serde_json::to_vec(&Archive {
        version: 1,
        scope: scope.into(),
        object_format,
        files,
    })
    .map_err(|e| CoreError::Storage(e.to_string()))?;
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(invalid("workspace archive quota exceeded"));
    }
    Ok(bytes)
}

pub(crate) fn restore(root: &Path, scope: &str, bytes: &[u8]) -> Result<()> {
    use base64::Engine;
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(invalid("workspace archive quota exceeded"));
    }
    let archive: Archive =
        serde_json::from_slice(bytes).map_err(|_| invalid("invalid workspace archive"))?;
    if archive.version != 1
        || archive.scope != scope
        || archive.files.len() > MAX_FILES
        || !matches!(archive.object_format.as_str(), "sha1" | "sha256")
    {
        return Err(invalid("workspace archive identity or version mismatch"));
    }
    let safe_config = if archive.object_format == "sha256" {
        "[core]\nrepositoryformatversion = 1\nbare = false\n[extensions]\nobjectFormat = sha256\n"
    } else {
        "[core]\nrepositoryformatversion = 0\nbare = false\n"
    };
    let mut decoded = vec![(
        ".git/config".to_owned(),
        false,
        safe_config.as_bytes().to_vec(),
    )];
    let mut total = 0_usize;
    for (path, entry) in archive.files {
        if path == ".git" || !valid_path(&path) || excluded(&path) {
            return Err(invalid("unsafe archive path"));
        }
        let content = base64::engine::general_purpose::STANDARD
            .decode(entry.content)
            .map_err(|_| invalid("invalid archive content"))?;
        total = total
            .checked_add(content.len())
            .ok_or_else(|| invalid("workspace byte quota exceeded"))?;
        if total > MAX_BYTES {
            return Err(invalid("workspace byte quota exceeded"));
        }
        decoded.push((path, entry.executable, content));
    }
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(root)
        .map_err(io)?;
    let result = (|| {
        for (path, executable, content) in decoded {
            let destination = root.join(path);
            let parent = destination
                .parent()
                .ok_or_else(|| invalid("invalid archive parent"))?;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(io)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(if executable { 0o700 } else { 0o600 })
                .open(destination)
                .map_err(io)?;
            file.write_all(&content).map_err(io)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(root);
    }
    result
}

/// Only called on a validated, privately materialized recovery generation.
pub(crate) fn portable_export(root: &Path) -> Result<Vec<u8>> {
    let output = std::process::Command::new("tar")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .args(["--format=posix", "--create", "--file=-", "--directory"])
        .arg(root)
        .args(["--", "."])
        .output()
        .map_err(io)?;
    if !output.status.success() || output.stdout.len() > MAX_ARCHIVE_BYTES {
        return Err(CoreError::Storage(
            "workspace export failed or exceeded its limit".into(),
        ));
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_preserves_uncommitted_files_and_omits_executable_git_config() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        std::fs::write(root.path().join("draft.aster"), b"uncommitted notebook").unwrap();
        std::fs::write(root.path().join("untracked.txt"), b"untracked content").unwrap();
        std::fs::write(root.path().join("Cargo.lock"), b"dependency lock").unwrap();
        std::fs::write(root.path().join(".git/HEAD"), b"ref: refs/heads/alice\n").unwrap();
        std::fs::write(
            root.path().join(".git/config"),
            b"[credential]\nhelper = unsafe-helper\n",
        )
        .unwrap();
        let archive = capture(root.path(), "workspace/checkout").unwrap();
        let value: serde_json::Value = serde_json::from_slice(&archive).unwrap();
        assert_eq!(value["scope"], "workspace/checkout");
        let files = value["files"].as_object().unwrap();
        assert!(files.contains_key("draft.aster"));
        assert!(files.contains_key("untracked.txt"));
        assert!(files.contains_key("Cargo.lock"));
        assert!(files.contains_key(".git/HEAD"));
        assert!(!files.contains_key(".git/config"));
        let destination = root.path().join("restored");
        restore(&destination, "workspace/checkout", &archive).unwrap();
        assert_eq!(
            std::fs::read(destination.join("draft.aster")).unwrap(),
            b"uncommitted notebook"
        );
        assert_eq!(
            std::fs::read(destination.join("untracked.txt")).unwrap(),
            b"untracked content"
        );
        assert!(!std::fs::read_to_string(destination.join(".git/config"))
            .unwrap()
            .contains("unsafe-helper"));
        let mut hostile = value;
        hostile["files"]["../outside"] = serde_json::json!({"content":"eA==","executable":false});
        let target = root.path().join("hostile");
        assert!(restore(
            &target,
            "workspace/checkout",
            &serde_json::to_vec(&hostile).unwrap()
        )
        .is_err());
        assert!(!target.exists());
        assert!(restore(&target, "another-workspace", &archive).is_err());
    }

    #[test]
    fn recovery_refuses_external_git_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".git"), "gitdir: /outside/repository").unwrap();
        assert!(
            capture(root.path(), "scope").is_err(),
            "external Git metadata must not be archived"
        );
        let archive = serde_json::json!({"version":1,"scope":"scope","object_format":"sha1","files":{
            ".git":{"content":"Z2l0ZGlyOiAvb3V0c2lkZS9yZXBvc2l0b3J5","executable":false}}});
        assert!(restore(
            &root.path().join("restored"),
            "scope",
            &serde_json::to_vec(&archive).unwrap()
        )
        .is_err());
    }

    #[test]
    fn checkpoint_refuses_external_object_dependencies() {
        for dependency in [
            ".git/commondir",
            ".git/gitdir",
            ".git/objects/info/alternates",
            ".git/objects/info/http-alternates",
        ] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join(dependency);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "/external/git/objects").unwrap();
            assert!(
                capture(root.path(), "scope").is_err(),
                "external dependency {dependency} cannot be silently dropped"
            );
        }
    }

    #[test]
    fn recovery_preserves_git_history_index_deletions_and_drafts() {
        fn git(root: &Path, args: &[&str]) -> Vec<u8> {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args([
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "-c",
                    "core.hooksPath=/dev/null",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            output.stdout
        }
        for format in ["sha1", "sha256"] {
            let root = tempfile::tempdir().unwrap();
            git(
                root.path(),
                &["init", "-b", "alice/analysis", "--object-format", format],
            );
            std::fs::write(root.path().join("draft.aster"), "committed").unwrap();
            std::fs::write(root.path().join("deleted.txt"), "deleted later").unwrap();
            git(root.path(), &["add", "."]);
            git(root.path(), &["commit", "-m", "unpublished local commit"]);
            std::fs::write(root.path().join("draft.aster"), "staged").unwrap();
            git(root.path(), &["add", "draft.aster"]);
            std::fs::write(root.path().join("draft.aster"), "unstaged").unwrap();
            std::fs::remove_file(root.path().join("deleted.txt")).unwrap();
            std::fs::write(root.path().join("untracked"), "new").unwrap();
            let archive = capture(root.path(), "scope").unwrap();
            let target_parent = tempfile::tempdir().unwrap();
            let target = target_parent.path().join("checkout");
            restore(&target, "scope", &archive).unwrap();
            for args in [
                &["rev-parse", "HEAD"][..],
                &["symbolic-ref", "HEAD"],
                &["show", ":draft.aster"],
                &["status", "--porcelain"],
            ] {
                assert_eq!(git(&target, args), git(root.path(), args));
            }
            assert_eq!(
                std::fs::read(target.join("draft.aster")).unwrap(),
                b"unstaged"
            );
            assert!(!target.join("deleted.txt").exists());
            assert_eq!(std::fs::read(target.join("untracked")).unwrap(), b"new");
        }
    }

    #[test]
    fn checkpoint_refuses_symlink_escape_instead_of_losing_files() {
        let root = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/etc/passwd", root.path().join("escape")).unwrap();
        assert!(capture(root.path(), "workspace/checkout").is_err());
    }
}
