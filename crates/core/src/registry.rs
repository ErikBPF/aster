use std::collections::HashMap;
use std::sync::Arc;

use crate::catalog::{Catalog, CatalogId};
use crate::engine::{EngineId, QueryEngine};

#[derive(Default)]
pub struct EngineRegistry {
    engines: HashMap<EngineId, Arc<dyn QueryEngine>>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, engine: Arc<dyn QueryEngine>) {
        self.engines.insert(engine.info().id.clone(), engine);
    }

    pub fn get(&self, id: &EngineId) -> Option<Arc<dyn QueryEngine>> {
        self.engines.get(id).cloned()
    }

    pub fn list(&self) -> Vec<Arc<dyn QueryEngine>> {
        self.engines.values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.engines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.engines.is_empty()
    }
}

#[derive(Default)]
pub struct CatalogRegistry {
    catalogs: HashMap<CatalogId, Arc<dyn Catalog>>,
}

impl CatalogRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, catalog: Arc<dyn Catalog>) {
        self.catalogs.insert(catalog.id().clone(), catalog);
    }

    pub fn get(&self, id: &CatalogId) -> Option<Arc<dyn Catalog>> {
        self.catalogs.get(id).cloned()
    }

    pub fn list(&self) -> Vec<Arc<dyn Catalog>> {
        self.catalogs.values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.catalogs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.catalogs.is_empty()
    }
}
