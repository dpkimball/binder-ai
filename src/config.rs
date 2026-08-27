use std::collections::{BTreeMap, HashMap};
use std::fmt;

use crate::policy::RoutingPolicy;
use crate::session::{UserId, WorkflowId};
use crate::task::TaskKind;
use binder_llm::Provider;

/// Immutable client configuration. Construct via [`crate::AiClient::builder`].
#[derive(Clone)]
pub struct AiConfig {
    pub(crate) routing_policy: RoutingPolicy,
    pub(crate) preferred_provider: Option<Provider>,
    pub(crate) preferred_models: HashMap<TaskKind, String>,
    pub(crate) fallback_order: Vec<Provider>,
    pub(crate) semantic_retries: u32,
    pub(crate) max_tool_rounds: u32,
    pub(crate) token_budget: TokenBudget,
    pub(crate) cost_limit_usd: Option<f64>,
    pub(crate) provider_settings: HashMap<Provider, ProviderSettings>,
    pub(crate) cache_enabled: bool,
}

impl AiConfig {
    pub fn routing_policy(&self) -> RoutingPolicy {
        self.routing_policy
    }

    pub fn semantic_retries(&self) -> u32 {
        self.semantic_retries
    }

    pub fn max_tool_rounds(&self) -> u32 {
        self.max_tool_rounds
    }

    pub fn token_budget(&self) -> &TokenBudget {
        &self.token_budget
    }

    pub fn cost_limit_usd(&self) -> Option<f64> {
        self.cost_limit_usd
    }

    pub fn cache_enabled(&self) -> bool {
        self.cache_enabled
    }

    pub fn preferred_model(&self, task: TaskKind) -> Option<&str> {
        self.preferred_models.get(&task).map(String::as_str)
    }

    pub fn has_credentials(&self, provider: Provider) -> bool {
        if provider.is_local() {
            return true;
        }
        let Some(settings) = self.provider_settings.get(&provider) else {
            return env_has_key(provider);
        };
        if settings.api_key.as_ref().is_some_and(|k| !k.is_empty()) {
            return true;
        }
        if let Some(env_name) = &settings.api_key_env {
            return std::env::var(env_name).is_ok_and(|v| !v.is_empty());
        }
        env_has_key(provider)
    }

    pub fn available_providers(&self) -> Vec<Provider> {
        let all = [
            Provider::Ollama,
            Provider::Openai,
            Provider::Anthropic,
            Provider::Xai,
            Provider::Gemini,
        ];
        all.into_iter()
            .filter(|p| self.has_credentials(*p))
            .collect()
    }
}

fn env_has_key(provider: Provider) -> bool {
    let vars = match provider {
        Provider::Ollama => return true,
        Provider::Openai => &["OPENAI_API_KEY"][..],
        Provider::Anthropic => &["ANTHROPIC_API_KEY"],
        Provider::Xai => &["XAI_API_KEY", "GROK_API_KEY"],
        Provider::Gemini => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
    };
    vars.iter()
        .any(|name| std::env::var(name).is_ok_and(|v| !v.is_empty()))
}

impl fmt::Debug for AiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiConfig")
            .field("routing_policy", &self.routing_policy)
            .field("preferred_provider", &self.preferred_provider)
            .field("preferred_models", &self.preferred_models)
            .field("fallback_order", &self.fallback_order)
            .field("semantic_retries", &self.semantic_retries)
            .field("max_tool_rounds", &self.max_tool_rounds)
            .field("token_budget", &self.token_budget)
            .field("cost_limit_usd", &self.cost_limit_usd)
            .field(
                "provider_settings",
                &redact_settings(&self.provider_settings),
            )
            .field("cache_enabled", &self.cache_enabled)
            .finish()
    }
}

fn redact_settings(
    settings: &HashMap<Provider, ProviderSettings>,
) -> HashMap<Provider, ProviderSettings> {
    settings
        .iter()
        .map(|(k, v)| {
            let mut copy = v.clone();
            if copy.api_key.is_some() {
                copy.api_key = Some("[redacted]".into());
            }
            (*k, copy)
        })
        .collect()
}

/// Endpoint settings passed through to `binder-llm`. This crate does not authenticate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderSettings {
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

impl ProviderSettings {
    pub fn defaults_for(provider: Provider) -> Self {
        let api_key_env = match provider {
            Provider::Ollama => None,
            Provider::Openai => Some("OPENAI_API_KEY".into()),
            Provider::Anthropic => Some("ANTHROPIC_API_KEY".into()),
            Provider::Xai => Some("XAI_API_KEY".into()),
            Provider::Gemini => Some("GEMINI_API_KEY".into()),
        };
        Self {
            api_key: None,
            api_key_env,
            base_url: None,
            default_model: None,
        }
    }
}

/// Pluggable token limits. Zero means unlimited for that scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenBudget {
    pub per_request: Option<u32>,
    pub per_session: Option<u32>,
    pub per_user: Option<u32>,
    pub per_workflow: Option<u32>,
}

impl TokenBudget {
    pub fn unlimited() -> Self {
        Self::default()
    }
}

/// Running counters for budget scopes.
#[derive(Debug, Default)]
pub struct BudgetLedger {
    pub session: BTreeMap<String, u32>,
    pub user: BTreeMap<String, u32>,
    pub workflow: BTreeMap<String, u32>,
    pub spent_usd: f64,
}

impl BudgetLedger {
    pub fn check_request(&self, budget: &TokenBudget, estimated: u32) -> crate::Result<()> {
        if let Some(limit) = budget.per_request {
            if estimated > limit {
                return Err(crate::AiError::budget(format!(
                    "per-request limit {limit} tokens, need {estimated}"
                )));
            }
        }
        Ok(())
    }

    pub fn check_scope(
        &self,
        budget: &TokenBudget,
        session: Option<&str>,
        user: Option<&UserId>,
        workflow: Option<&WorkflowId>,
        add: u32,
    ) -> crate::Result<()> {
        if let (Some(limit), Some(id)) = (budget.per_session, session) {
            let used = self.session.get(id).copied().unwrap_or(0);
            if used.saturating_add(add) > limit {
                return Err(crate::AiError::budget(format!(
                    "per-session limit {limit} tokens exceeded"
                )));
            }
        }
        if let (Some(limit), Some(id)) = (budget.per_user, user) {
            let used = self.user.get(&id.0).copied().unwrap_or(0);
            if used.saturating_add(add) > limit {
                return Err(crate::AiError::budget(format!(
                    "per-user limit {limit} tokens exceeded"
                )));
            }
        }
        if let (Some(limit), Some(id)) = (budget.per_workflow, workflow) {
            let used = self.workflow.get(&id.0).copied().unwrap_or(0);
            if used.saturating_add(add) > limit {
                return Err(crate::AiError::budget(format!(
                    "per-workflow limit {limit} tokens exceeded"
                )));
            }
        }
        Ok(())
    }

    pub fn record(
        &mut self,
        session: Option<&str>,
        user: Option<&UserId>,
        workflow: Option<&WorkflowId>,
        tokens: u32,
        usd: f64,
    ) {
        if let Some(id) = session {
            *self.session.entry(id.to_string()).or_insert(0) += tokens;
        }
        if let Some(id) = user {
            *self.user.entry(id.0.clone()).or_insert(0) += tokens;
        }
        if let Some(id) = workflow {
            *self.workflow.entry(id.0.clone()).or_insert(0) += tokens;
        }
        self.spent_usd += usd;
    }
}
