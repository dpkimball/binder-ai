use serde::{Deserialize, Serialize};

use crate::context::IssueContext;

/// Work the caller wants done. The router chooses how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Task {
    Summarization {
        document: String,
    },
    Classification {
        text: String,
        #[serde(default)]
        labels: Vec<String>,
    },
    Extraction {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema_hint: Option<String>,
    },
    Planning {
        goal: String,
    },
    IssuePlanning {
        issue: IssueContext,
    },
    CodeGeneration {
        spec: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
    },
    CodeReview {
        diff: String,
    },
    ArchitectureReview {
        repository: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pull_request: Option<String>,
    },
    Refactoring {
        code: String,
        instruction: String,
    },
    Reasoning {
        prompt: String,
    },
    Conversation {
        message: String,
    },
    Embedding {
        input: String,
    },
    Moderation {
        content: String,
    },
}

/// Discriminant used by routing, prompts, and telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Summarization,
    Classification,
    Extraction,
    Planning,
    IssuePlanning,
    CodeGeneration,
    CodeReview,
    ArchitectureReview,
    Refactoring,
    Reasoning,
    Conversation,
    Embedding,
    Moderation,
}

impl Task {
    pub fn kind(&self) -> TaskKind {
        match self {
            Self::Summarization { .. } => TaskKind::Summarization,
            Self::Classification { .. } => TaskKind::Classification,
            Self::Extraction { .. } => TaskKind::Extraction,
            Self::Planning { .. } => TaskKind::Planning,
            Self::IssuePlanning { .. } => TaskKind::IssuePlanning,
            Self::CodeGeneration { .. } => TaskKind::CodeGeneration,
            Self::CodeReview { .. } => TaskKind::CodeReview,
            Self::ArchitectureReview { .. } => TaskKind::ArchitectureReview,
            Self::Refactoring { .. } => TaskKind::Refactoring,
            Self::Reasoning { .. } => TaskKind::Reasoning,
            Self::Conversation { .. } => TaskKind::Conversation,
            Self::Embedding { .. } => TaskKind::Embedding,
            Self::Moderation { .. } => TaskKind::Moderation,
        }
    }

    pub fn user_input(&self) -> String {
        match self {
            Self::Summarization { document } => document.clone(),
            Self::Classification { text, labels } => {
                if labels.is_empty() {
                    text.clone()
                } else {
                    format!("{text}\n\nLabels: {}", labels.join(", "))
                }
            }
            Self::Extraction { text, schema_hint } => match schema_hint {
                Some(hint) => format!("{text}\n\nSchema: {hint}"),
                None => text.clone(),
            },
            Self::Planning { goal } => goal.clone(),
            Self::IssuePlanning { issue } => issue.render(),
            Self::CodeGeneration { spec, language } => match language {
                Some(lang) => format!("Language: {lang}\n\n{spec}"),
                None => spec.clone(),
            },
            Self::CodeReview { diff } => diff.clone(),
            Self::ArchitectureReview {
                repository,
                pull_request,
            } => match pull_request {
                Some(pr) => format!("Repository: {repository}\nPull request: {pr}"),
                None => format!("Repository: {repository}"),
            },
            Self::Refactoring { code, instruction } => {
                format!("{instruction}\n\n```\n{code}\n```")
            }
            Self::Reasoning { prompt } => prompt.clone(),
            Self::Conversation { message } => message.clone(),
            Self::Embedding { input } => input.clone(),
            Self::Moderation { content } => content.clone(),
        }
    }

    pub fn prompt_name(&self) -> &'static str {
        self.kind().prompt_name()
    }
}

impl TaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Summarization => "summarization",
            Self::Classification => "classification",
            Self::Extraction => "extraction",
            Self::Planning => "planning",
            Self::IssuePlanning => "issue_planning",
            Self::CodeGeneration => "code_generation",
            Self::CodeReview => "code_review",
            Self::ArchitectureReview => "architecture_review",
            Self::Refactoring => "refactoring",
            Self::Reasoning => "reasoning",
            Self::Conversation => "conversation",
            Self::Embedding => "embedding",
            Self::Moderation => "moderation",
        }
    }

    pub fn prompt_name(self) -> &'static str {
        match self {
            Self::ArchitectureReview => "architecture-review",
            Self::CodeGeneration => "code-generation",
            Self::CodeReview => "code-review",
            Self::IssuePlanning => "issue-planning",
            other => other.as_str(),
        }
    }

    pub fn wants_json(self) -> bool {
        matches!(
            self,
            Self::Classification | Self::Extraction | Self::Planning | Self::IssuePlanning
        )
    }

    pub fn wants_tools(self) -> bool {
        matches!(
            self,
            Self::ArchitectureReview
                | Self::CodeGeneration
                | Self::CodeReview
                | Self::Refactoring
                | Self::IssuePlanning
        )
    }

    pub fn is_embedding(self) -> bool {
        matches!(self, Self::Embedding)
    }
}

impl std::fmt::Display for TaskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_matches_variant() {
        let task = Task::Summarization {
            document: "x".into(),
        };
        assert_eq!(task.kind(), TaskKind::Summarization);
        assert_eq!(task.prompt_name(), "summarization");
    }
}
