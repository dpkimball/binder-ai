mod fallback;
mod selector;

pub use fallback::FallbackChain;
pub use selector::{CatalogSelector, ModelSelection, ModelSelector, ModelTarget};

use crate::error::Result;
use crate::models::{ModelCatalog, ModelRequirements};
use crate::policy::ExecutionPolicy;
use crate::task::TaskKind;

/// Policy router. Selection is catalog + capabilities, not vendor name tables.
#[derive(Debug, Clone)]
pub struct Router {
    policy: ExecutionPolicy,
    catalog: ModelCatalog,
    selector: CatalogSelector,
}

impl Router {
    pub fn new(policy: ExecutionPolicy) -> Self {
        Self {
            policy,
            catalog: ModelCatalog::builtin(),
            selector: CatalogSelector::new(),
        }
    }

    pub fn with_catalog(mut self, catalog: ModelCatalog) -> Self {
        self.catalog = catalog;
        self
    }

    pub fn catalog(&self) -> &ModelCatalog {
        &self.catalog
    }

    pub fn catalog_mut(&mut self) -> &mut ModelCatalog {
        &mut self.catalog
    }

    pub fn policy(&self) -> ExecutionPolicy {
        self.policy
    }

    pub fn route(&self, task: TaskKind, requirements: &ModelRequirements) -> Result<FallbackChain> {
        let selection = self.selector.select(&task, requirements, &self.catalog)?;
        FallbackChain::try_from_targets(selection.targets())
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new(ExecutionPolicy::Balanced)
    }
}
