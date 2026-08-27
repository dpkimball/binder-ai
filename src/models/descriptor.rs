use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{AiError, Result};
use crate::models::availability::Availability;
use crate::models::capability::ModelCapabilities;
use crate::models::pricing::Pricing;
use crate::models::quality::QualityProfile;
use binder_llm::Provider;

/// Provider-neutral model identity: `{provider}:{upstream}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelId(String);

impl ModelId {
    pub fn new(provider: Provider, upstream: impl AsRef<str>) -> Self {
        Self(format!("{}:{}", provider.as_str(), upstream.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn provider(&self) -> Result<Provider> {
        let (head, _) = self.split()?;
        head.parse()
            .map_err(|_| AiError::routing(format!("unknown provider in model id '{}'", self.0)))
    }

    pub fn upstream(&self) -> &str {
        self.0
            .split_once(':')
            .map(|(_, rest)| rest)
            .unwrap_or(&self.0)
    }

    fn split(&self) -> Result<(&str, &str)> {
        self.0.split_once(':').ok_or_else(|| {
            AiError::configuration(format!(
                "model id '{}' is missing a provider prefix",
                self.0
            ))
        })
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ModelId {
    type Err = AiError;

    fn from_str(s: &str) -> Result<Self> {
        let (head, rest) = s.split_once(':').ok_or_else(|| {
            AiError::configuration(format!(
                "model id '{s}' must be '{{provider}}:{{upstream}}'"
            ))
        })?;
        if rest.is_empty() {
            return Err(AiError::configuration(format!(
                "model id '{s}' is missing an upstream name"
            )));
        }
        let provider: Provider = head.parse().map_err(|_| {
            AiError::configuration(format!("unknown provider '{head}' in model id '{s}'"))
        })?;
        Ok(Self::new(provider, rest))
    }
}

impl From<&str> for ModelId {
    fn from(value: &str) -> Self {
        value
            .parse()
            .unwrap_or_else(|_| ModelId::new(Provider::Ollama, value))
    }
}

impl From<String> for ModelId {
    fn from(value: String) -> Self {
        ModelId::from(value.as_str())
    }
}

/// Where the model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Local,
    Cloud,
}

impl ExecutionMode {
    pub fn from_provider(provider: Provider) -> Self {
        if provider.is_local() {
            Self::Local
        } else {
            Self::Cloud
        }
    }
}

/// Provider-neutral catalog entry. Capabilities are Binder metadata, not vendor JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelDescriptor {
    pub id: ModelId,
    pub provider: Provider,
    pub display_name: String,
    pub capabilities: ModelCapabilities,
    pub execution: ExecutionMode,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub pricing: Pricing,
    pub quality: QualityProfile,
    pub availability: Availability,
    pub metadata: HashMap<String, String>,
}

impl ModelDescriptor {
    pub fn upstream(&self) -> &str {
        self.id.upstream()
    }

    pub fn matches_prefix(&self, prefix: &str) -> bool {
        let name = self.upstream();
        name == prefix || name.starts_with(&format!("{prefix}:")) || name.starts_with(prefix)
    }
}
