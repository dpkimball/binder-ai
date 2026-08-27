use crate::policy::RoutingPolicy;
use crate::task::TaskKind;
use binder_llm::Provider;

/// A concrete provider+model the router selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTarget {
    pub provider: Provider,
    pub model: String,
}

/// Opinionated catalog. Not live provider discovery — that stays in binder-llm.
#[derive(Debug, Clone)]
pub struct ModelCatalog {
    pub ollama_chat: String,
    pub ollama_embed: String,
    pub openai_chat: String,
    pub anthropic_chat: String,
    pub xai_chat: String,
    pub gemini_chat: String,
    pub gemini_long: String,
}

impl Default for ModelCatalog {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ModelCatalog {
    pub fn builtin() -> Self {
        Self {
            ollama_chat: "qwen3-coder:30b".into(),
            ollama_embed: "nomic-embed-text".into(),
            openai_chat: "gpt-4.1".into(),
            anthropic_chat: "claude-sonnet-4-5".into(),
            xai_chat: "grok-3".into(),
            gemini_chat: "gemini-2.5-flash".into(),
            gemini_long: "gemini-2.5-pro".into(),
        }
    }

    pub fn default_model(&self, provider: Provider, task: TaskKind) -> &str {
        if task.is_embedding() {
            return match provider {
                Provider::Ollama => &self.ollama_embed,
                Provider::Openai => "text-embedding-3-small",
                Provider::Gemini => "text-embedding-004",
                Provider::Xai => "text-embedding-3-small",
                Provider::Anthropic => &self.anthropic_chat,
            };
        }
        match provider {
            Provider::Ollama => &self.ollama_chat,
            Provider::Openai => &self.openai_chat,
            Provider::Anthropic => &self.anthropic_chat,
            Provider::Xai => &self.xai_chat,
            Provider::Gemini => {
                if matches!(
                    task,
                    TaskKind::ArchitectureReview | TaskKind::Reasoning | TaskKind::Planning
                ) {
                    &self.gemini_long
                } else {
                    &self.gemini_chat
                }
            }
        }
    }

    /// Spec examples: planning → Claude, small rust → Qwen, JSON → GPT, embed → local, large context → Gemini.
    pub fn preferred_for_task(&self, task: TaskKind, policy: RoutingPolicy) -> Option<ModelTarget> {
        if matches!(policy, RoutingPolicy::LocalOnly) {
            return Some(ModelTarget {
                provider: Provider::Ollama,
                model: self.default_model(Provider::Ollama, task).to_string(),
            });
        }
        if matches!(policy, RoutingPolicy::CloudOnly) && task.is_embedding() {
            return Some(ModelTarget {
                provider: Provider::Openai,
                model: self.default_model(Provider::Openai, task).to_string(),
            });
        }
        let target = match task {
            TaskKind::Planning | TaskKind::IssuePlanning | TaskKind::ArchitectureReview => {
                ModelTarget {
                    provider: Provider::Anthropic,
                    model: self.anthropic_chat.clone(),
                }
            }
            TaskKind::CodeGeneration | TaskKind::Refactoring => ModelTarget {
                provider: Provider::Ollama,
                model: self.ollama_chat.clone(),
            },
            TaskKind::Extraction | TaskKind::Classification | TaskKind::Moderation => ModelTarget {
                provider: Provider::Openai,
                model: self.openai_chat.clone(),
            },
            TaskKind::Embedding => ModelTarget {
                provider: Provider::Ollama,
                model: self.ollama_embed.clone(),
            },
            TaskKind::Reasoning => ModelTarget {
                provider: Provider::Gemini,
                model: self.gemini_long.clone(),
            },
            TaskKind::Summarization | TaskKind::Conversation => ModelTarget {
                provider: Provider::Openai,
                model: self.openai_chat.clone(),
            },
        };
        if matches!(policy, RoutingPolicy::CloudOnly) && target.provider.is_local() {
            return Some(ModelTarget {
                provider: Provider::Openai,
                model: self.openai_chat.clone(),
            });
        }
        if matches!(policy, RoutingPolicy::LowestCost | RoutingPolicy::Fastest)
            && !task.is_embedding()
        {
            return Some(ModelTarget {
                provider: Provider::Ollama,
                model: self.ollama_chat.clone(),
            });
        }
        Some(target)
    }
}
