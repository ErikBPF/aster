use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("engine error: {0}")]
    Engine(String),
    /// The engine ran the statement and refused it: a SQL, analysis or planning
    /// error. The message is the engine's own and is safe to show the caller, so
    /// a typo reads as a query problem instead of a broken backend.
    #[error("query error: {0}")]
    Query(String),
    #[error("catalog error: {0}")]
    Catalog(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("invalid request: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
