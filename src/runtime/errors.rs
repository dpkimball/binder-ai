use thiserror::Error;

/// Runtime-specific failure. Independent of [`crate::AiError`] routing/LLM kinds.
#[derive(Debug, Clone, Error)]
pub enum RuntimeError {
    #[error("file not found: {path}")]
    NotFound { path: String },

    #[error("path escapes the runtime workspace: {path}")]
    PathEscape { path: String },

    #[error("io error: {message}")]
    Io { message: String },

    #[error("search error: {message}")]
    Search { message: String },

    #[error("command failed to start: {message}")]
    Command { message: String },

    #[error("git error: {message}")]
    Git { message: String },

    #[error("unsupported runtime operation: {message}")]
    Unsupported { message: String },
}

impl RuntimeError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::PathEscape { .. } => "path_escape",
            Self::Io { .. } => "io",
            Self::Search { .. } => "search",
            Self::Command { .. } => "command",
            Self::Git { .. } => "git",
            Self::Unsupported { .. } => "unsupported",
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::Io {
            message: message.into(),
        }
    }

    pub fn git(message: impl Into<String>) -> Self {
        Self::Git {
            message: message.into(),
        }
    }

    pub fn command(message: impl Into<String>) -> Self {
        Self::Command {
            message: message.into(),
        }
    }

    pub fn search(message: impl Into<String>) -> Self {
        Self::Search {
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for RuntimeError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound {
                path: err.to_string(),
            }
        } else {
            Self::io(err.to_string())
        }
    }
}

/// Result alias for runtime operations.
pub type Result<T> = std::result::Result<T, RuntimeError>;
