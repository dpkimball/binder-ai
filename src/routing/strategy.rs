use crate::error::{AiError, Result};
use crate::policy::RoutingPolicy;
use crate::routing::fallback::FallbackChain;
use crate::routing::selector::{ModelCatalog, ModelTarget};
use crate::task::TaskKind;
use binder_llm::Provider;

/// Maps a [`RoutingPolicy`] onto provider preference order plus task hints.
#[derive(Debug, Clone)]
pub struct RoutingStrategy {
    policy: RoutingPolicy,
}

impl RoutingStrategy {
    pub fn from_policy(policy: RoutingPolicy) -> Self {
        Self { policy }
    }

    pub fn policy(&self) -> RoutingPolicy {
        self.policy
    }

    pub fn provider_order(&self) -> Vec<Provider> {
        match self.policy {
            RoutingPolicy::BestQuality => vec![
                Provider::Anthropic,
                Provider::Openai,
                Provider::Gemini,
                Provider::Xai,
                Provider::Ollama,
            ],
            RoutingPolicy::Fastest | RoutingPolicy::LowestCost => vec![
                Provider::Ollama,
                Provider::Gemini,
                Provider::Xai,
                Provider::Openai,
                Provider::Anthropic,
            ],
            RoutingPolicy::LocalOnly => vec![Provider::Ollama],
            RoutingPolicy::CloudOnly => vec![
                Provider::Anthropic,
                Provider::Openai,
                Provider::Gemini,
                Provider::Xai,
            ],
            RoutingPolicy::Balanced => vec![
                Provider::Openai,
                Provider::Anthropic,
                Provider::Ollama,
                Provider::Gemini,
                Provider::Xai,
            ],
        }
    }

    pub fn select(
        &self,
        task: TaskKind,
        override_provider: Option<Provider>,
        override_model: Option<&str>,
        available: &[Provider],
        catalog: &ModelCatalog,
    ) -> Result<FallbackChain> {
        if let Some(provider) = override_provider {
            if !self.allows(provider) {
                return Err(AiError::routing(format!(
                    "policy {} forbids provider {}",
                    self.policy,
                    provider.as_str()
                )));
            }
            let model = override_model
                .map(str::to_string)
                .unwrap_or_else(|| catalog.default_model(provider, task).to_string());
            let mut rest = self.ordered_available(available);
            rest.retain(|p| *p != provider);
            let mut chain = vec![ModelTarget { provider, model }];
            chain.extend(rest.into_iter().map(|p| ModelTarget {
                provider: p,
                model: catalog.default_model(p, task).to_string(),
            }));
            return FallbackChain::try_from_targets(chain);
        }

        let mut targets = Vec::new();
        if let Some(preferred) = catalog.preferred_for_task(task, self.policy) {
            if self.allows(preferred.provider) && available.contains(&preferred.provider) {
                let mut preferred = preferred;
                if let Some(model) = override_model {
                    preferred.model = model.to_string();
                }
                targets.push(preferred);
            }
        }
        for provider in self.ordered_available(available) {
            if targets.iter().any(|t| t.provider == provider) {
                continue;
            }
            let model = override_model
                .map(str::to_string)
                .unwrap_or_else(|| catalog.default_model(provider, task).to_string());
            targets.push(ModelTarget { provider, model });
        }
        FallbackChain::try_from_targets(targets)
    }

    fn allows(&self, provider: Provider) -> bool {
        if provider.is_local() {
            self.policy.allows_local()
        } else {
            self.policy.allows_cloud()
        }
    }

    fn ordered_available(&self, available: &[Provider]) -> Vec<Provider> {
        self.provider_order()
            .into_iter()
            .filter(|p| available.contains(p) && self.allows(*p))
            .collect()
    }
}
