//! Git-backed Markdown documents and images for catalog entities.
//!
//! The store is deliberately small and local: one checkout, one writer lock,
//! local commits only. It never pushes and never serves anything but Markdown
//! documents and image assets.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use aster_core::{CoreError, Result};
use serde::Serialize;

const MAX_DOC_BYTES: usize = 256 * 1024;
const MAX_ASSET_BYTES: u64 = 5 * 1024 * 1024;
const MAX_HISTORY: usize = 30;
const ASSET_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "svg", "webp", "gif"];

#[derive(Clone, Debug)]
pub enum Entity {
    Catalog {
        catalog: String,
    },
    Schema {
        catalog: String,
        namespace: Vec<String>,
    },
    Table {
        catalog: String,
        namespace: Vec<String>,
        table: String,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct Document {
    pub markdown: String,
    /// Blob OID of the working file; absent before the first save.
    pub revision: Option<String>,
    /// Commit that last changed this document.
    pub commit: Option<String>,
    pub path: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Commit {
    pub oid: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Asset {
    pub name: String,
    pub bytes: u64,
}

pub struct CatalogAssets {
    root: PathBuf,
    lock: Mutex<()>,
}

impl CatalogAssets {
    pub fn from_env() -> Result<Option<Self>> {
        let Some(root) = std::env::var("ASTER_CATALOG_ASSETS_DIR").ok() else {
            return Ok(None);
        };
        if root.trim().is_empty() {
            return Ok(None);
        }
        Self::open(Path::new(&root)).map(Some)
    }

    pub fn open(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .map_err(|_| CoreError::Invalid("catalog asset directory unavailable".into()))?;
        if !root.join(".git").exists() {
            return Err(CoreError::Invalid(
                "catalog asset directory is not a git checkout".into(),
            ));
        }
        Ok(Self {
            root,
            lock: Mutex::new(()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn document(&self, entity: &Entity) -> Result<Option<Document>> {
        let path = doc_path(entity)?;
        self.read_current(&path)
    }

    pub fn save(
        &self,
        entity: &Entity,
        markdown: &str,
        expected: Option<&str>,
        subject: &str,
    ) -> Result<Document> {
        if markdown.len() > MAX_DOC_BYTES {
            return Err(CoreError::Invalid("catalog document is too large".into()));
        }
        let path = doc_path(entity)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| CoreError::Storage("catalog asset lock poisoned".into()))?;
        let current = self.revision(&path)?;
        if current.as_deref() != expected {
            return Err(CoreError::Conflict(
                "catalog document changed; reload before saving".into(),
            ));
        }
        if current.is_some()
            && self
                .read_current(&path)?
                .is_some_and(|doc| doc.markdown == markdown)
        {
            return self
                .read_current(&path)?
                .ok_or_else(|| CoreError::NotFound("catalog document".into()));
        }
        let target = self.root.join(&path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CoreError::Storage(format!("catalog document directory: {error}"))
            })?;
        }
        if std::fs::symlink_metadata(&target)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(CoreError::Invalid("catalog document is a symlink".into()));
        }
        self.confined(&target)?;
        let previous = std::fs::read(&target).ok();
        std::fs::write(&target, markdown)
            .map_err(|error| CoreError::Storage(format!("catalog document write: {error}")))?;
        let author = git_author(subject);
        let message = format!("catalog: update {} by {}", label(entity), author);
        if let Err(error) = self.git(&["add", "--", &path]) {
            restore(&target, previous.as_deref());
            return Err(error);
        }
        if let Err(error) =
            self.git_with_identity(&["commit", "-q", "-m", &message, "--", &path], &author)
        {
            restore(&target, previous.as_deref());
            return Err(error);
        }
        Ok(Document {
            markdown: markdown.to_string(),
            revision: self.revision(&path)?,
            commit: Some(self.git(&["rev-parse", "HEAD"])?.trim().to_string()),
            path,
        })
    }

    pub fn history(&self, entity: &Entity) -> Result<Vec<Commit>> {
        let path = doc_path(entity)?;
        if self.git(&["rev-parse", "--verify", "HEAD"]).is_err() {
            return Ok(Vec::new());
        }
        let output = self.git(&[
            "log",
            &format!("-n{MAX_HISTORY}"),
            "--format=%H%x1f%an%x1f%aI%x1f%s",
            "--",
            &path,
        ])?;
        Ok(output
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('\u{1f}');
                Some(Commit {
                    oid: parts.next()?.to_string(),
                    author: parts.next()?.to_string(),
                    date: parts.next()?.to_string(),
                    message: parts.next()?.to_string(),
                })
            })
            .collect())
    }

    pub fn revision_document(&self, entity: &Entity, oid: &str) -> Result<Document> {
        let path = doc_path(entity)?;
        if !valid_oid(oid) {
            return Err(CoreError::Invalid("invalid catalog revision".into()));
        }
        let markdown = self.git(&["show", &format!("{oid}:{path}")])?;
        if markdown.len() > MAX_DOC_BYTES {
            return Err(CoreError::Invalid("catalog document is too large".into()));
        }
        Ok(Document {
            markdown,
            revision: Some(oid.to_string()),
            commit: Some(oid.to_string()),
            path,
        })
    }

    pub fn assets(&self, entity: &Entity) -> Result<Vec<Asset>> {
        let directory = self.root.join(assets_path(entity)?);
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(CoreError::Storage(format!(
                    "catalog asset directory: {error}"
                )))
            }
        };
        let mut assets = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !valid_asset_name(&name) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !file_type.is_file() || file_type.is_symlink() || !metadata.is_file() {
                continue;
            }
            assets.push(Asset {
                name,
                bytes: metadata.len(),
            });
        }
        assets.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(assets)
    }

    pub fn read_asset(&self, entity: &Entity, name: &str) -> Result<(Vec<u8>, &'static str)> {
        if !valid_asset_name(name) {
            return Err(CoreError::Invalid("unsupported catalog asset".into()));
        }
        let path = self.root.join(assets_path(entity)?).join(name);
        self.confined(&path)?;
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| CoreError::NotFound("catalog asset".into()))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(CoreError::Invalid("unsupported catalog asset".into()));
        }
        if metadata.len() > MAX_ASSET_BYTES {
            return Err(CoreError::Invalid("catalog asset is too large".into()));
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| CoreError::Storage(format!("catalog asset read: {error}")))?;
        Ok((bytes, content_type(name)))
    }

    fn confined(&self, target: &Path) -> Result<()> {
        let parent = target.parent().unwrap_or(&self.root);
        let parent = parent
            .canonicalize()
            .map_err(|_| CoreError::Invalid("catalog asset path is unavailable".into()))?;
        if !parent.starts_with(&self.root) {
            return Err(CoreError::Invalid(
                "catalog asset path escapes the checkout".into(),
            ));
        }
        Ok(())
    }

    fn read_current(&self, path: &str) -> Result<Option<Document>> {
        let target = self.root.join(path);
        if target.exists() {
            self.confined(&target)?;
        }
        let metadata = match std::fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(CoreError::Storage(format!(
                    "catalog document metadata: {error}"
                )))
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(CoreError::Invalid(
                "catalog document is not a regular file".into(),
            ));
        }
        if metadata.len() > MAX_DOC_BYTES as u64 {
            return Err(CoreError::Invalid("catalog document is too large".into()));
        }
        let markdown = std::fs::read_to_string(&target)
            .map_err(|error| CoreError::Storage(format!("catalog document read: {error}")))?;
        Ok(Some(Document {
            markdown,
            revision: self.revision(path)?,
            commit: None,
            path: path.to_string(),
        }))
    }

    fn revision(&self, path: &str) -> Result<Option<String>> {
        if !self.root.join(path).exists() {
            return Ok(None);
        }
        Ok(Some(
            self.git(&["hash-object", "--", path])?.trim().to_string(),
        ))
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(args)
            .output()
            .map_err(|error| CoreError::Storage(format!("git unavailable: {error}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Storage(format!("git failed: {stderr}")));
        }
        String::from_utf8(output.stdout)
            .map_err(|_| CoreError::Storage("git output was not utf-8".into()))
    }

    fn git_with_identity(&self, args: &[&str], author: &str) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.root)
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(["-c", &format!("user.name={author}")])
            .args(["-c", &format!("user.email={author}@aster.local")])
            .args(args)
            .output()
            .map_err(|error| CoreError::Storage(format!("git unavailable: {error}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CoreError::Storage(format!("git commit failed: {stderr}")));
        }
        String::from_utf8(output.stdout)
            .map_err(|_| CoreError::Storage("git output was not utf-8".into()))
    }
}

fn restore(target: &Path, previous: Option<&[u8]>) {
    match previous {
        Some(bytes) => {
            let _ = std::fs::write(target, bytes);
        }
        None => {
            let _ = std::fs::remove_file(target);
        }
    }
}

fn doc_path(entity: &Entity) -> Result<String> {
    let catalog = component(catalog_id(entity))?;
    match entity {
        Entity::Catalog { .. } => Ok(format!("catalogs/{catalog}/README.md")),
        Entity::Schema { namespace, .. } => {
            let namespace = namespace_path(namespace)?;
            Ok(format!("catalogs/{catalog}/schemas/{namespace}/README.md"))
        }
        Entity::Table {
            namespace, table, ..
        } => {
            let namespace = namespace_path(namespace)?;
            let table = component(table)?;
            Ok(format!(
                "catalogs/{catalog}/schemas/{namespace}/tables/{table}.md"
            ))
        }
    }
}

fn assets_path(entity: &Entity) -> Result<String> {
    let doc = doc_path(entity)?;
    let parent = doc.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("");
    Ok(format!("{parent}/assets"))
}

fn catalog_id(entity: &Entity) -> &str {
    match entity {
        Entity::Catalog { catalog }
        | Entity::Schema { catalog, .. }
        | Entity::Table { catalog, .. } => catalog,
    }
}

fn label(entity: &Entity) -> String {
    match entity {
        Entity::Catalog { catalog } => format!("catalog {catalog}"),
        Entity::Schema { catalog, namespace } => {
            format!("schema {catalog}.{}", namespace.join("."))
        }
        Entity::Table {
            catalog,
            namespace,
            table,
        } => format!("table {catalog}.{}.{table}", namespace.join(".")),
    }
}

fn namespace_path(namespace: &[String]) -> Result<String> {
    if namespace.is_empty() || namespace.len() > 32 {
        return Err(CoreError::Invalid("invalid catalog namespace".into()));
    }
    namespace
        .iter()
        .map(|segment| component(segment))
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

fn component(value: &str) -> Result<String> {
    if value.is_empty()
        || value.len() > 128
        || value == "."
        || value == ".."
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
    {
        return Err(CoreError::Invalid("invalid catalog asset path".into()));
    }
    Ok(value.to_string())
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_asset_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 128
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
    {
        return false;
    }
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    extension.is_some_and(|extension| {
        ASSET_EXTENSIONS
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(extension))
    })
}

fn content_type(name: &str) -> &'static str {
    match name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

fn git_author(subject: &str) -> String {
    let author = subject
        .chars()
        .map(|character| {
            if character.is_control() || character == '<' || character == '>' {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    if author.trim().is_empty() {
        "aster".into()
    } else {
        author
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn repo() -> (PathBuf, CatalogAssets) {
        let dir = std::env::temp_dir().join(format!(
            "aster-catalog-assets-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let status = Command::new("git")
            .current_dir(&dir)
            .args(["init", "-q"])
            .status()
            .unwrap();
        assert!(status.success());
        let store = CatalogAssets::open(&dir).unwrap();
        (dir, store)
    }

    fn table() -> Entity {
        Entity::Table {
            catalog: "polaris".into(),
            namespace: vec!["sales".into()],
            table: "orders".into(),
        }
    }

    #[test]
    fn documents_commit_and_require_the_expected_blob() {
        let (dir, store) = repo();
        let entity = table();
        let first = store
            .save(&entity, "# Orders\n", None, "alice")
            .expect("save");
        assert!(first.revision.is_some());
        assert_eq!(
            first.path,
            "catalogs/polaris/schemas/sales/tables/orders.md"
        );
        let history = store.history(&entity).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].author, "alice");
        assert!(store.save(&entity, "# changed\n", None, "bob").is_err());
        let second = store
            .save(&entity, "# Orders v2\n", first.revision.as_deref(), "alice")
            .unwrap();
        assert_ne!(first.revision, second.revision);
        assert_eq!(
            store
                .revision_document(&entity, &history[0].oid)
                .unwrap()
                .markdown,
            "# Orders\n"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn paths_and_assets_are_bounded() {
        let (dir, store) = repo();
        let entity = table();
        assert!(store
            .save(
                &Entity::Table {
                    catalog: "polaris".into(),
                    namespace: vec!["../escape".into()],
                    table: "orders".into(),
                },
                "# x",
                None,
                "alice"
            )
            .is_err());
        let assets = dir.join("catalogs/polaris/schemas/sales/tables/assets");
        std::fs::create_dir_all(&assets).unwrap();
        std::fs::write(assets.join("diagram.svg"), "<svg/>").unwrap();
        std::fs::write(assets.join("notes.txt"), "no").unwrap();
        let listed = store.assets(&entity).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "diagram.svg");
        assert_eq!(
            store.read_asset(&entity, "diagram.svg").unwrap().1,
            "image/svg+xml"
        );
        assert!(store.read_asset(&entity, "notes.txt").is_err());
        assert!(store.read_asset(&entity, "../diagram.svg").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
