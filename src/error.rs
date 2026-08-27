use binder_llm::LlmError;

/// Orchestration error. Provider payloads never escape `binder-llm`.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AiError {
    #[error(transparent)]
    Llm(#[from] LlmError),

    #[error("routing failed: {message}")]
    Routing { message: String },

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
}

/// Result alias for binder-ai operations.
pub type Result<T> = std::result::Result<T, AiError>;

impl AiError {
    /// Stable kind for tracing (never includes prompt text).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Llm(err) => err.kind(),
            Self::Routing { .. } => "routing",
            Self::Prompt { .. } => "prompt",
            Self::Budget { .. } => "budget",
            Self::Tool { .. } => "tool",
            Self::Json { .. } => "json",
            Self::Conversation { .. } => "conversation",
            Self::Configuration { .. } => "configuration",
        }
    }

    pub fn routing(message: impl Into<String>) -> Self {
        Self::Routing {
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
        match self {
            Self::Llm(err) => matches!(
                err,
                LlmError::Connection { .. }
                    | LlmError::Timeout
                    | LlmError::RateLimited { .. }
                    | LlmError::ProviderError { .. }
                    | LlmError::UnsupportedFeature { .. }
            ),
            Self::Routing { .. } => true,
            _ => false,
        }
    }

    pub fn is_context_too_large(&self) -> bool {
        match self {
            Self::Llm(LlmError::InvalidRequest { message }) => {
                let m = message.to_ascii_lowercase();
                m.contains("context") || m.contains("too large") || m.contains("token")
            }
            Self::Llm(LlmError::ProviderError { status, .. }) => *status == Some(413),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_stable() {
        assert_eq!(AiError::routing("none").kind(), "routing");
        assert_eq!(AiError::json("bad", 2).kind(), "json");
    }
}
