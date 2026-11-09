use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::engine::EngineId;
use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub id: String,
    pub sql: String,
    /// Legacy per-cell engine. New saves use the notebook engine; this remains
    /// readable so older documents keep working.
    pub engine: Option<EngineId>,
    /// Persisted client context, including catalog/schema selection.
    #[serde(default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CellKind {
    Sql,
    Markdown,
}

impl Cell {
    /// The first non-empty line selects the cell kind. `%md` makes a Markdown
    /// cell; `%sql` is accepted for symmetry and any other line is SQL.
    pub fn kind(&self) -> CellKind {
        match self.magic() {
            Some("%md") => CellKind::Markdown,
            _ => CellKind::Sql,
        }
    }

    /// Cell text without the leading magic line, so callers never execute or
    /// render `%md`/`%sql` as content.
    pub fn source(&self) -> &str {
        let mut offset = 0;
        for line in self.sql.split_inclusive('\n') {
            let trimmed = line.trim_end_matches(['\n', '\r']).trim();
            if trimmed.is_empty() {
                offset += line.len();
                continue;
            }
            return match trimmed {
                "%md" | "%sql" => &self.sql[offset + line.len()..],
                _ => &self.sql,
            };
        }
        &self.sql
    }

    fn magic(&self) -> Option<&str> {
        self.sql
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(str::trim)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookContext {
    pub catalog_context: Option<String>,
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notebook {
    pub id: String,
    pub title: String,
    /// One engine for the whole notebook. `None` keeps legacy documents on
    /// their per-cell engines and the configured default.
    #[serde(default)]
    pub engine: Option<EngineId>,
    /// Missing means a legacy document; an explicit empty context clears selection.
    #[serde(default)]
    pub context: Option<NotebookContext>,
    pub cells: Vec<Cell>,
}

impl Notebook {
    /// The notebook engine, falling back to the first legacy per-cell engine.
    pub fn effective_engine(&self) -> Option<&EngineId> {
        self.engine
            .as_ref()
            .or_else(|| self.cells.iter().find_map(|cell| cell.engine.as_ref()))
    }
}

/// The content revision is a committed Git blob OID for one notebook, while
/// the save revision remains the enclosing commit OID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotebookPrecondition {
    Absent,
    Blob(String),
}

#[derive(Debug, Clone)]
pub struct NotebookSnapshot {
    pub notebook: Notebook,
    pub content_revision: String,
}

#[derive(Debug, Clone)]
pub struct NotebookSave {
    pub revision: String,
    pub content_revision: String,
}

/// Git-backed notebook persistence. Listing is read from committed Git state;
/// ownership is a separate metadata concern.
#[async_trait]
pub trait NotebookStore: Send + Sync {
    async fn get(&self, id: &str) -> Result<Notebook>;
    async fn save(&self, notebook: &Notebook, actor: &str) -> Result<String>;
    async fn list(&self, actor: &str) -> Result<Vec<String>>;
    async fn snapshot(&self, _id: &str) -> Result<NotebookSnapshot> {
        Err(CoreError::Storage("notebook snapshot unsupported".into()))
    }
    async fn save_if(
        &self,
        _notebook: &Notebook,
        _actor: &str,
        _expected: NotebookPrecondition,
    ) -> Result<NotebookSave> {
        Err(CoreError::Storage(
            "notebook precondition unsupported".into(),
        ))
    }
    async fn revisions(&self, _id: &str) -> Result<Vec<String>> {
        Err(CoreError::Invalid("notebook history unsupported".into()))
    }
    async fn revision(&self, _id: &str, _revision: &str) -> Result<Notebook> {
        Err(CoreError::Invalid("notebook history unsupported".into()))
    }
    fn source_key(&self) -> String {
        "unconfigured".into()
    }
}

#[derive(Debug, Clone)]
pub struct NotebookOwnerChange {
    pub source: String,
    pub id: String,
    pub expected_owner: Option<String>,
    pub owner: String,
    pub source_blob: String,
    pub actor: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NotebookOwnerRecord {
    pub owner: String,
    pub content_revision: String,
}

/// Durable metadata ownership for the single legacy checkout. Branch authority
/// for future personal workspaces is a separate V2b boundary.
#[async_trait]
pub trait NotebookOwners: Send + Sync {
    async fn record(&self, source: &str, id: &str) -> Result<Option<NotebookOwnerRecord>>;
    async fn change(&self, change: NotebookOwnerChange) -> Result<()>;
    async fn advance(
        &self,
        source: &str,
        id: &str,
        owner: &str,
        before: &str,
        after: &str,
    ) -> Result<()>;
}

#[derive(Default)]
pub struct InMemoryNotebookOwners {
    owners: Mutex<HashMap<(String, String), NotebookOwnerRecord>>,
    events: Mutex<Vec<NotebookOwnerChange>>,
}

impl InMemoryNotebookOwners {
    pub fn events(&self) -> Result<Vec<NotebookOwnerChange>> {
        Ok(self
            .events
            .lock()
            .map_err(|_| CoreError::Storage("notebook owner events poisoned".into()))?
            .clone())
    }
}

#[async_trait]
impl NotebookOwners for InMemoryNotebookOwners {
    async fn record(&self, source: &str, id: &str) -> Result<Option<NotebookOwnerRecord>> {
        Ok(self
            .owners
            .lock()
            .map_err(|_| CoreError::Storage("notebook owners poisoned".into()))?
            .get(&(source.to_string(), id.to_string()))
            .cloned())
    }

    async fn change(&self, change: NotebookOwnerChange) -> Result<()> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| CoreError::Storage("notebook owner events poisoned".into()))?;
        let mut owners = self
            .owners
            .lock()
            .map_err(|_| CoreError::Storage("notebook owners poisoned".into()))?;
        let key = (change.source.clone(), change.id.clone());
        if owners.get(&key).map(|record| record.owner.as_str()) != change.expected_owner.as_deref()
        {
            return Err(CoreError::Conflict("notebook owner changed".into()));
        }
        owners.insert(
            key,
            NotebookOwnerRecord {
                owner: change.owner.clone(),
                content_revision: change.source_blob.clone(),
            },
        );
        events.push(change);
        Ok(())
    }

    async fn advance(
        &self,
        source: &str,
        id: &str,
        owner: &str,
        before: &str,
        after: &str,
    ) -> Result<()> {
        let mut owners = self
            .owners
            .lock()
            .map_err(|_| CoreError::Storage("notebook owners poisoned".into()))?;
        let Some(record) = owners.get_mut(&(source.to_string(), id.to_string())) else {
            return Err(CoreError::Conflict("notebook owner missing".into()));
        };
        if record.owner != owner || record.content_revision != before {
            return Err(CoreError::Conflict(
                "notebook owner or content changed".into(),
            ));
        }
        record.content_revision = after.to_string();
        Ok(())
    }
}

/// Marker for the text-first on-disk format (D8: custom `aster` format). Chosen
/// over `.ipynb` so notebooks diff cleanly in git; the notebook id is the file
/// name, so it is not stored in the body.
const HEADER_V1: &str = "# aster notebook v1";
const HEADER_V3: &str = "# aster notebook v3";
const CONTEXT_PREFIX: &str = "# context: ";
const HEADER_V2: &str = "# aster notebook v2";
const TITLE_PREFIX: &str = "# title: ";
const ENGINE_PREFIX: &str = "# engine: ";
const CELL_PREFIX: &str = "-- cell ";

impl Notebook {
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(if self.context.is_some() {
            HEADER_V3
        } else {
            HEADER_V2
        });
        out.push('\n');
        out.push_str(TITLE_PREFIX);
        out.push_str(&self.title);
        out.push('\n');
        if let Some(engine) = &self.engine {
            out.push_str(ENGINE_PREFIX);
            out.push_str(&engine.0);
            out.push('\n');
        }
        if let Some(context) = &self.context {
            out.push_str(CONTEXT_PREFIX);
            out.push_str(&serde_json::to_string(context).expect("string context serializes"));
            out.push('\n');
        }
        for cell in &self.cells {
            out.push_str(CELL_PREFIX);
            out.push_str(&cell.id);
            if let Some(engine) = &cell.engine {
                out.push_str(" engine=");
                out.push_str(&engine.0);
            }
            if !cell.metadata.is_empty() {
                out.push_str(" metadata=");
                out.push_str(&serde_json::Value::Object(cell.metadata.clone()).to_string());
            }
            out.push('\n');
            for (index, line) in cell.sql.split('\n').enumerate() {
                if index > 0 {
                    out.push('\n');
                }
                if line.starts_with(CELL_PREFIX) || line.starts_with('\\') {
                    out.push('\\');
                }
                out.push_str(line);
            }
            out.push('\n');
        }
        out
    }

