use crate::models::capability::ModelCapability;
use crate::models::catalog::ModelCatalog;
use crate::models::descriptor::ModelId;
use crate::models::pricing::CostClass;
use crate::policy::ExecutionPolicy;
use crate::task::TaskKind;

/// Constraints the selector uses. Built from task defaults plus the request.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelRequirements {
    pub required_capabilities: Vec<ModelCapability>,
    pub execution_policy: ExecutionPolicy,
    pub max_cost: Option<CostClass>,
    pub min_quality: Option<u8>,
    pub context_tokens: Option<u64>,
    pub preferred_models: Vec<ModelId>,
    pub preferred_prefixes: Vec<String>,
    pub excluded_models: Vec<ModelId>,
    pub model_override: Option<ModelId>,
}

impl ModelRequirements {
    pub fn for_task(task: TaskKind, catalog: &ModelCatalog) -> Self {
        let defaults = catalog.task_defaults(task);
        Self {
            required_capabilities: defaults.required.clone(),
            execution_policy: defaults.policy.unwrap_or(task.default_policy()),
            max_cost: None,
            min_quality: defaults.min_quality,
            context_tokens: None,
            preferred_models: Vec::new(),
            preferred_prefixes: defaults.preferred_prefixes.clone(),
            excluded_models: Vec::new(),
            model_override: None,
        }
    }
}

impl TaskKind {
    pub fn default_policy(self) -> ExecutionPolicy {
        match self {
            Self::CodeGeneration | Self::Refactoring | Self::CodeReview | Self::Embedding => {
                ExecutionPolicy::PreferLocal
            }
            Self::Summarization | Self::Classification | Self::Extraction | Self::Moderation => {
                ExecutionPolicy::LowestCost
            }
            Self::ArchitectureReview => ExecutionPolicy::BestQuality,
            Self::Planning | Self::IssuePlanning | Self::Reasoning | Self::Conversation => {
                ExecutionPolicy::Balanced
            }
        }
    }
}
