use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::task::TaskKind;
use binder_llm::{Provider, Usage};

/// Estimated USD for a call. Explicitly not billing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct CostEstimate {
    pub usd: f64,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub provider: Option<ProviderCostKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderCostKind {
    Ollama,
    Openai,
    Anthropic,
    Xai,
    Gemini,
}

impl From<Provider> for ProviderCostKind {
    fn from(provider: Provider) -> Self {
        match provider {
            Provider::Ollama => Self::Ollama,
            Provider::Openai => Self::Openai,
            Provider::Anthropic => Self::Anthropic,
            Provider::Xai => Self::Xai,
            Provider::Gemini => Self::Gemini,
        }
    }
}

/// Rough per-million-token rates. Operators may replace this; it is not a price sheet.
pub fn estimate_cost(provider: Provider, usage: Usage) -> CostEstimate {
    estimate_cost_for(provider, usage, None)
}

/// Prefer catalog rates when present; unknown pricing must not break routing.
pub fn estimate_cost_for(
    provider: Provider,
    usage: Usage,
    pricing: Option<crate::models::Pricing>,
) -> CostEstimate {
    if let Some(pricing) = pricing {
        if let Some(usd) = pricing.estimate_usd(usage.prompt_tokens, usage.completion_tokens) {
            return CostEstimate {
                usd,
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                provider: Some(provider.into()),
            };
        }
    }
    let (in_rate, out_rate) = match provider {
        Provider::Ollama => (0.0, 0.0),
        Provider::Openai => (2.0, 8.0),
        Provider::Anthropic => (3.0, 15.0),
        Provider::Xai => (2.0, 8.0),
        Provider::Gemini => (0.25, 1.0),
    };
    let usd = (usage.prompt_tokens as f64 / 1_000_000.0) * in_rate
        + (usage.completion_tokens as f64 / 1_000_000.0) * out_rate;
    CostEstimate {
        usd,
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        provider: Some(provider.into()),
    }
}

/// Counters for a process. No telemetry backend.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiMetrics {
    pub requests: u64,
    pub failures: u64,
    pub fallbacks: u64,
    pub tool_calls: u64,
    pub latency: Duration,
    pub estimated_cost_usd: f64,
}

impl AiMetrics {
    pub fn record_success(
        &mut self,
        latency: Duration,
        fallbacks: u32,
        tool_calls: u32,
        cost: CostEstimate,
    ) {
        self.requests += 1;
        self.fallbacks += u64::from(fallbacks);
        self.tool_calls += u64::from(tool_calls);
        self.latency += latency;
        self.estimated_cost_usd += cost.usd;
    }

    pub fn record_failure(&mut self) {
        self.requests += 1;
        self.failures += 1;
    }
}

pub fn trace_start(
    task: TaskKind,
    policy: &str,
    selected_model: &str,
    provider: Provider,
    execution_mode: crate::models::ExecutionMode,
) {
    tracing::info!(
        task = %task,
        policy = policy,
        selected_model = selected_model,
        provider = %provider,
        execution_mode = ?execution_mode,
        "ai.request.start"
    );
}

#[allow(clippy::too_many_arguments)]
pub fn trace_ok(
    task: TaskKind,
    provider: Provider,
    selected_model: &str,
    execution_mode: crate::models::ExecutionMode,
    policy: &str,
    latency_ms: u128,
    fallback_count: u32,
    tool_rounds: u32,
    usage: Usage,
    estimated_cost: f64,
) {
    tracing::info!(
        task = %task,
        selected_model = selected_model,
        provider = %provider,
        execution_mode = ?execution_mode,
        policy = policy,
        fallback_count,
        estimated_cost,
        latency_ms = latency_ms as u64,
        tokens = usage.total_tokens,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        tool_rounds,
        "ai.request.ok"
    );
}

pub fn trace_err(task: TaskKind, error_kind: &str) {
    tracing::warn!(task = %task, error_kind, "ai.request.err");
}
