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

pub fn trace_start(task: TaskKind, policy: &str, model: &str) {
    tracing::info!(
        task = %task,
        policy = policy,
        model = model,
        "ai.request.start"
    );
}

#[allow(clippy::too_many_arguments)]
pub fn trace_ok(
    task: TaskKind,
    provider: Provider,
    model: &str,
    latency_ms: u128,
    fallbacks: u32,
    tool_rounds: u32,
    usage: Usage,
    cost_usd: f64,
) {
    tracing::info!(
        task = %task,
        provider = %provider,
        model = model,
        latency_ms = latency_ms as u64,
        fallbacks,
        tool_rounds,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        estimated_cost_usd = cost_usd,
        "ai.request.ok"
    );
}

pub fn trace_err(task: TaskKind, error_kind: &str) {
    tracing::warn!(task = %task, error_kind, "ai.request.err");
}
