use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::models::{ExecutionMode, ModelId};
use crate::task::TaskKind;
use crate::telemetry::CostEstimate;
use binder_llm::{FinishReason, Provider, Usage};

/// Result of an orchestration call.
#[derive(Debug, Clone, PartialEq)]
pub struct AiResponse {
    pub content: String,
    pub task: TaskKind,
    pub usage: Usage,
    pub cost: CostEstimate,
    pub latency: Duration,
    pub finish_reason: FinishReason,
    pub fallbacks: u32,
    pub tool_rounds: u32,
    pub prompt_name: String,
    pub prompt_version: u32,
    /// Observability only. Applications should not branch on provider.
    pub execution: ExecutionTrace,
    pub ai_usage: AiUsage,
}

/// What actually ran. Present so operators can debug; not part of the task API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub provider: Provider,
    pub model: String,
    pub model_id: String,
    pub policy: String,
    pub execution_mode: ExecutionMode,
}

/// Completed-inference usage. Estimated cost is not a bill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiUsage {
    pub provider: Provider,
    pub model: ModelId,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub estimated_cost: Option<f64>,
    pub execution_mode: ExecutionMode,
    pub latency: Duration,
}

impl Default for AiUsage {
    fn default() -> Self {
        Self {
            provider: Provider::Ollama,
            model: ModelId::new(Provider::Ollama, "unknown"),
            prompt_tokens: None,
            completion_tokens: None,
            estimated_cost: None,
            execution_mode: ExecutionMode::Local,
            latency: Duration::ZERO,
        }
    }
}

impl AiResponse {
    pub fn text(&self) -> &str {
        &self.content
    }
}
