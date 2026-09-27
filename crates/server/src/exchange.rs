//! PostgreSQL-backed session exchange store.

use std::collections::HashMap;
use std::fmt::Display;

use aster_core::{
    Cell, CellResult, CoreError, ExchangeStore, Notebook, NotebookExchange, NotebookPrecondition,
    Principal, Result,
};
use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::AppState;

pub struct PgExchanges {
    pub(crate) pool: PgPool,
}

fn storage(error: impl Display) -> CoreError {
    CoreError::Storage(error.to_string())
}

impl PgExchanges {
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(storage)?;
        sqlx::raw_sql(include_str!("../../../migrations/0010_exchange.sql"))
            .execute(&pool)
            .await
            .map_err(storage)?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl ExchangeStore for PgExchanges {
    async fn get(&self, subject: &str, notebook: &str) -> Result<NotebookExchange> {
        let summary: Option<String> = sqlx::query_scalar(
            "SELECT summary FROM notebook_exchange_summary WHERE subject = $1 AND notebook = $2",
        )
        .bind(subject)
        .bind(notebook)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?
        .flatten();
        let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
            "SELECT cell, result FROM notebook_exchange_result WHERE subject = $1 AND notebook = $2",
        )
        .bind(subject)
        .bind(notebook)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let mut results = HashMap::new();
        for (cell, value) in rows {
            if let Ok(result) = serde_json::from_value::<CellResult>(value) {
                results.insert(cell, result);
            }
        }
        Ok(NotebookExchange { summary, results })
    }

    async fn set_summary(
        &self,
        subject: &str,
        notebook: &str,
        summary: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO notebook_exchange_summary (subject, notebook, summary) VALUES ($1, $2, $3) \
             ON CONFLICT (subject, notebook) DO UPDATE SET summary = EXCLUDED.summary, updated_at = now()",
        )
        .bind(subject)
        .bind(notebook)
        .bind(summary)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }

    async fn set_result(
        &self,
        subject: &str,
        notebook: &str,
        cell: &str,
        result: CellResult,
    ) -> Result<()> {
        let value = serde_json::to_value(&result).map_err(storage)?;
        sqlx::query(
            "INSERT INTO notebook_exchange_result (subject, notebook, cell, result) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (subject, notebook, cell) DO UPDATE SET result = EXCLUDED.result, updated_at = now()",
        )
        .bind(subject)
        .bind(notebook)
        .bind(cell)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(())
    }
}

/// The most recent rows a cell result keeps for the session exchange.
const RESULT_ROWS: usize = 100;
/// The summary is one short paragraph, like a chat message.
const SUMMARY_LIMIT: usize = 8 * 1024;

fn cell_id(cell: &str) -> Result<()> {
    if aster_core::llm::valid_id(cell) {
        Ok(())
    } else {
        Err(CoreError::Invalid("invalid cell id".into()))
    }
}

fn find_cell<'a>(notebook: &'a Notebook, cell: &str) -> Result<&'a Cell> {
    notebook
        .cells
        .iter()
        .find(|existing| existing.id == cell)
        .ok_or_else(|| CoreError::NotFound(format!("cell {cell}")))
}

/// Record a cell's last result. The caller logs a failure and keeps the run:
/// an exchange bookkeeping error never fails a query that already ran.
pub(crate) async fn record_result(
    state: &AppState,
    principal: &Principal,
    notebook: &str,
    cell: &str,
    columns: &[String],
    rows_json: &[String],
    truncated: bool,
) -> Result<()> {
    if !aster_core::llm::valid_id(notebook) {
        return Err(CoreError::Invalid("invalid notebook id".into()));
    }
    cell_id(cell)?;
    let rows = rows_json.len().min(RESULT_ROWS);
    state
        .exchanges
        .set_result(
            &principal.subject,
            notebook,
            cell,
            CellResult {
                columns: columns.to_vec(),
                rows_json: rows_json[..rows].to_vec(),
                truncated: truncated || rows < rows_json.len(),
            },
        )
        .await
}

/// One cell's SQL from the notebook document and the revision it was read at.
pub(crate) async fn fetch_query(
    state: &AppState,
    principal: &Principal,
    notebook: &str,
    cell: &str,
) -> Result<(String, String)> {
    crate::conversations::check_notebook(state, principal, notebook).await?;
    cell_id(cell)?;
    let snapshot = state.notebooks.snapshot(notebook).await?;
    let existing = find_cell(&snapshot.notebook, cell)?;
    Ok((existing.sql.clone(), snapshot.content_revision))
}

/// One cell's last recorded result; an absent result is simply empty.
pub(crate) async fn fetch_result(
    state: &AppState,
    principal: &Principal,
    notebook: &str,
    cell: &str,
) -> Result<CellResult> {
    crate::conversations::check_notebook(state, principal, notebook).await?;
    cell_id(cell)?;
    let exchange = state.exchanges.get(&principal.subject, notebook).await?;
    Ok(exchange.results.get(cell).cloned().unwrap_or_default())
}

/// The notebook summary and the cell index.
pub(crate) async fn fetch_summary(
    state: &AppState,
    principal: &Principal,
    notebook: &str,
) -> Result<(Option<String>, Vec<(String, String)>)> {
    crate::conversations::check_notebook(state, principal, notebook).await?;
    let document = state.notebooks.get(notebook).await?;
    let cells = document
        .cells
        .iter()
        .map(|cell| (cell.id.clone(), cell.sql.clone()))
        .collect();
    let exchange = state.exchanges.get(&principal.subject, notebook).await?;
    Ok((exchange.summary, cells))
}

/// Replace the notebook summary.
pub(crate) async fn send_summary(
    state: &AppState,
    principal: &Principal,
    notebook: &str,
    summary: Option<String>,
) -> Result<()> {
    crate::conversations::check_notebook(state, principal, notebook).await?;
    if let Some(summary) = &summary {
        if summary.len() > SUMMARY_LIMIT {
            return Err(CoreError::Invalid("summary is limited to 8 KiB".into()));
        }
    }
    state
        .exchanges
        .set_summary(&principal.subject, notebook, summary)
        .await
}

/// Replace one cell's SQL, refusing a stale content revision as a conflict.
pub(crate) async fn update_query(
    state: &AppState,
    principal: &Principal,
    notebook: &str,
    cell: &str,
    sql: &str,
    expected_content_revision: &str,
) -> Result<String> {
    crate::conversations::check_notebook(state, principal, notebook).await?;
    aster_core::authorize(principal, aster_core::Action::WriteNotebook)?;
    cell_id(cell)?;
    if sql.trim().is_empty() {
        return Err(CoreError::Invalid("cell SQL is required".into()));
    }
    let mut snapshot = state.notebooks.snapshot(notebook).await?;
    let index = snapshot
        .notebook
        .cells
        .iter()
        .position(|existing| existing.id == cell)
        .ok_or_else(|| CoreError::NotFound(format!("cell {cell}")))?;
    snapshot.notebook.cells[index].sql = sql.to_string();
    let saved = state
        .save_notebook_owned(
            &snapshot.notebook,
            &principal.subject,
            NotebookPrecondition::Blob(expected_content_revision.to_string()),
        )
        .await?;
    Ok(saved.content_revision)
}
