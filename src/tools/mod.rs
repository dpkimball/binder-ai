use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{AiError, Result};
use binder_llm::{ToolCall, ToolDefinition, ToolResult};

/// Named tools the registry can advertise. Execution is injected; this crate does not own git or files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    SearchCode,
    ReadFile,
    WriteFile,
    RunTests,
    RunCargo,
    GitStatus,
    GitCommit,
    GitDiff,
    GitHubIssue,
}

impl ToolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SearchCode => "search_code",
            Self::ReadFile => "read_file",
            Self::WriteFile => "write_file",
            Self::RunTests => "run_tests",
            Self::RunCargo => "run_cargo",
            Self::GitStatus => "git_status",
            Self::GitCommit => "git_commit",
            Self::GitDiff => "git_diff",
            Self::GitHubIssue => "github_issue",
        }
    }

    pub fn all() -> &'static [ToolKind] {
        &[
            Self::SearchCode,
            Self::ReadFile,
            Self::WriteFile,
            Self::RunTests,
            Self::RunCargo,
            Self::GitStatus,
            Self::GitCommit,
            Self::GitDiff,
            Self::GitHubIssue,
        ]
    }

    pub fn definition(self) -> ToolDefinition {
        let (description, parameters) = match self {
            Self::SearchCode => (
                "Search repository code for a query",
                serde_json::json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
            ),
            Self::ReadFile => (
                "Read a file from the workspace",
                serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            ),
            Self::WriteFile => (
                "Write a file in the workspace",
                serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"contents":{"type":"string"}},"required":["path","contents"]}),
            ),
            Self::RunTests => (
                "Run the project test suite",
                serde_json::json!({"type":"object","properties":{"filter":{"type":"string"}}}),
            ),
            Self::RunCargo => (
                "Run a cargo command",
                serde_json::json!({"type":"object","properties":{"args":{"type":"array","items":{"type":"string"}}},"required":["args"]}),
            ),
            Self::GitStatus => (
                "Show git status",
                serde_json::json!({"type":"object","properties":{}}),
            ),
            Self::GitCommit => (
                "Create a git commit",
                serde_json::json!({"type":"object","properties":{"message":{"type":"string"}},"required":["message"]}),
            ),
            Self::GitDiff => (
                "Show git diff",
                serde_json::json!({"type":"object","properties":{"path":{"type":"string"}}}),
            ),
            Self::GitHubIssue => (
                "Fetch a GitHub issue",
                serde_json::json!({"type":"object","properties":{"number":{"type":"integer"}},"required":["number"]}),
            ),
        };
        ToolDefinition::new(self.as_str(), description, parameters)
    }
}

/// Describes tools. Does not execute them.
#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    definitions: Vec<ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn binder_defaults() -> Self {
        let mut registry = Self::new();
        for kind in ToolKind::all() {
            registry.register(kind.definition());
        }
        registry
    }

    pub fn register(&mut self, definition: ToolDefinition) {
        self.definitions
            .retain(|existing| existing.name != definition.name);
        self.definitions.push(definition);
    }

    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}

/// Runs a tool call. Binders (git, storage, …) implement this; binder-ai only orchestrates.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult>;
}

/// Executor that records calls and returns configured results. No side effects.
#[derive(Debug, Default)]
pub struct MockToolExecutor {
    inner: std::sync::Mutex<MockToolInner>,
}

#[derive(Debug, Default)]
struct MockToolInner {
    results: std::collections::HashMap<String, ToolResult>,
    default_error: bool,
    calls: Vec<ToolCall>,
}

impl MockToolExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_result(self, name: impl Into<String>, result: ToolResult) -> Self {
        self.inner
            .lock()
            .expect("mock tool mutex")
            .results
            .insert(name.into(), result);
        self
    }

    pub fn calls(&self) -> Vec<ToolCall> {
        self.inner.lock().expect("mock tool mutex").calls.clone()
    }
}

#[async_trait]
impl ToolExecutor for MockToolExecutor {
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| AiError::tool("tool executor poisoned"))?;
        inner.calls.push(call.clone());
        if let Some(result) = inner.results.get(&call.name).cloned() {
            return Ok(result);
        }
        if inner.default_error {
            return Err(AiError::tool(format!("tool '{}' failed", call.name)));
        }
        Ok(ToolResult::new(
            call.id.clone(),
            format!("unbound tool '{}'", call.name),
        ))
    }
}

/// Rejects every call. Used when tools are advertised but no binder capability is registered.
#[derive(Debug, Default)]
pub struct UnboundToolExecutor;

#[async_trait]
impl ToolExecutor for UnboundToolExecutor {
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            content: format!("tool '{}' is not bound to an executor", call.name),
            is_error: true,
        })
    }
}
