use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::engine::EngineId;
use crate::error::Result;

/// One executed query attempt, recorded regardless of outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub subject: String,
    pub engine: EngineId,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub sql: String,
    pub latency_ms: u64,
    pub row_count: usize,
    pub ok: bool,
}

/// Append-only audit trail. In memory for dev and tests, Postgres in the server.
#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn record(&self, event: &AuditEvent) -> Result<()>;
    async fn events(&self) -> Result<Vec<AuditEvent>>;
}

#[derive(Default)]
pub struct InMemoryAudit {
    events: Mutex<Vec<AuditEvent>>,
}

impl InMemoryAudit {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl AuditSink for InMemoryAudit {
    async fn record(&self, event: &AuditEvent) -> Result<()> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }

    async fn events(&self) -> Result<Vec<AuditEvent>> {
        Ok(self.events.lock().unwrap().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn records_events_append_only() {
        let audit = InMemoryAudit::new();
        assert!(audit.is_empty());

        block_on(audit.record(&AuditEvent {
            subject: "alice".into(),
            engine: EngineId::new("trino-a"),
            catalog: Some("polaris".into()),
            schema: None,
            sql: "SELECT 1".into(),
            latency_ms: 3,
            row_count: 1,
            ok: true,
        }))
        .unwrap();

        assert_eq!(audit.len(), 1);
        assert_eq!(block_on(audit.events()).unwrap()[0].row_count, 1);
    }
}
