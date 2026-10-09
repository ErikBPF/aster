//! aster domain: engine and catalog abstractions, notebook model, RBAC, the
//! session and handshake state ports, and the registries that wire plugins
//! together. No UI, HTTP, or database dependencies live here.

pub mod audit;
pub mod auth;
pub mod catalog;
pub mod config;
pub mod contract;
pub mod engine;
pub mod error;
pub mod grants;
pub mod health;
pub mod identity;
pub mod llm;
pub mod notebook;
pub mod odcs;
pub mod registry;
pub mod secrets;
pub mod semantic;
pub mod shared_models;
pub mod state;

pub use audit::{AuditEvent, AuditSink, InMemoryAudit};
pub use auth::{authorize, Action, Principal, Role, SharedModelGrant};
pub use catalog::{
    Catalog, CatalogId, ColumnSchema, Namespace, TableDescriptor, TableRef, TableSchema,
};
pub use config::{
    AppConfig, BindingPolicy, CatalogBindingConfig, CatalogConfig, EngineConfig,
    TrinoDelegationConfig,
};
pub use contract::{for_table, relevant, ContractField, DataContract};
pub use engine::{Column, EngineId, EngineInfo, QueryEngine, QueryRequest, QueryResult};
pub use error::{CoreError, Result};
pub use grants::{authorize_engine, Grants, InMemoryGrants};
pub use health::Health;
pub use identity::{
    CurrentIdentity, CurrentIdentityProvider, Identity, IdentityHandshake, IdentityProvider,
};
pub use llm::{InMemoryLlm, LlmConfig, LlmStore};
pub use notebook::{
    Cell, InMemoryNotebookOwners, Notebook, NotebookOwnerChange, NotebookOwnerRecord,
    NotebookOwners, NotebookPrecondition, NotebookSave, NotebookSnapshot, NotebookStore,
};
pub use registry::{CatalogRegistry, EngineRegistry};
pub use secrets::{require, EnvSecrets, InMemorySecrets, SecretStore};
pub use semantic::{SemanticFormat, TableModel};
pub use shared_models::{
    AdminSharedModelSummary, EncryptedSharedModel, InMemorySharedModels, SharedModelStore,
};
pub use state::{
    new_sid, HandshakeStore, InMemoryHandshakes, InMemorySessions, InMemoryUserState,
    SessionRecord, SessionRegistry, UserState, WorkingState,
};

pub mod conversation;
pub use conversation::{ChatMessage, Conversation, ConversationStore, InMemoryConversations};

pub mod exchange;
pub use exchange::{CellResult, ExchangeStore, InMemoryExchanges, NotebookExchange};
