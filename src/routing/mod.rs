mod fallback;
mod selector;
mod strategy;

pub use fallback::FallbackChain;
pub use selector::{ModelCatalog, ModelTarget};
pub use strategy::RoutingStrategy;

use crate::error::Result;
use crate::policy::RoutingPolicy;
use crate::task::TaskKind;
use binder_llm::Provider;

/// Policy router. Selects a primary target and an ordered fallback list.
#[derive(Debug, Clone)]
pub struct Router {
    strategy: RoutingStrategy,
    catalog: ModelCatalog,
}

impl Router {
    pub fn new(policy: RoutingPolicy) -> Self {
        Self {
            strategy: RoutingStrategy::from_policy(policy),
            catalog: ModelCatalog::builtin(),
        }
    }

    pub fn with_catalog(mut self, catalog: ModelCatalog) -> Self {
        self.catalog = catalog;
        self
    }

    pub fn policy(&self) -> RoutingPolicy {
        self.strategy.policy()
    }

    pub fn route(
        &self,
        task: TaskKind,
        policy_override: Option<RoutingPolicy>,
        override_provider: Option<Provider>,
        override_model: Option<&str>,
        available: &[Provider],
    ) -> Result<FallbackChain> {
        if let (Some(provider), Some(model)) = (override_provider, override_model) {
            return Ok(FallbackChain::single(ModelTarget {
                provider,
                model: model.to_string(),
            }));
        }
        let policy = policy_override.unwrap_or(self.strategy.policy());
        let strategy = RoutingStrategy::from_policy(policy);
        strategy.select(
            task,
            override_provider,
            override_model,
            available,
            &self.catalog,
        )
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new(RoutingPolicy::Balanced)
    }
}
