mod fallback;
mod selector;

pub use fallback::FallbackChain;
pub use selector::{CatalogSelector, ModelSelection, ModelSelector, ModelTarget};

use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::models::{Availability, ModelCatalog, ModelId, ModelRequirements};
use crate::policy::ExecutionPolicy;
use crate::task::TaskKind;

/// Policy router. Selection is catalog + capabilities, not vendor name tables.
///
/// The catalog held here is the **effective** catalog (configured + discovery).
#[derive(Debug, Clone)]
pub struct Router {
    policy: ExecutionPolicy,
    catalog: Arc<Mutex<ModelCatalog>>,
    selector: CatalogSelector,
}

impl Router {
    pub fn new(policy: ExecutionPolicy) -> Self {
        Self {
            policy,
            catalog: Arc::new(Mutex::new(ModelCatalog::builtin())),
            selector: CatalogSelector::new(),
        }
    }

    pub fn with_catalog(self, catalog: ModelCatalog) -> Self {
        *self.catalog.lock().expect("catalog mutex") = catalog;
        self
    }

    pub fn catalog(&self) -> ModelCatalog {
        self.catalog.lock().expect("catalog mutex").clone()
    }

    pub fn mark_unavailable(&self, id: &ModelId) {
        self.catalog
            .lock()
            .expect("catalog mutex")
            .mark_unavailable(id);
    }

    pub fn ingest_discovered(
        &self,
        provider: binder_llm::Provider,
        infos: &[binder_llm::ModelInfo],
    ) {
        self.catalog
            .lock()
            .expect("catalog mutex")
            .ingest_discovered(provider, infos);
    }

    pub fn mark_providers(&self, providers: &[binder_llm::Provider], status: Availability) {
        self.catalog
            .lock()
            .expect("catalog mutex")
            .mark_providers(providers, status);
    }

    pub fn policy(&self) -> ExecutionPolicy {
        self.policy
    }

    pub fn route(&self, task: TaskKind, requirements: &ModelRequirements) -> Result<FallbackChain> {
        let catalog = self.catalog();
        let selection = self.selector.select(&task, requirements, &catalog)?;
        FallbackChain::try_from_targets(selection.targets())
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new(ExecutionPolicy::Balanced)
    }
}
