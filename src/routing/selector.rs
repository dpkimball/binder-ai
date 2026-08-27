use crate::error::{AiError, Result};
use crate::models::{
    Availability, ExecutionMode, ModelCatalog, ModelDescriptor, ModelId, ModelRequirements,
};
use crate::policy::ExecutionPolicy;
use crate::task::TaskKind;
use binder_llm::Provider;

/// Concrete provider+upstream name passed to `binder-llm`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTarget {
    pub provider: Provider,
    pub model: String,
    pub id: ModelId,
}

impl From<&ModelDescriptor> for ModelTarget {
    fn from(value: &ModelDescriptor) -> Self {
        Self {
            provider: value.provider,
            model: value.upstream().to_string(),
            id: value.id.clone(),
        }
    }
}

/// Ordered attempts after filtering and scoring.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSelection {
    pub chosen: ModelDescriptor,
    pub fallbacks: Vec<ModelDescriptor>,
}

impl ModelSelection {
    pub fn targets(&self) -> Vec<ModelTarget> {
        std::iter::once(&self.chosen)
            .chain(self.fallbacks.iter())
            .map(ModelTarget::from)
            .collect()
    }
}

/// Deterministic catalog selector. No per-model name branches.
pub trait ModelSelector {
    fn select(
        &self,
        task: &TaskKind,
        requirements: &ModelRequirements,
        catalog: &ModelCatalog,
    ) -> Result<ModelSelection>;
}

#[derive(Debug, Clone, Default)]
pub struct CatalogSelector {
    pub capability_weight: f64,
    pub quality_weight: f64,
    pub cost_weight: f64,
    pub local_weight: f64,
    pub latency_weight: f64,
}

impl CatalogSelector {
    pub fn new() -> Self {
        Self {
            capability_weight: 0.40,
            quality_weight: 0.30,
            cost_weight: 0.15,
            local_weight: 0.10,
            latency_weight: 0.05,
        }
    }
}

impl ModelSelector for CatalogSelector {
    fn select(
        &self,
        task: &TaskKind,
        requirements: &ModelRequirements,
        catalog: &ModelCatalog,
    ) -> Result<ModelSelection> {
        if let Some(id) = &requirements.model_override {
            let Some(model) = catalog.get(id).cloned() else {
                return Err(AiError::ModelUnavailable {
                    message: id.to_string(),
                });
            };
            if requirements.execution_policy.is_private() && model.execution == ExecutionMode::Cloud
            {
                return Err(AiError::LocalExecutionRequired);
            }
            if let Some(reason) = reject(&model, *task, requirements, true) {
                tracing::debug!(
                    model_id = %model.id,
                    reason,
                    "routing.reject"
                );
                return Err(AiError::NoEligibleModel);
            }
            let fallbacks: Vec<_> = rank(self, *task, requirements, catalog)
                .into_iter()
                .filter(|m| m.id != model.id)
                .collect();
            tracing::info!(
                selected_model = %model.id,
                fallback_count = fallbacks.len() as u32,
                "routing.selected"
            );
            return Ok(ModelSelection {
                chosen: model,
                fallbacks,
            });
        }

        let ranked = rank(self, *task, requirements, catalog);
        let Some(chosen) = ranked.first().cloned() else {
            tracing::info!(
                available = catalog.ids_with(Availability::Available).len() as u32,
                "routing.empty"
            );
            return Err(AiError::NoEligibleModel);
        };
        let fallbacks = ranked.into_iter().skip(1).collect::<Vec<_>>();
        tracing::info!(
            selected_model = %chosen.id,
            fallback_count = fallbacks.len() as u32,
            "routing.selected"
        );
        Ok(ModelSelection { chosen, fallbacks })
    }
}

