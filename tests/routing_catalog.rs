use binder_ai::{
    Availability, CatalogSelector, CostClass, ExecutionMode, ExecutionPolicy, ModelCapability,
    ModelCatalog, ModelId, ModelRequirements, ModelSelector, TaskKind,
};
use binder_llm::Provider;

fn fixture() -> ModelCatalog {
    let mut catalog = ModelCatalog::from_toml(
        r#"
[[models]]
key = "local-coder"
provider = "ollama"
model = "qwen3-coder:30b"
execution = "local"
context_window = 32768
capabilities = { chat = true, coding = true, reasoning = true, tools = true, structured_output = true, streaming = true }
quality = { general = 4, coding = 5, reasoning = 4, tool_use = 4 }
cost_class = "free"

[[models]]
key = "local-reasoner"
provider = "ollama"
model = "deepseek-r1:32b"
execution = "local"
capabilities = { chat = true, coding = true, reasoning = true, planning = true, streaming = true }
quality = { general = 4, coding = 3, reasoning = 5, tool_use = 3 }
cost_class = "free"

[[models]]
key = "local-chat"
provider = "ollama"
model = "mistral-small"
execution = "local"
capabilities = { chat = true, structured_output = true, streaming = true }
quality = { general = 4, coding = 1, reasoning = 2, tool_use = 1 }
cost_class = "free"

[[models]]
key = "local-tiny-context"
provider = "ollama"
model = "tiny-context"
execution = "local"
context_window = 128
capabilities = { chat = true, coding = true, streaming = true }
quality = { general = 5, coding = 5, reasoning = 5, tool_use = 5 }
cost_class = "free"

[[models]]
key = "cloud-premium"
provider = "anthropic"
model = "claude-sonnet-4-5"
execution = "cloud"
context_window = 200000
capabilities = { chat = true, coding = true, reasoning = true, planning = true, tools = true, structured_output = true, streaming = true }
quality = { general = 5, coding = 5, reasoning = 5, tool_use = 5 }
cost_class = "high"
input_per_million = 3.0
output_per_million = 15.0

[task_defaults.summarization]
policy = "lowest_cost"
required = ["chat"]

[task_defaults.code_generation]
policy = "prefer_local"
required = ["chat", "coding"]
preferred_prefixes = ["qwen3-coder"]
min_quality = 3

[task_defaults.conversation]
policy = "balanced"
required = ["chat"]

[task_defaults.planning]
policy = "balanced"
required = ["chat", "reasoning"]
min_quality = 3
"#,
    )
    .expect("fixture toml");
    catalog.mark_providers(
        &[Provider::Ollama, Provider::Anthropic],
        Availability::Available,
    );
    catalog
}

fn select(
    catalog: &ModelCatalog,
    task: TaskKind,
    policy: ExecutionPolicy,
) -> binder_ai::Result<binder_ai::ModelSelection> {
    let mut requirements = ModelRequirements::for_task(task, catalog);
    requirements.execution_policy = policy;
    CatalogSelector::new().select(&task, &requirements, catalog)
}

#[test]
fn local_only_never_selects_cloud() {
    let catalog = fixture();
    let selection = select(
        &catalog,
        TaskKind::CodeGeneration,
        ExecutionPolicy::LocalOnly,
    )
    .unwrap();
    assert_eq!(selection.chosen.execution, ExecutionMode::Local);
    assert!(selection
        .fallbacks
        .iter()
        .all(|m| m.execution == ExecutionMode::Local));
}

#[test]
fn cloud_only_never_selects_local() {
    let catalog = fixture();
    let selection = select(
        &catalog,
        TaskKind::CodeGeneration,
        ExecutionPolicy::CloudOnly,
    )
    .unwrap();
    assert_eq!(selection.chosen.provider, Provider::Anthropic);
    assert_eq!(selection.chosen.execution, ExecutionMode::Cloud);
}

#[test]
fn lowest_cost_prefers_free_model() {
    let catalog = fixture();
    let selection = select(
        &catalog,
        TaskKind::Summarization,
        ExecutionPolicy::LowestCost,
    )
    .unwrap();
    assert_eq!(selection.chosen.pricing.cost_class, CostClass::Free);
    assert_eq!(selection.chosen.execution, ExecutionMode::Local);
}

#[test]
fn required_capabilities_filter_models() {
    let catalog = fixture();
    let selection = select(
        &catalog,
        TaskKind::CodeGeneration,
        ExecutionPolicy::LocalOnly,
    )
    .unwrap();
    assert!(selection.chosen.capabilities.coding);
    assert_ne!(selection.chosen.upstream(), "mistral-small");
}

