//! Internal compiled artifact intake. No type here is a public response projection.
use std::{
    collections::HashSet,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Component, Path},
    sync::OnceLock,
};

use aster_core::{odcs::OdcsDocument, CoreError, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const SCHEMA_SHA256: &str = "edb41f33ec46e84780e99872ab2bd67f074959d2bf3e9c9fc54e61f8982b0d93";
pub const SCHEMA: &[u8] = include_bytes!("../schemas/odcs-v3.2.0.json");
pub const MAX_CONFIG_BYTES: usize = 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_BUNDLE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_DOCUMENTS: usize = 128;

pub(crate) fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>> {
    // O_NONBLOCK avoids waiting for a FIFO writer; inspect the opened inode,
    // not pre-open path metadata which can race replacement.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| invalid("compiled input unavailable"))?;
    let metadata = file
        .metadata()
        .map_err(|_| invalid("compiled input metadata unavailable"))?;
    if !metadata.is_file() {
        return Err(invalid("compiled input must be a regular file"));
    }
    if metadata.len() > limit as u64 {
        return Err(invalid("compiled input size limit exceeded"));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("compiled input unreadable"))?;
    if bytes.len() > limit {
        return Err(invalid("compiled input size limit exceeded"));
    }
    Ok(bytes)
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub path: String,
    pub sha256: String,
    pub id: String,
    pub version: String,
    pub target: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    manifest_version: u32,
    bundle: String,
    version: String,
    documents: Vec<Selection>,
    bindings_sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhysicalBinding {
    pub path: String,
    /// Exact JSON pointer to a schema object; no invented optional ODCS IDs.
    pub object: String,
    pub catalog: String,
    pub namespace_segments: Vec<String>,
    pub physical_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhysicalBindings {
    pub format_version: u32,
    pub version: String,
    pub bindings: Vec<PhysicalBinding>,
}

pub struct CompiledArtifact {
    pub selection: Selection,
    pub document: OdcsDocument,
}

pub struct CompiledBundle {
    pub(crate) root: std::path::PathBuf,
    pub bundle: String,
    pub version: String,
    pub manifest_sha256: String,
    pub documents: Vec<CompiledArtifact>,
    pub physical_bindings: Option<PhysicalBindings>,
}

pub fn from_env() -> Result<Option<CompiledBundle>> {
    let root = std::env::var("ASTER_CONTRACT_BUNDLE").ok();
    let digest = std::env::var("ASTER_CONTRACT_MANIFEST_SHA256").ok();
    let version = std::env::var("ASTER_CONTRACT_BUNDLE_VERSION").ok();
    match (root, digest, version) {
        (None, None, None) => Ok(None),
        (Some(root), Some(digest), Some(version)) if !root.is_empty() && !version.is_empty() => {
            load_bundle(Path::new(&root), &digest, &version).map(Some)
        }
        _ => Err(invalid(
            "compiled bundle requires directory, manifest SHA256 and version",
        )),
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::Invalid(message.into())
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn pin(bytes: &[u8], expected: &str) -> Result<()> {
    if sha256(bytes) != expected {
        return Err(invalid("compiled bundle digest mismatch"));
    }
    Ok(())
}

pub fn validate_schema(raw: &Value) -> Result<()> {
    static VALIDATOR: OnceLock<std::result::Result<jsonschema::Validator, String>> =
        OnceLock::new();
    let validator = VALIDATOR
        .get_or_init(|| {
            pin(SCHEMA, SCHEMA_SHA256).map_err(|e| e.to_string())?;
            let schema: Value = serde_json::from_slice(SCHEMA).map_err(|e| e.to_string())?;
            jsonschema::draft201909::options()
                .build(&schema)
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|_| invalid("offline ODCS validator unavailable"))?;
    validator
        .validate(raw)
        .map_err(|_| invalid("ODCS schema validation failed"))
}

/// Synchronous filesystem and validation work: startup calls this in spawn_blocking.
pub fn load_bundle(
    root: &Path,
    manifest_sha256: &str,
    expected_version: &str,
) -> Result<CompiledBundle> {
    let bytes = read_regular(&root.join("manifest.json"), MAX_CONFIG_BYTES)?;
    let mut aggregate = bytes.len();
    pin(&bytes, manifest_sha256)?;
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| invalid("invalid compiled manifest"))?;
    if manifest.manifest_version != 1
        || manifest.bundle.is_empty()
        || manifest.version != expected_version
        || manifest.documents.is_empty()
        || manifest.documents.len() > MAX_DOCUMENTS
    {
        return Err(invalid("compiled manifest version or selection mismatch"));
    }
    let root = root
        .canonicalize()
        .map_err(|_| invalid("compiled bundle unavailable"))?;
    let mut paths = HashSet::new();
    let mut identities = HashSet::new();
    let mut documents = Vec::new();
    for selection in manifest.documents {
        let path = Path::new(&selection.path);
        if !selection.path.starts_with("compiled/")
            || !path.components().all(|c| matches!(c, Component::Normal(_)))
            || !paths.insert(selection.path.clone())
            || selection.target.is_empty()
            || !identities.insert((
                selection.id.clone(),
                selection.version.clone(),
                selection.target.clone(),
            ))
        {
            return Err(invalid("missing or ambiguous compiled selection"));
        }
        let resolved = root
            .join(path)
            .canonicalize()
            .map_err(|_| invalid("selected compiled document unavailable"))?;
        if !resolved.starts_with(root.join("compiled")) {
            return Err(invalid("compiled selection escapes bundle"));
        }
        let source = read_regular(
            &resolved,
            MAX_DOCUMENT_BYTES.min(MAX_BUNDLE_BYTES - aggregate),
        )?;
        aggregate += source.len();
        pin(&source, &selection.sha256)?;
        let document = OdcsDocument::parse(&source)?;
        validate_schema(&document.raw)?;
        if document.projection.id != selection.id
            || document.projection.version != selection.version
        {
            return Err(invalid("compiled document identity mismatch"));
        }
        documents.push(CompiledArtifact {
            selection,
            document,
        });
    }
    let physical_bindings = match (
        manifest.bindings_sha256,
        root.join("bindings.json").exists(),
    ) {
        (None, false) => None,
        (Some(digest), true) => {
            let bytes = read_regular(
                &root.join("bindings.json"),
                MAX_CONFIG_BYTES.min(MAX_BUNDLE_BYTES - aggregate),
            )?;
            pin(&bytes, &digest)?;
            let config: PhysicalBindings = serde_json::from_slice(&bytes)
                .map_err(|_| invalid("invalid physical binding config"))?;
            if config.format_version != 1 || config.version.is_empty() {
                return Err(invalid("unsupported physical binding config"));
            }
            let mut seen = HashSet::new();
            for binding in &config.bindings {
                let document = documents
                    .iter()
                    .find(|d| d.selection.path == binding.path)
                    .ok_or_else(|| invalid("binding references unselected artifact"))?;
                let index = binding
                    .object
                    .strip_prefix("/schema/")
                    .and_then(|v| v.parse::<usize>().ok())
                    .ok_or_else(|| invalid("binding requires exact schema object pointer"))?;
                if binding.object != format!("/schema/{index}")
                    || index >= document.document.projection.schema.len()
                    || binding.catalog.is_empty()
                    || binding.physical_name.is_empty()
                    || !seen.insert((&binding.path, &binding.object))
                {
                    return Err(invalid("missing or ambiguous physical binding identity"));
                }
                aster_core::catalog::namespace_segments("", &binding.namespace_segments)?;
            }
            Some(config)
        }
        _ => {
            return Err(invalid(
                "physical bindings require an exact manifest digest",
            ))
        }
    };
    Ok(CompiledBundle {
        root,
        bundle: manifest.bundle,
        version: manifest.version,
        manifest_sha256: manifest_sha256.into(),
        documents,
        physical_bindings,
    })
}
