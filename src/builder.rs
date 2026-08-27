use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::cache::{AiCache, MemoryCache};
use crate::client::AiClient;
use crate::config::{AiConfig, ProviderSettings, TokenBudget};
use crate::conversation::{ConversationStore, InMemoryConversationStore};
use crate::models::{Availability, ExecutionMode, ModelCatalog, ModelId};
use crate::policy::RoutingPolicy;
use crate::prompt::manager::PromptManager;
use crate::prompt::PromptRegistry;
use crate::providers::{DefaultLlmFactory, LlmFactory, StaticLlmFactory};
use crate::routing::{ModelTarget, Router};
use crate::task::TaskKind;
use crate::telemetry::AiMetrics;
use crate::tools::{ToolExecutor, ToolRegistry, UnboundToolExecutor};
use binder_llm::{LlmClient, Provider};

/// Fluent builder for [`AiClient`].
pub struct AiClientBuilder {
    routing_policy: RoutingPolicy,
    preferred_provider: Option<Provider>,
    preferred_models: HashMap<TaskKind, String>,
    fallback_order: Vec<Provider>,
    semantic_retries: u32,
    max_tool_rounds: u32,
    token_budget: TokenBudget,
    cost_limit_usd: Option<f64>,
    provider_settings: HashMap<Provider, ProviderSettings>,
    cache_enabled: bool,
    prompt_registry: Option<PromptRegistry>,
    tool_registry: Option<ToolRegistry>,
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    conversation_store: Option<Arc<dyn ConversationStore>>,
    cache: Option<Arc<dyn AiCache>>,
    llm_factory: Option<Arc<dyn LlmFactory>>,
    available_providers: Option<Vec<Provider>>,
    catalog: Option<ModelCatalog>,
}

impl Default for AiClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AiClientBuilder {
    pub fn new() -> Self {
        Self {
            routing_policy: RoutingPolicy::Balanced,
            preferred_provider: None,
            preferred_models: HashMap::new(),
            fallback_order: Vec::new(),
            semantic_retries: 2,
            max_tool_rounds: 8,
            token_budget: TokenBudget::unlimited(),
            cost_limit_usd: None,
            provider_settings: default_provider_settings(),
            cache_enabled: false,
            prompt_registry: None,
            tool_registry: None,
            tool_executor: None,
            conversation_store: None,
            cache: None,
            llm_factory: None,
            available_providers: None,
            catalog: None,
        }
    }

    pub fn routing_policy(mut self, policy: RoutingPolicy) -> Self {
        self.routing_policy = policy;
        self
    }

    pub fn preferred_provider(mut self, provider: Provider) -> Self {
        self.preferred_provider = Some(provider);
        self
    }

    pub fn preferred_model(mut self, task: TaskKind, model: impl Into<String>) -> Self {
        self.preferred_models.insert(task, model.into());
        self
    }

    pub fn fallback_order(mut self, order: Vec<Provider>) -> Self {
        self.fallback_order = order;
        self
    }

    pub fn semantic_retries(mut self, retries: u32) -> Self {
        self.semantic_retries = retries;
        self
    }

    pub fn max_tool_rounds(mut self, rounds: u32) -> Self {
        self.max_tool_rounds = rounds;
        self
    }

    pub fn token_budget(mut self, budget: TokenBudget) -> Self {
        self.token_budget = budget;
        self
    }

    pub fn cost_limit_usd(mut self, limit: f64) -> Self {
        self.cost_limit_usd = Some(limit);
        self
    }

    pub fn provider_settings(mut self, provider: Provider, settings: ProviderSettings) -> Self {
        self.provider_settings.insert(provider, settings);
        self
    }

    pub fn cache_enabled(mut self, enabled: bool) -> Self {
        self.cache_enabled = enabled;
        self
    }

    pub fn prompt_registry(mut self, registry: PromptRegistry) -> Self {
        self.prompt_registry = Some(registry);
        self
    }

    pub fn tool_registry(mut self, registry: ToolRegistry) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn tool_executor(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        self.tool_executor = Some(executor);
        self
    }

    pub fn conversation_store(mut self, store: Arc<dyn ConversationStore>) -> Self {
        self.conversation_store = Some(store);
        self
    }

    pub fn cache(mut self, cache: Arc<dyn AiCache>) -> Self {
        self.cache = Some(cache);
        self.cache_enabled = true;
        self
    }

    /// Inject a single LLM client (tests). Routing still runs unless providers are pinned.
    pub fn llm(mut self, client: LlmClient) -> Self {
        let provider = client.provider_kind();
        self.available_providers = Some(vec![provider]);
        self.llm_factory = Some(Arc::new(StaticLlmFactory::new(client)));
        self
    }

    pub fn llm_factory(mut self, factory: Arc<dyn LlmFactory>) -> Self {
        self.llm_factory = Some(factory);
        self
    }

    pub fn available_providers(mut self, providers: Vec<Provider>) -> Self {
        self.available_providers = Some(providers);
        self
    }

