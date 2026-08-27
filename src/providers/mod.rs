use std::collections::HashMap;

use crate::config::ProviderSettings;
use crate::error::{AiError, Result};
use crate::routing::ModelTarget;
use binder_llm::{LlmClient, Provider};

/// Builds `LlmClient` instances from routing targets. No HTTP of its own.
pub trait LlmFactory: Send + Sync {
    fn client_for(&self, target: &ModelTarget) -> Result<LlmClient>;
}

/// Constructs real `binder-llm` clients from configured endpoints and env keys.
#[derive(Debug, Clone)]
pub struct DefaultLlmFactory {
    settings: HashMap<Provider, ProviderSettings>,
}

impl DefaultLlmFactory {
    pub fn new(settings: HashMap<Provider, ProviderSettings>) -> Self {
        Self { settings }
    }
}

impl LlmFactory for DefaultLlmFactory {
    fn client_for(&self, target: &ModelTarget) -> Result<LlmClient> {
        let settings = self
            .settings
            .get(&target.provider)
            .cloned()
            .unwrap_or_else(|| ProviderSettings::defaults_for(target.provider));
        let mut builder = LlmClient::builder()
            .provider(target.provider)
            .model(&target.model);
        if let Some(url) = settings.base_url.filter(|u| !u.is_empty()) {
            builder = builder.base_url(url);
        }
        if let Some(key) = settings.api_key.filter(|k| !k.is_empty()) {
            builder = builder.api_key(key);
        } else if let Some(env_name) = settings.api_key_env {
            if let Ok(key) = std::env::var(&env_name) {
                if !key.is_empty() {
                    builder = builder.api_key(key);
                }
            }
        }
        builder.build().map_err(AiError::from)
    }
}

/// Test factory that returns injected clients per provider.
#[derive(Clone)]
pub struct StaticLlmFactory {
    clients: HashMap<Provider, LlmClient>,
    fallback: Option<LlmClient>,
}

impl StaticLlmFactory {
    pub fn new(client: LlmClient) -> Self {
        Self {
            clients: HashMap::new(),
            fallback: Some(client),
        }
    }

    pub fn with_provider(mut self, provider: Provider, client: LlmClient) -> Self {
        self.clients.insert(provider, client);
        self
    }
}

impl LlmFactory for StaticLlmFactory {
    fn client_for(&self, target: &ModelTarget) -> Result<LlmClient> {
        self.clients
            .get(&target.provider)
            .cloned()
            .or_else(|| self.fallback.clone())
            .ok_or_else(|| {
                AiError::routing(format!(
                    "no LLM client configured for {}",
                    target.provider.as_str()
                ))
            })
    }
}
