use serde::{Deserialize, Serialize};

/// Composable context inputs. Binders register data; this crate does not own the sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextSource {
    Conversation(String),
    Memory(String),
    User(String),
    Repository(String),
    GitDiff(String),
    Issue(IssueContext),
    Configuration(String),
    Workspace(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueContext {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl IssueContext {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: None,
            body: None,
        }
    }

    pub fn render(&self) -> String {
        let mut out = format!("Issue: {}", self.id);
        if let Some(title) = &self.title {
            out.push_str("\nTitle: ");
            out.push_str(title);
        }
        if let Some(body) = &self.body {
            out.push_str("\n\n");
            out.push_str(body);
        }
        out
    }
}

impl ContextSource {
    pub fn heading(&self) -> &'static str {
        match self {
            Self::Conversation(_) => "Conversation",
            Self::Memory(_) => "Memory",
            Self::User(_) => "User",
            Self::Repository(_) => "Repository",
            Self::GitDiff(_) => "Git Diff",
            Self::Issue(_) => "Issue",
            Self::Configuration(_) => "Configuration",
            Self::Workspace(_) => "Workspace",
        }
    }

    pub fn body(&self) -> String {
        match self {
            Self::Conversation(v)
            | Self::Memory(v)
            | Self::User(v)
            | Self::Repository(v)
            | Self::GitDiff(v)
            | Self::Configuration(v)
            | Self::Workspace(v) => v.clone(),
            Self::Issue(issue) => issue.render(),
        }
    }
}
