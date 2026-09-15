use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::engine::EngineId;
use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub id: String,
    pub sql: String,
    /// Engine chosen for this cell; `None` means the default engine.
    pub engine: Option<EngineId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notebook {
    pub id: String,
    pub title: String,
    pub cells: Vec<Cell>,
}

/// Git-backed notebook persistence. Implementations commit to the user's
/// repository; the metadata database stores only the index.
#[async_trait]
pub trait NotebookStore: Send + Sync {
    async fn get(&self, id: &str) -> Result<Notebook>;
    async fn save(&self, notebook: &Notebook, actor: &str) -> Result<String>;
    async fn list(&self, actor: &str) -> Result<Vec<String>>;
}

/// Marker for the text-first on-disk format (D8: custom `aster` format). Chosen
/// over `.ipynb` so notebooks diff cleanly in git; the notebook id is the file
/// name, so it is not stored in the body.
const HEADER: &str = "# aster notebook v1";
const TITLE_PREFIX: &str = "# title: ";
const CELL_PREFIX: &str = "-- cell ";

impl Notebook {
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');
        out.push_str(TITLE_PREFIX);
        out.push_str(&self.title);
        out.push('\n');
        for cell in &self.cells {
            out.push_str(CELL_PREFIX);
            out.push_str(&cell.id);
            if let Some(engine) = &cell.engine {
                out.push_str(" engine=");
                out.push_str(&engine.0);
            }
            out.push('\n');
            out.push_str(&cell.sql);
            out.push('\n');
        }
        out
    }

    pub fn from_text(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let header = lines
            .next()
            .ok_or_else(|| CoreError::Invalid("empty notebook".into()))?;
        if header.trim() != HEADER {
            return Err(CoreError::Invalid("missing aster notebook header".into()));
        }

        let mut title = String::from("untitled");
        let mut cells: Vec<Cell> = Vec::new();
        let mut current: Option<Cell> = None;

        for line in lines {
            if let Some(rest) = line.strip_prefix(CELL_PREFIX) {
                if let Some(cell) = current.take() {
                    cells.push(cell);
                }
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
                });
            } else if current.is_none() && line.starts_with(TITLE_PREFIX) {
                title = line[TITLE_PREFIX.len()..].to_string();
            } else if let Some(cell) = current.as_mut() {
                if !cell.sql.is_empty() {
                    cell.sql.push('\n');
                }
                cell.sql.push_str(line);
            }
        }
        if let Some(cell) = current {
            cells.push(cell);
        }

        Ok(Notebook {
            id: String::new(),
            title,
            cells,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Notebook {
        Notebook {
            id: "nb1".into(),
            title: "Sales".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    sql: "SELECT 1".into(),
                    engine: Some(EngineId::new("trino-local")),
                },
                Cell {
                    id: "c2".into(),
                    sql: "SELECT\n  2".into(),
                    engine: None,
                },
            ],
        }
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
    fn missing_header_is_rejected() {
        assert!(Notebook::from_text("SELECT 1").is_err());
    }
}
