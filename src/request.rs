use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::ContextSource;
use crate::conversation::ConversationId;
use crate::models::{CostClass, ModelCatalog, ModelId, ModelRequirements};
use crate::policy::ExecutionPolicy;
use crate::prompt::PromptRef;
use crate::session::SessionId;
use crate::task::{Task, TaskKind};
use binder_llm::Provider;

/// Orchestration request. Applications describe intent, not wire format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiRequest {
    pub task: TaskKind,
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<ConversationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<PromptRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<ContextSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<ExecutionPolicy>,
    #[serde(default)]
    pub json_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_provider: Option<Provider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer_model: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<ModelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost: Option<CostClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    #[serde(default)]
    pub use_tools: bool,
    /// Image bytes for the question. Provider encoding stays in binder-llm.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<AiImage>,
}

/// One image attached to an [`AiRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiImage {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl AiRequest {
    pub fn builder() -> AiRequestBuilder {
        AiRequestBuilder::default()
    }

    pub fn new(task: TaskKind, input: impl Into<String>) -> Self {
        Self {
            json_mode: task.wants_json(),
            use_tools: task.wants_tools(),
            task,
            input: input.into(),
            conversation_id: None,
            session_id: None,
            prompt: None,
            context: Vec::new(),
            policy: None,
            json_schema: None,
            override_provider: None,
            override_model: None,
            prefer_model: None,
            model_override: None,
            max_cost: None,
            context_tokens: None,
            images: Vec::new(),
        }
    }

    pub fn with_image(mut self, media_type: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.images.push(AiImage {
            media_type: media_type.into(),
            bytes,
        });
        self
    }

    pub fn from_task(task: Task) -> Self {
        let kind = task.kind();
        let mut request = Self::new(kind, task.user_input());
        match task {
            Task::ArchitectureReview {
                repository,
                pull_request,
            } => {
                request.context.push(ContextSource::Repository(repository));
                if let Some(pr) = pull_request {
                    request
                        .context
                        .push(ContextSource::Issue(crate::context::IssueContext {
                            id: pr,
                            title: None,
                            body: None,
                        }));
                }
            }
            Task::IssuePlanning { issue } => {
                request.context.push(ContextSource::Issue(issue));
            }
            _ => {}
        }
        request
    }

    pub fn with_conversation(mut self, id: ConversationId) -> Self {
        self.conversation_id = Some(id);
        self
    }

    pub fn with_session(mut self, id: SessionId) -> Self {
        self.session_id = Some(id);
        self
    }

    pub fn with_prompt(mut self, prompt: PromptRef) -> Self {
        self.prompt = Some(prompt);
        self
    }

    pub fn with_context(mut self, source: ContextSource) -> Self {
        self.context.push(source);
        self
    }

    pub fn with_policy(mut self, policy: ExecutionPolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    pub fn with_json_mode(mut self, json_mode: bool) -> Self {
        self.json_mode = json_mode;
        self
    }

    pub fn with_override_provider(mut self, provider: Provider) -> Self {
        self.override_provider = Some(provider);
        self
    }

    pub fn requirements(&self, catalog: &ModelCatalog) -> ModelRequirements {
        let mut req = ModelRequirements::for_task(self.task, catalog);
        if let Some(policy) = self.policy {
            req.execution_policy = policy;
        }
        req.max_cost = self.max_cost.or(req.max_cost);
        req.context_tokens = self.context_tokens;
        if let Some(id) = &self.prefer_model {
            req.preferred_models.push(id.clone());
        }
        if let Some(id) = &self.model_override {
            req.model_override = Some(id.clone());
        } else if let (Some(provider), Some(model)) =
            (self.override_provider, self.override_model.as_deref())
        {
            req.model_override = Some(ModelId::new(provider, model));
        } else if let Some(model) = self.override_model.as_deref() {
            if let Ok(id) = model.parse() {
                req.model_override = Some(id);
            }
        } else if let Some(provider) = self.override_provider {
            req.excluded_models.extend(
                catalog
                    .models()
                    .iter()
                    .filter(|m| m.provider != provider)
                    .map(|m| m.id.clone()),
            );
        }
        req
    }
}

impl From<Task> for AiRequest {
    fn from(task: Task) -> Self {
        Self::from_task(task)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AiRequestBuilder {
    task: Option<TaskKind>,
    input: String,
    policy: Option<ExecutionPolicy>,
    max_cost: Option<CostClass>,
    prefer_model: Option<ModelId>,
    model_override: Option<ModelId>,
}

impl AiRequestBuilder {
    pub fn task(mut self, task: TaskKind) -> Self {
        self.task = Some(task);
        self
    }

    pub fn input(mut self, input: impl Into<String>) -> Self {
        self.input = input.into();
        self
    }

    pub fn policy(mut self, policy: ExecutionPolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    pub fn max_cost(mut self, max: CostClass) -> Self {
        self.max_cost = Some(max);
        self
    }

    pub fn prefer_model(mut self, id: impl Into<ModelId>) -> Self {
        self.prefer_model = Some(id.into());
        self
    }

    pub fn model_override(mut self, id: impl Into<ModelId>) -> Self {
        self.model_override = Some(id.into());
        self
    }

    pub fn build(self) -> AiRequest {
        let task = self.task.unwrap_or(TaskKind::Conversation);
        let mut request = AiRequest::new(task, self.input);
        request.policy = self.policy;
        request.max_cost = self.max_cost;
        request.prefer_model = self.prefer_model;
        request.model_override = self.model_override;
        request
    }
}
