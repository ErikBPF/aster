//! aster domain: engine and catalog abstractions, notebook model, RBAC, and
//! the registries that wire plugins together. No UI, HTTP, or database
//! dependencies live here.

pub mod audit;
pub mod auth;
pub mod catalog;
pub mod config;
pub mod contract;
pub mod engine;
pub mod error;
pub mod grants;
pub mod llm;
pub mod notebook;
pub mod registry;
pub mod session;

pub use audit::{AuditEvent, AuditSink, InMemoryAudit};
pub use auth::{authorize, Action, Principal, Role};
pub use catalog::{
    Catalog, CatalogHealth, CatalogId, ColumnSchema, Namespace, TableRef, TableSchema,
};
pub use config::{AppConfig, CatalogConfig, EngineConfig};
pub use contract::{relevant, ContractField, DataContract};
pub use engine::{
    Column, EngineHealth, EngineId, EngineInfo, QueryEngine, QueryRequest, QueryResult,
};
pub use error::{CoreError, Result};
pub use grants::{authorize_engine, Grants, InMemoryGrants};
pub use llm::{InMemoryLlm, LlmConfig, LlmStore};
pub use notebook::{Cell, Notebook, NotebookStore};
pub use registry::{CatalogRegistry, EngineRegistry};
pub use session::{Session, SessionStore};