    pub fn catalog(mut self, catalog: ModelCatalog) -> Self {
        self.catalog = Some(catalog);
        self
    }

    /// Infallible, matching the spec `AiClient::builder().build()`.
    ///
    /// Local providers are discovered here. The router receives the **effective**
    /// catalog only (configured + discovery). Configured local models are never
    /// silently marked Available.
    pub fn build(self) -> AiClient {
        let config = AiConfig {
            routing_policy: self.routing_policy,
            preferred_provider: self.preferred_provider,
            preferred_models: self.preferred_models,
            fallback_order: self.fallback_order,
            semantic_retries: self.semantic_retries,
            max_tool_rounds: self.max_tool_rounds,
            token_budget: self.token_budget,
            cost_limit_usd: self.cost_limit_usd,
            provider_settings: self.provider_settings.clone(),
            cache_enabled: self.cache_enabled,
        };
        let registry = self.prompt_registry.unwrap_or_else(PromptRegistry::builtin);
        let factory = self
            .llm_factory
            .unwrap_or_else(|| Arc::new(DefaultLlmFactory::new(self.provider_settings.clone())));
        let cache = if self.cache_enabled {
            Some(self.cache.unwrap_or_else(|| Arc::new(MemoryCache::new())))
        } else {
            self.cache
        };
        let mut catalog = self.catalog.unwrap_or_else(ModelCatalog::builtin);
        let selectable = self
            .available_providers
            .clone()
            .unwrap_or_else(|| config.available_providers());
        let configured_ids = catalog.ids();
        tracing::info!(
            configured = configured_ids.join(","),
            enabled_providers = selectable
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(","),
            "catalog.configured"
        );
        catalog.mark_all_except(&selectable, Availability::Unavailable);
        for provider in &selectable {
            if ExecutionMode::from_provider(*provider) == ExecutionMode::Cloud {
                catalog.mark_providers(&[*provider], Availability::Available);
            }
        }
        for provider in &selectable {
            if !provider.is_local() {
                continue;
            }
            match discover_installed(factory.as_ref(), *provider) {
                Ok(installed) => {
                    tracing::info!(
                        provider = %provider,
                        installed = installed.join(","),
                        "catalog.discovered"
                    );
                    catalog.ingest_names(*provider, &installed);
                }
                Err(err) => {
                    tracing::warn!(
                        provider = %provider,
                        error_kind = err.kind(),
                        "catalog.discovery_failed"
                    );
                    catalog.mark_providers(&[*provider], Availability::Unavailable);
                }
            }
        }
        tracing::info!(
            available = catalog.ids_with(Availability::Available).join(","),
            unavailable = catalog.ids_with(Availability::Unavailable).join(","),
            "catalog.effective"
        );
        AiClient {
            config,
            router: Router::new(self.routing_policy).with_catalog(catalog),
            prompts: PromptManager::new(registry),
            tools: self
                .tool_registry
                .unwrap_or_else(ToolRegistry::binder_defaults),
            executor: self
                .tool_executor
                .unwrap_or_else(|| Arc::new(UnboundToolExecutor)),
            store: self
                .conversation_store
                .unwrap_or_else(|| Arc::new(InMemoryConversationStore::new())),
            cache,
            factory,
            available_providers: self.available_providers,
            metrics: std::sync::Arc::new(Mutex::new(AiMetrics::default())),
            budgets: std::sync::Arc::new(Mutex::new(crate::config::BudgetLedger::default())),
        }
    }
}

fn discover_installed(
    factory: &dyn LlmFactory,
    provider: Provider,
) -> crate::error::Result<Vec<String>> {
    let probe = ModelTarget {
        provider,
        model: "probe".into(),
        id: ModelId::new(provider, "probe"),
    };
    let client = factory.client_for(&probe)?;
    let configured = client.config().model().to_string();
    let client_for_list = client.clone();
    let infos = block_on_discovery(async move { client_for_list.list_models().await })?;
    let mut names: Vec<String> = infos
        .into_iter()
        .filter(|info| info.provider == provider)
        .map(|info| info.id)
        .collect();
    if !configured.is_empty() && configured != "probe" && !names.iter().any(|n| n == &configured) {
        names.push(configured);
    }
    Ok(names)
}

fn block_on_discovery<T, F>(fut: F) -> T
where
    T: Send + 'static,
    F: std::future::Future<Output = T> + Send + 'static,
{
    std::thread::Builder::new()
        .name("binder-ai-discovery".into())
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("discovery runtime")
                .block_on(fut)
        })
        .expect("discovery thread")
        .join()
        .expect("discovery join")
}

fn default_provider_settings() -> HashMap<Provider, ProviderSettings> {
    [
        Provider::Ollama,
        Provider::Openai,
        Provider::Anthropic,
        Provider::Xai,
        Provider::Gemini,
    ]
    .into_iter()
    .map(|p| (p, ProviderSettings::defaults_for(p)))
    .collect()
}
