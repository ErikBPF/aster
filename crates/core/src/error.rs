use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("engine error: {0}")]
    Engine(String),
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
