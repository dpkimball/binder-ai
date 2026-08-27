use crate::error::Result;
use crate::models::availability::Availability;
use crate::models::capability::ModelCapabilities;
use crate::models::config::{CatalogFile, PrefixProfile, TaskDefaultConfig};
use crate::models::descriptor::{ExecutionMode, ModelDescriptor, ModelId};
use crate::models::pricing::{CostClass, Pricing};
use crate::models::quality::QualityProfile;
use crate::task::TaskKind;
use binder_llm::{ModelInfo, Provider};

/// Effective catalog: builtin metadata + discovery + user overrides.
#[derive(Debug, Clone)]
pub struct ModelCatalog {
    models: Vec<ModelDescriptor>,
    profiles: Vec<PrefixProfile>,
    task_defaults: std::collections::HashMap<String, TaskDefaultConfig>,
}

impl Default for ModelCatalog {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ModelCatalog {
    pub fn builtin() -> Self {
        Self::from_file(CatalogFile::builtin().expect("builtin catalog toml"))
    }

    pub fn from_toml(text: &str) -> Result<Self> {
        Ok(Self::from_file(CatalogFile::from_toml(text)?))
    }

    pub fn from_file(file: CatalogFile) -> Self {
        let models = file
            .models
            .into_iter()
            .map(|m| m.into_descriptor())
            .collect();
        Self {
            models,
            profiles: file.profiles,
            task_defaults: file.task_defaults,
        }
    }

    pub fn merge_toml(&mut self, text: &str) -> Result<()> {
        let mut base = CatalogFile {
            profiles: self.profiles.clone(),
            models: Vec::new(),
            task_defaults: self.task_defaults.clone(),
        };
        // Keep existing descriptors; overlay file then re-apply models by id.
        let overlay = CatalogFile::from_toml(text)?;
        let overlay_models = overlay.models.clone();
        base.merge(overlay);
        self.profiles = base.profiles;
        self.task_defaults = base.task_defaults;
        for cfg in overlay_models {
            let desc = cfg.into_descriptor();
            self.upsert(desc);
        }
        Ok(())
    }

    pub fn models(&self) -> &[ModelDescriptor] {
        &self.models
    }

    pub fn get(&self, id: &ModelId) -> Option<&ModelDescriptor> {
        self.models.iter().find(|m| &m.id == id)
    }

    pub fn upsert(&mut self, descriptor: ModelDescriptor) {
        if let Some(existing) = self.models.iter_mut().find(|m| m.id == descriptor.id) {
            *existing = descriptor;
        } else {
            self.models.push(descriptor);
        }
    }

    pub fn task_defaults(&self, task: TaskKind) -> TaskDefaultConfig {
        self.task_defaults
            .get(task.as_str())
            .cloned()
            .unwrap_or_default()
    }

    /// Tests / injected clients: every model for these providers is selectable.
    pub fn mark_providers(&mut self, providers: &[Provider], status: Availability) {
        for model in &mut self.models {
            if providers.contains(&model.provider) {
                model.availability = status;
            }
        }
    }

    pub fn mark_all_except(&mut self, providers: &[Provider], status: Availability) {
        for model in &mut self.models {
            if !providers.contains(&model.provider) {
                model.availability = status;
            }
        }
    }

    /// Merge Ollama `/api/tags` (via `binder-llm` `list_models`). No HTTP here.
    pub fn ingest_discovered(&mut self, provider: Provider, infos: &[ModelInfo]) {
        let names: Vec<String> = infos
            .iter()
            .filter(|info| info.provider == provider)
            .map(|info| info.id.clone())
            .collect();
        self.ingest_names(provider, &names);
    }

