use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::ContextSource;
use crate::conversation::ConversationId;
use crate::policy::RoutingPolicy;
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
    pub policy: Option<RoutingPolicy>,
    #[serde(default)]
    pub json_mode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_provider: Option<Provider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_model: Option<String>,
    #[serde(default)]
    pub use_tools: bool,
}

impl AiRequest {
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
        }
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

    pub fn with_policy(mut self, policy: RoutingPolicy) -> Self {
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
}
