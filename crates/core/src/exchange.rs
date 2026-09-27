//! The notebook session's view of cell material.
//!
//! The notebook session discusses the whole notebook without reading any cell
//! conversation. It reaches a cell only through the exchange operations, and
//! what it can reach is the cell's SQL (from the notebook document) and the
//! last result a run of that cell recorded here.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// A bounded snapshot of one cell's last run result.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellResult {
    pub columns: Vec<String>,
    pub rows_json: Vec<String>,
    pub truncated: bool,
}

/// Everything the exchange holds for one notebook.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NotebookExchange {
    pub summary: Option<String>,
    pub results: HashMap<String, CellResult>,
}

/// Storage for the notebook summary and each cell's last recorded result.
#[async_trait]
pub trait ExchangeStore: Send + Sync {
    async fn get(&self, subject: &str, notebook: &str) -> Result<NotebookExchange>;
    async fn set_summary(
        &self,
        subject: &str,
        notebook: &str,
        summary: Option<String>,
    ) -> Result<()>;
    async fn set_result(
        &self,
        subject: &str,
        notebook: &str,
        cell: &str,
        result: CellResult,
    ) -> Result<()>;
}

#[derive(Default)]
pub struct InMemoryExchanges {
    exchanges: Mutex<HashMap<(String, String), NotebookExchange>>,
}

impl InMemoryExchanges {
    fn lock(&self) -> Result<MutexGuard<'_, HashMap<(String, String), NotebookExchange>>> {
        self.exchanges
            .lock()
            .map_err(|_| CoreError::Storage("exchange lock poisoned".into()))
    }
}

#[async_trait]
impl ExchangeStore for InMemoryExchanges {
    async fn get(&self, subject: &str, notebook: &str) -> Result<NotebookExchange> {
        Ok(self
            .lock()?
            .get(&(subject.to_string(), notebook.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    async fn set_summary(
        &self,
        subject: &str,
        notebook: &str,
        summary: Option<String>,
    ) -> Result<()> {
        self.lock()?
            .entry((subject.to_string(), notebook.to_string()))
            .or_default()
            .summary = summary;
        Ok(())
    }

    async fn set_result(
        &self,
        subject: &str,
        notebook: &str,
        cell: &str,
        result: CellResult,
    ) -> Result<()> {
        self.lock()?
            .entry((subject.to_string(), notebook.to_string()))
            .or_default()
            .results
            .insert(cell.to_string(), result);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exchanges_are_scoped_to_subject_and_notebook() {
        let store = InMemoryExchanges::default();
        store
            .set_summary("alice", "sales", Some("revenue".into()))
            .await
            .unwrap();
        store
            .set_result(
                "alice",
                "sales",
                "q1",
                CellResult {
                    columns: vec!["one".into()],
                    rows_json: vec!["[1]".into()],
                    truncated: false,
                },
            )
            .await
            .unwrap();
        let sales = store.get("alice", "sales").await.unwrap();
        assert_eq!(sales.summary.as_deref(), Some("revenue"));
        assert_eq!(sales.results["q1"].rows_json, vec!["[1]".to_string()]);
        assert_eq!(
            store.get("alice", "ops").await.unwrap(),
            NotebookExchange::default()
        );
        assert_eq!(
            store.get("bob", "sales").await.unwrap(),
            NotebookExchange::default()
        );
    }
}
