use std::time::Duration;

use serde::{Deserialize, Serialize};

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
}

/// What actually ran. Present so operators can debug; not part of the task API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub provider: Provider,
    pub model: String,
    pub policy: String,
}

impl AiResponse {
    pub fn text(&self) -> &str {
        &self.content
    }
}
