use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{AiError, Result};
use crate::models::capability::{ModelCapabilities, ModelCapability};
use crate::models::descriptor::{ExecutionMode, ModelDescriptor, ModelId};
use crate::models::pricing::{CostClass, Pricing};
use crate::models::quality::QualityProfile;
use crate::policy::ExecutionPolicy;
use binder_llm::Provider;

const BUILTIN_TOML: &str = include_str!("builtin.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogFile {
    #[serde(default)]
    pub profiles: Vec<PrefixProfile>,
    #[serde(default)]
    pub models: Vec<ModelConfig>,
    #[serde(default)]
    pub task_defaults: HashMap<String, TaskDefaultConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PrefixProfile {
    pub provider: Provider,
    pub model_prefix: String,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub quality: QualityProfile,
    #[serde(default)]
    pub cost_class: CostClass,
    #[serde(default)]
    pub context_window: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub key: String,
    pub provider: Provider,
    pub model: String,
    #[serde(default)]
    pub execution: Option<ExecutionMode>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub quality: QualityProfile,
    #[serde(default)]
    pub cost_class: CostClass,
    #[serde(default)]
    pub input_per_million: Option<f64>,
    #[serde(default)]
    pub output_per_million: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TaskDefaultConfig {
    #[serde(default)]
    pub policy: Option<ExecutionPolicy>,
    #[serde(default)]
    pub required: Vec<ModelCapability>,
    #[serde(default)]
    pub min_quality: Option<u8>,
    #[serde(default)]
    pub preferred_prefixes: Vec<String>,
}

impl CatalogFile {
    pub fn builtin() -> Result<Self> {
        Self::from_toml(BUILTIN_TOML)
    }

    pub fn from_toml(text: &str) -> Result<Self> {
        toml::from_str(text).map_err(|e| AiError::configuration(format!("catalog toml: {e}")))
    }

    pub fn merge(&mut self, override_file: CatalogFile) {
        for profile in override_file.profiles {
            self.profiles.retain(|p| {
                !(p.provider == profile.provider && p.model_prefix == profile.model_prefix)
            });
            self.profiles.push(profile);
        }
        for model in override_file.models {
            self.models.retain(|m| m.key != model.key);
            self.models.push(model);
        }
        for (k, v) in override_file.task_defaults {
            self.task_defaults.insert(k, v);
        }
    }
}

impl ModelConfig {
    pub fn into_descriptor(self) -> ModelDescriptor {
        let execution = self
            .execution
            .unwrap_or_else(|| ExecutionMode::from_provider(self.provider));
        ModelDescriptor {
            id: ModelId::new(self.provider, &self.model),
            provider: self.provider,
            display_name: self.key.clone(),
            capabilities: self.capabilities,
            execution,
            context_window: self.context_window,
            max_output_tokens: self.max_output_tokens,
            pricing: Pricing {
                cost_class: self.cost_class,
                input_per_million: self.input_per_million,
                output_per_million: self.output_per_million,
            },
            quality: self.quality.clamp(),
            availability: crate::models::Availability::Unknown,
            metadata: HashMap::from([("key".into(), self.key)]),
        }
    }
}

impl PrefixProfile {
    pub fn applies_to(&self, provider: Provider, upstream: &str) -> bool {
        provider == self.provider
            && (upstream == self.model_prefix
                || upstream.starts_with(&format!("{}:", self.model_prefix))
                || upstream.starts_with(&self.model_prefix))
    }

    pub fn descriptor_for(&self, upstream: &str) -> ModelDescriptor {
        ModelDescriptor {
            id: ModelId::new(self.provider, upstream),
            provider: self.provider,
            display_name: upstream.to_string(),
            capabilities: self.capabilities,
            execution: ExecutionMode::from_provider(self.provider),
            context_window: self.context_window,
            max_output_tokens: None,
            pricing: Pricing {
                cost_class: self.cost_class,
                input_per_million: if self.provider.is_local() {
                    Some(0.0)
                } else {
                    None
                },
                output_per_million: if self.provider.is_local() {
                    Some(0.0)
                } else {
                    None
                },
            },
            quality: self.quality.clamp(),
            availability: crate::models::Availability::Available,
            metadata: HashMap::from([("discovered".into(), "true".into())]),
        }
    }
}
