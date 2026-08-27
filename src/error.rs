use binder_llm::LlmError;

/// Orchestration error. Provider payloads never escape `binder-llm`.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AiError {
    #[error(transparent)]
    Llm(#[from] LlmError),

    #[error("no eligible model for this task and policy")]
    NoEligibleModel,

    #[error("model unavailable: {message}")]
    ModelUnavailable { message: String },

    #[error("required capability unavailable: {message}")]
    CapabilityUnavailable { message: String },

    #[error("cost limit exceeded: {message}")]
    CostLimitExceeded { message: String },

    #[error("context too large: {message}")]
    ContextTooLarge { message: String },

    #[error("local execution required; refusing cloud fallback")]
    LocalExecutionRequired,

    #[error("provider failure: {message}")]
    ProviderFailure { message: String },

    #[error("routing failed: {message}")]
    RoutingFailure { message: String },

    #[error("prompt error: {message}")]
    Prompt { message: String },

    #[error("token budget exceeded: {message}")]
    Budget { message: String },

    #[error("tool error: {message}")]
    Tool { message: String },

    #[error("structured output failed after {attempts} attempt(s): {message}")]
    Json { message: String, attempts: u32 },

    #[error("conversation error: {message}")]
    Conversation { message: String },

    #[error("invalid configuration: {message}")]
    Configuration { message: String },

    #[error(transparent)]
    Runtime(#[from] crate::runtime::RuntimeError),
}

/// Result alias for binder-ai operations.
pub type Result<T> = std::result::Result<T, AiError>;

impl AiError {
    /// Stable kind for tracing (never includes prompt text).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Llm(err) => err.kind(),
            Self::NoEligibleModel => "no_eligible_model",
            Self::ModelUnavailable { .. } => "model_unavailable",
            Self::CapabilityUnavailable { .. } => "capability_unavailable",
            Self::CostLimitExceeded { .. } => "cost_limit_exceeded",
            Self::ContextTooLarge { .. } => "context_too_large",
            Self::LocalExecutionRequired => "local_execution_required",
            Self::ProviderFailure { .. } => "provider_failure",
            Self::RoutingFailure { .. } => "routing_failure",
            Self::Prompt { .. } => "prompt",
            Self::Budget { .. } => "budget",
            Self::Tool { .. } => "tool",
            Self::Json { .. } => "json",
            Self::Conversation { .. } => "conversation",
            Self::Configuration { .. } => "configuration",
            Self::Runtime(_) => "runtime",
        }
    }

    pub fn routing(message: impl Into<String>) -> Self {
        Self::RoutingFailure {
            message: message.into(),
        }
    }

    pub fn prompt(message: impl Into<String>) -> Self {
        Self::Prompt {
            message: message.into(),
        }
    }

    pub fn budget(message: impl Into<String>) -> Self {
        Self::Budget {
            message: message.into(),
        }
    }

    pub fn tool(message: impl Into<String>) -> Self {
        Self::Tool {
            message: message.into(),
        }
    }

    pub fn json(message: impl Into<String>, attempts: u32) -> Self {
        Self::Json {
            message: message.into(),
            attempts,
        }
    }

    pub fn conversation(message: impl Into<String>) -> Self {
        Self::Conversation {
            message: message.into(),
        }
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    pub fn is_fallback_candidate(&self) -> bool {
        if self.is_local_model_missing() {
            return true;
        }
        match self {
            Self::Llm(err) => matches!(
                err,
                LlmError::Connection { .. }
                    | LlmError::Timeout
                    | LlmError::RateLimited { .. }
                    | LlmError::ProviderError { .. }
                    | LlmError::UnsupportedFeature { .. }
            ),
            Self::RoutingFailure { .. }
            | Self::ProviderFailure { .. }
            | Self::ModelUnavailable { .. } => true,
            _ => false,
        }
    }

    /// Ollama (and similar) report a missing model as HTTP 404 / "not found".
    /// That is an availability failure, not a malformed request.
    pub fn is_local_model_missing(&self) -> bool {
        match self {
            Self::ModelUnavailable { .. } => true,
            Self::Llm(LlmError::ProviderError {
                status: Some(404), ..
            }) => true,
            Self::Llm(LlmError::InvalidRequest { message }) => looks_like_missing_model(message),
            _ => false,
        }
    }

    pub fn is_context_too_large(&self) -> bool {
        match self {
            Self::ContextTooLarge { .. } => true,
            Self::Llm(LlmError::InvalidRequest { message }) => {
                let m = message.to_ascii_lowercase();
                m.contains("context") || m.contains("too large") || m.contains("token")
            }
            Self::Llm(LlmError::ProviderError { status, .. }) => *status == Some(413),
            _ => false,
        }
    }
}

fn looks_like_missing_model(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    let missing = m.contains("not found")
        || m.contains("does not exist")
        || m.contains("no such model")
        || m.contains("unknown model");
    missing && (m.contains("model") || m.contains("ollama"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_stable() {
        assert_eq!(AiError::routing("none").kind(), "routing_failure");
        assert_eq!(AiError::json("bad", 2).kind(), "json");
        assert_eq!(AiError::NoEligibleModel.kind(), "no_eligible_model");
    }

    #[test]
    fn model_not_found_is_fallback() {
        let err = AiError::Llm(LlmError::invalid("model 'deepseek-r1:32b' not found"));
        assert!(err.is_local_model_missing());
        assert!(err.is_fallback_candidate());
        let schema = AiError::Llm(LlmError::invalid("invalid json schema"));
        assert!(!schema.is_local_model_missing());
    }
}