#[test]
fn insufficient_context_window_filters_model() {
    let catalog = fixture();
    let mut requirements = ModelRequirements::for_task(TaskKind::Conversation, &catalog);
    requirements.execution_policy = ExecutionPolicy::LocalOnly;
    requirements.required_capabilities = vec![ModelCapability::Chat, ModelCapability::Coding];
    requirements.context_tokens = Some(1024);
    requirements.min_quality = None;
    let selection = CatalogSelector::new()
        .select(&TaskKind::Conversation, &requirements, &catalog)
        .unwrap();
    assert_ne!(selection.chosen.upstream(), "tiny-context");
}

#[test]
fn unavailable_model_is_ignored() {
    let mut catalog = fixture();
    let id = ModelId::new(Provider::Ollama, "qwen3-coder:30b");
    catalog.upsert({
        let mut model = catalog.get(&id).cloned().unwrap();
        model.availability = Availability::Unavailable;
        model
    });
    let selection = select(
        &catalog,
        TaskKind::CodeGeneration,
        ExecutionPolicy::LocalOnly,
    )
    .unwrap();
    assert_ne!(selection.chosen.id, id);
}

#[test]
fn explicit_cost_ceiling_is_respected() {
    let catalog = fixture();
    let mut requirements = ModelRequirements::for_task(TaskKind::CodeGeneration, &catalog);
    requirements.execution_policy = ExecutionPolicy::Balanced;
    requirements.max_cost = Some(CostClass::Free);
    let selection = CatalogSelector::new()
        .select(&TaskKind::CodeGeneration, &requirements, &catalog)
        .unwrap();
    assert_eq!(selection.chosen.pricing.cost_class, CostClass::Free);
    assert!(selection
        .fallbacks
        .iter()
        .all(|m| m.pricing.cost_class.within(CostClass::Free)));
}

#[test]
fn preferred_model_wins_when_eligible() {
    let catalog = fixture();
    let mut requirements = ModelRequirements::for_task(TaskKind::Conversation, &catalog);
    requirements.execution_policy = ExecutionPolicy::LowestCost;
    requirements.required_capabilities = vec![ModelCapability::Chat];
    requirements.min_quality = None;
    requirements.preferred_models = vec![ModelId::new(Provider::Ollama, "mistral-small")];
    let selection = CatalogSelector::new()
        .select(&TaskKind::Conversation, &requirements, &catalog)
        .unwrap();
    assert_eq!(selection.chosen.upstream(), "mistral-small");
}

#[test]
fn fallback_advances_correctly() {
    let catalog = fixture();
    let selection = select(
        &catalog,
        TaskKind::CodeGeneration,
        ExecutionPolicy::PreferLocal,
    )
    .unwrap();
    assert_eq!(selection.chosen.upstream(), "qwen3-coder:30b");
    assert!(selection
        .fallbacks
        .iter()
        .any(|m| m.upstream() == "deepseek-r1:32b"));
    assert!(selection
        .fallbacks
        .iter()
        .any(|m| m.provider == Provider::Anthropic));
}

#[test]
fn no_eligible_model_returns_error() {
    let catalog = fixture();
    let mut requirements = ModelRequirements::for_task(TaskKind::CodeGeneration, &catalog);
    requirements.execution_policy = ExecutionPolicy::LocalOnly;
    requirements.max_cost = Some(CostClass::Free);
    requirements.required_capabilities = vec![ModelCapability::Audio];
    let err = CatalogSelector::new()
        .select(&TaskKind::CodeGeneration, &requirements, &catalog)
        .unwrap_err();
    assert_eq!(err.kind(), "no_eligible_model");
}

#[test]
fn private_policy_never_includes_cloud() {
    let catalog = fixture();
    let selection = select(&catalog, TaskKind::Planning, ExecutionPolicy::Private).unwrap();
    assert_eq!(selection.chosen.execution, ExecutionMode::Local);
    assert!(selection
        .fallbacks
        .iter()
        .all(|m| m.execution == ExecutionMode::Local));
}

#[test]
fn request_builder_matches_spec() {
    let request = binder_ai::AiRequest::builder()
        .task(TaskKind::CodeGeneration)
        .policy(ExecutionPolicy::PreferLocal)
        .input("write a function")
        .build();
    let catalog = fixture();
    let req = request.requirements(&catalog);
    assert_eq!(req.execution_policy, ExecutionPolicy::PreferLocal);
    let selection = CatalogSelector::new()
        .select(&request.task, &req, &catalog)
        .unwrap();
    assert_eq!(selection.chosen.upstream(), "qwen3-coder:30b");
}