    pub fn from_text(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let header = lines
            .next()
            .ok_or_else(|| CoreError::Invalid("empty notebook".into()))?;
        let escaped = match header.trim() {
            HEADER_V1 => false,
            HEADER_V2 | HEADER_V3 => true,
            _ => return Err(CoreError::Invalid("missing aster notebook header".into())),
        };

        let mut title = String::from("untitled");
        let mut engine = None;
        let mut context = None;
        let mut cells: Vec<Cell> = Vec::new();
        let mut current: Option<Cell> = None;
        let mut saw_sql_line = false;

        for line in lines {
            if let Some(rest) = line.strip_prefix(CELL_PREFIX) {
                if let Some(cell) = current.take() {
                    cells.push(cell);
                }
                let (rest, metadata) = match rest.split_once(" metadata=") {
                    Some((rest, metadata)) => (
                        rest,
                        serde_json::from_str(metadata)
                            .map_err(|_| CoreError::Invalid("invalid cell metadata".into()))?,
                    ),
                    None => (rest, serde_json::Map::new()),
                };
                let mut parts = rest.split_whitespace();
                let id = parts
                    .next()
                    .ok_or_else(|| CoreError::Invalid("cell missing id".into()))?
                    .to_string();
                let engine = parts
                    .find_map(|part| part.strip_prefix("engine="))
                    .map(EngineId::new);
                current = Some(Cell {
                    id,
                    sql: String::new(),
                    engine,
                    metadata,
                });
                saw_sql_line = false;
            } else if current.is_none() && line.starts_with(CONTEXT_PREFIX) {
                context = Some(
                    serde_json::from_str(&line[CONTEXT_PREFIX.len()..])
                        .map_err(|_| CoreError::Invalid("invalid notebook context".into()))?,
                );
            } else if current.is_none() && line.starts_with(TITLE_PREFIX) {
                title = line[TITLE_PREFIX.len()..].to_string();
            } else if current.is_none() && line.starts_with(ENGINE_PREFIX) {
                engine = Some(EngineId::new(line[ENGINE_PREFIX.len()..].trim()));
            } else if let Some(cell) = current.as_mut() {
                if saw_sql_line {
                    cell.sql.push('\n');
                }
                cell.sql.push_str(if escaped {
                    line.strip_prefix('\\').unwrap_or(line)
                } else {
                    line
                });
                saw_sql_line = true;
            }
        }
        if let Some(cell) = current {
            cells.push(cell);
        }

        Ok(Notebook {
            context,
            id: String::new(),
            title,
            engine,
            cells,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Notebook {
        Notebook {
            context: None,
            id: "nb1".into(),
            title: "Sales".into(),
            engine: None,
            cells: vec![
                Cell {
                    id: "c1".into(),
                    sql: "SELECT 1".into(),
                    engine: Some(EngineId::new("trino-local")),
                    metadata: Default::default(),
                },
                Cell {
                    id: "c2".into(),
                    sql: "SELECT\n  2".into(),
                    engine: None,
                    metadata: Default::default(),
                },
            ],
        }
    }

    #[test]
    fn notebook_context_survives_json_and_git_without_changing_cells() {
        let document = serde_json::json!({"id":"context", "title":"Context",
            "context":{"catalog_context":"warehouse", "schema":"sales.eu"},
            "cells":[{"id":"c1","sql":"SELECT 1","engine":null,
                "metadata":{"other":false}}]});
        let notebook: Notebook = serde_json::from_value(document.clone()).unwrap();
        let text = notebook.to_text();
        let parsed = Notebook::from_text(&text).unwrap();
        assert_eq!(
            serde_json::to_value(&parsed).unwrap()["context"],
            document["context"]
        );
        assert!(text.starts_with("# aster notebook v3\n"));
        assert_eq!(
            serde_json::to_value(&parsed.cells[0]).unwrap(),
            document["cells"][0]
        );
        let empty: Notebook = serde_json::from_value(serde_json::json!({
            "id":"empty", "title":"Empty", "context":{}, "cells":[]}))
        .unwrap();
        assert!(empty.to_text().contains("# context: "));
        let legacy =
            Notebook::from_text("# aster notebook v2\n# title: Old\n-- cell c1\nSELECT 1\n")
                .unwrap();
        assert!(serde_json::to_value(&legacy).unwrap()["context"].is_null());
    }

    #[test]
    fn cell_metadata_survives_git_text_roundtrip() {
        let document = serde_json::json!({"id":"bench", "title":"Benchmarks", "cells":[
            {"id":"c1", "sql":"SELECT count(*) FROM nation", "engine":null,
             "metadata":{"catalog_context":"tpch", "schema":"tiny", "other":{"false":false,"zero":0,"text":"a b\n☃"}}}
        ]});
        let notebook: Notebook = serde_json::from_value(document.clone()).unwrap();
        let parsed = Notebook::from_text(&notebook.to_text()).unwrap();
        assert_eq!(
            serde_json::to_value(&parsed.cells[0]).unwrap()["metadata"],
            document["cells"][0]["metadata"]
        );
        let legacy =
            Notebook::from_text("# aster notebook v2\n# title: Old\n-- cell c1\nSELECT 1\n")
                .unwrap();
        assert_eq!(
            serde_json::to_value(&legacy.cells[0]).unwrap()["metadata"],
            serde_json::json!({})
        );
    }

    #[test]
    fn text_round_trips_cells() {
        let notebook = sample();
        let parsed = Notebook::from_text(&notebook.to_text()).unwrap();
        assert_eq!(parsed.title, notebook.title);
        assert_eq!(parsed.cells.len(), 2);
        assert_eq!(parsed.cells[0].sql, "SELECT 1");
        assert_eq!(parsed.cells[0].engine.as_ref().unwrap().0, "trino-local");
        assert_eq!(parsed.cells[1].sql, "SELECT\n  2");
        assert!(parsed.cells[1].engine.is_none());
    }

    #[test]
    fn notebook_engine_round_trips_and_wins_over_legacy_cells() {
        let mut notebook = sample();
        notebook.engine = Some(EngineId::new("trino-notebook"));
        let parsed = Notebook::from_text(&notebook.to_text()).unwrap();
        assert_eq!(
            parsed.engine.as_ref().expect("notebook engine").0,
            "trino-notebook"
        );
        assert_eq!(
            parsed.effective_engine().expect("effective engine").0,
            "trino-notebook"
        );
        assert_eq!(parsed.cells[0].engine.as_ref().unwrap().0, "trino-local");
        let text = notebook.to_text();
        assert!(text.contains("# engine: trino-notebook\n"), "{text}");
    }

    #[test]
    fn legacy_mixed_cell_engines_survive_a_round_trip() {
        let mut notebook = sample();
        notebook.engine = None;
        notebook.cells[0].engine = Some(EngineId::new("trino-a"));
        notebook.cells[1].engine = Some(EngineId::new("trino-b"));
        let parsed = Notebook::from_text(&notebook.to_text()).unwrap();
        assert_eq!(parsed.engine, None);
        assert_eq!(parsed.cells[0].engine.as_ref().unwrap().0, "trino-a");
        assert_eq!(parsed.cells[1].engine.as_ref().unwrap().0, "trino-b");
    }

    #[test]
    fn cell_magic_selects_markdown_and_strips_the_marker() {
        let cell = Cell {
            id: "c1".into(),
            sql: "\n%md\n# Title\n\n```mermaid\ngraph TD; A-->B\n```\n".into(),
            engine: None,
            metadata: Default::default(),
        };
        assert_eq!(cell.kind(), CellKind::Markdown);
        assert_eq!(
            cell.source(),
            "# Title\n\n```mermaid\ngraph TD; A-->B\n```\n"
        );
        let sql = Cell {
            id: "c2".into(),
            sql: "%sql\nSELECT 1".into(),
            engine: None,
            metadata: Default::default(),
        };
        assert_eq!(sql.kind(), CellKind::Sql);
        assert_eq!(sql.source(), "SELECT 1");
        let plain = Cell {
            id: "c3".into(),
            sql: "SELECT 2".into(),
            engine: None,
            metadata: Default::default(),
        };
        assert_eq!(plain.kind(), CellKind::Sql);
        assert_eq!(plain.source(), "SELECT 2");
    }

    #[test]
    fn missing_header_is_rejected() {
        assert!(Notebook::from_text("SELECT 1").is_err());
    }

    #[test]
    fn sql_comment_that_looks_like_a_cell_marker_round_trips() {
        let mut notebook = sample();
        notebook.cells[0].sql = "SELECT 1\n-- cell not-a-cell\nSELECT 2".into();
        let parsed = Notebook::from_text(&notebook.to_text()).unwrap();
        assert_eq!(parsed.cells.len(), notebook.cells.len());
        assert_eq!(parsed.cells[0].sql, notebook.cells[0].sql);
    }

    #[test]
    fn legacy_v1_notebook_remains_readable() {
        let old = "# aster notebook v1\n# title: Sales\n-- cell c1\nSELECT 1\n";
        let notebook = Notebook::from_text(old).unwrap();
        assert_eq!(notebook.title, "Sales");
        assert_eq!(notebook.cells[0].sql, "SELECT 1");
    }

    #[test]
    fn leading_blank_and_backslash_sql_lines_round_trip() {
        let mut notebook = sample();
        notebook.cells[0].sql = "\n\\literal\nSELECT 1\n".into();
        let parsed = Notebook::from_text(&notebook.to_text()).unwrap();
        assert_eq!(parsed.cells[0].sql, notebook.cells[0].sql);
    }
}