fn rank(
    selector: &CatalogSelector,
    task: TaskKind,
    requirements: &ModelRequirements,
    catalog: &ModelCatalog,
) -> Vec<ModelDescriptor> {
    let mut eligible: Vec<(f64, ModelDescriptor)> = Vec::new();
    for model in catalog.models() {
        if let Some(reason) = reject(model, task, requirements, false) {
            tracing::debug!(model_id = %model.id, reason, "routing.reject");
            continue;
        }
        eligible.push((selector.score(model, task, requirements), model.clone()));
    }

    match requirements.execution_policy {
        ExecutionPolicy::LowestCost | ExecutionPolicy::FreeFirst | ExecutionPolicy::Fastest => {
            eligible.sort_by(|a, b| {
                cost_key(&a.1)
                    .cmp(&cost_key(&b.1))
                    .then_with(|| local_ord(&a.1).cmp(&local_ord(&b.1)))
                    .then_with(|| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal))
            });
        }
        ExecutionPolicy::BestQuality => {
            eligible.sort_by(|a, b| {
                b.1.quality
                    .for_task(task)
                    .cmp(&a.1.quality.for_task(task))
                    .then_with(|| b.1.quality.general.cmp(&a.1.quality.general))
                    .then_with(|| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal))
            });
        }
        ExecutionPolicy::PreferLocal | ExecutionPolicy::Balanced => {
            eligible.sort_by(|a, b| {
                local_ord(&a.1)
                    .cmp(&local_ord(&b.1))
                    .then_with(|| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal))
            });
        }
        ExecutionPolicy::LocalOnly | ExecutionPolicy::Private | ExecutionPolicy::CloudOnly => {
            eligible.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    if matches!(
        requirements.execution_policy,
        ExecutionPolicy::PreferLocal | ExecutionPolicy::FreeFirst
    ) {
        let locals: Vec<_> = eligible
            .iter()
            .filter(|(_, m)| m.execution == ExecutionMode::Local)
            .cloned()
            .collect();
        if !locals.is_empty() {
            let mut rest: Vec<_> = eligible
                .into_iter()
                .filter(|(_, m)| m.execution != ExecutionMode::Local)
                .collect();
            let mut out = locals;
            out.append(&mut rest);
            eligible = out;
        }
    }

    eligible.into_iter().map(|(_, m)| m).collect()
}

fn local_ord(model: &ModelDescriptor) -> u8 {
    u8::from(model.execution != ExecutionMode::Local)
}

fn cost_key(model: &ModelDescriptor) -> u8 {
    model.pricing.cost_class.rank()
}

fn reject(
    model: &ModelDescriptor,
    task: TaskKind,
    req: &ModelRequirements,
    allow_unavailable: bool,
) -> Option<&'static str> {
    if req.excluded_models.contains(&model.id) {
        return Some("excluded");
    }
    if !allow_unavailable && !model.availability.is_selectable() {
        return Some("unavailable");
    }
    if model.execution == ExecutionMode::Local && !req.execution_policy.allows_local() {
        return Some("execution_policy");
    }
    if model.execution == ExecutionMode::Cloud && !req.execution_policy.allows_cloud() {
        return Some("execution_policy");
    }
    if model.capabilities.required_fit(&req.required_capabilities) < 1.0 {
        return Some("capabilities");
    }
    if let Some(needed) = req.context_tokens {
        match model.context_window {
            Some(window) if window < needed => return Some("context_window"),
            None => {}
            Some(_) => {}
        }
    }
    if let Some(max) = req.max_cost {
        if !model.pricing.cost_class.within(max) {
            return Some("cost");
        }
    }
    if let Some(min) = req.min_quality {
        if model.quality.for_task(task) < min {
            return Some("quality");
        }
    }
    None
}

impl CatalogSelector {
    fn score(&self, model: &ModelDescriptor, task: TaskKind, req: &ModelRequirements) -> f64 {
        let cap = model.capabilities.required_fit(&req.required_capabilities);
        let quality = f64::from(model.quality.for_task(task)) / 5.0;
        let cost = 1.0 - (f64::from(model.pricing.cost_class.rank()) / 5.0);
        let local = if model.execution == ExecutionMode::Local {
            1.0
        } else {
            0.0
        };
        let latency = if model.execution == ExecutionMode::Local {
            1.0
        } else {
            0.4
        };
        let mut score = cap * self.capability_weight
            + quality * self.quality_weight
            + cost * self.cost_weight
            + local * self.local_weight
            + latency * self.latency_weight;
        if req.preferred_models.contains(&model.id) {
            score += 0.25;
        }
        if req
            .preferred_prefixes
            .iter()
            .any(|prefix| model.matches_prefix(prefix))
        {
            score += 0.15;
        }
        score
    }
}

impl Default for ModelTarget {
    fn default() -> Self {
        Self {
            provider: Provider::Ollama,
            model: String::new(),
            id: ModelId::new(Provider::Ollama, "unknown"),
        }
    }
}