    pub fn ingest_names(&mut self, provider: Provider, names: &[String]) {
        for name in names {
            let id = ModelId::new(provider, name);
            if let Some(existing) = self.models.iter_mut().find(|m| m.id == id) {
                existing.availability = Availability::Available;
                continue;
            }
            let desc = self
                .profiles
                .iter()
                .find(|p| p.applies_to(provider, name))
                .map(|p| p.descriptor_for(name))
                .unwrap_or_else(|| conservative_discovered(provider, name));
            self.models.push(desc);
        }
        if provider.is_local() {
            for model in &mut self.models {
                if model.provider == provider
                    && !names.iter().any(|n| model.id.upstream() == n)
                    && !model.metadata.contains_key("discovered")
                {
                    // Configured but not installed.
                    if matches!(
                        model.availability,
                        Availability::Unknown | Availability::Available
                    ) {
                        model.availability = Availability::Unavailable;
                    }
                }
            }
            for name in names {
                if let Some(model) = self
                    .models
                    .iter_mut()
                    .find(|m| m.provider == provider && m.id.upstream() == name)
                {
                    model.availability = Availability::Available;
                }
            }
        }
    }

    pub fn profile_for(&self, provider: Provider, upstream: &str) -> Option<&PrefixProfile> {
        self.profiles
            .iter()
            .find(|p| p.applies_to(provider, upstream))
    }
}

fn conservative_discovered(provider: Provider, upstream: &str) -> ModelDescriptor {
    ModelDescriptor {
        id: ModelId::new(provider, upstream),
        provider,
        display_name: upstream.to_string(),
        capabilities: ModelCapabilities {
            chat: true,
            streaming: true,
            ..ModelCapabilities::default()
        },
        execution: ExecutionMode::from_provider(provider),
        context_window: None,
        max_output_tokens: None,
        pricing: if provider.is_local() {
            Pricing::free()
        } else {
            Pricing {
                cost_class: CostClass::Medium,
                input_per_million: None,
                output_per_million: None,
            }
        },
        quality: QualityProfile {
            general: 2,
            coding: 2,
            reasoning: 2,
            tool_use: 2,
        },
        availability: Availability::Available,
        metadata: std::collections::HashMap::from([
            ("discovered".into(), "true".into()),
            ("unprofiled".into(), "true".into()),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_parses() {
        let catalog = ModelCatalog::builtin();
        assert!(catalog
            .get(&ModelId::new(Provider::Ollama, "qwen3-coder:30b"))
            .is_some());
        assert!(catalog
            .get(&ModelId::new(Provider::Anthropic, "claude-sonnet-4-5"))
            .is_some());
    }

    #[test]
    fn ingest_names_applies_prefix_profile() {
        let mut catalog = ModelCatalog::builtin();
        catalog.ingest_names(
            Provider::Ollama,
            &["qwen3-coder:30b".into(), "mystery-local".into()],
        );
        let qwen = catalog
            .get(&ModelId::new(Provider::Ollama, "qwen3-coder:30b"))
            .unwrap();
        assert_eq!(qwen.availability, Availability::Available);
        assert!(qwen.capabilities.coding);
        let mystery = catalog
            .get(&ModelId::new(Provider::Ollama, "mystery-local"))
            .unwrap();
        assert!(mystery.capabilities.chat);
        assert_eq!(mystery.pricing.cost_class, CostClass::Free);
        let missing = catalog
            .get(&ModelId::new(Provider::Ollama, "mistral-small"))
            .unwrap();
        assert_eq!(missing.availability, Availability::Unavailable);
    }

    #[test]
    fn user_toml_overrides_builtin() {
        let mut catalog = ModelCatalog::builtin();
        catalog
            .merge_toml(
                r#"
[[models]]
key = "qwen-coder"
provider = "ollama"
model = "qwen3-coder:30b"
quality = { general = 1, coding = 1, reasoning = 1, tool_use = 1 }
cost_class = "free"
"#,
            )
            .unwrap();
        let qwen = catalog
            .get(&ModelId::new(Provider::Ollama, "qwen3-coder:30b"))
            .unwrap();
        assert_eq!(qwen.quality.coding, 1);
    }
}
