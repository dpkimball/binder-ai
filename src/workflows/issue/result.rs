use std::path::PathBuf;

use crate::runtime::{CommandResult, GitDiff};

/// Reviewable outcome of [`super::IssueImplementationWorkflow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueImplementationResult {
    pub success: bool,
    pub modified_files: Vec<PathBuf>,
    pub command_results: Vec<CommandResult>,
    pub git_diff: GitDiff,
    pub summary: String,
}

/// Workflow failure. Messages are kinds/summaries, not provider payloads.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum WorkflowError {
    #[error("search failed: {message}")]
    SearchFailed { message: String },

    #[error("planning failed: {message}")]
    PlanningFailed { message: String },

    #[error("implementation failed: {message}")]
    ImplementationFailed { message: String },

    #[error("formatting failed: {message}")]
    FormattingFailed { message: String },

    #[error("testing failed: {message}")]
    TestingFailed { message: String },

    #[error("git failed: {message}")]
    GitFailed { message: String },

    #[error("knowledge failed: {message}")]
    KnowledgeFailed { message: String },

    #[error("workspace resolution failed: {unresolved} (expected repository {expected_repository}): {message}")]
    WorkspaceResolution {
        unresolved: String,
        expected_repository: String,
        message: String,
    },
}

impl WorkflowError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SearchFailed { .. } => "search_failed",
            Self::PlanningFailed { .. } => "planning_failed",
            Self::ImplementationFailed { .. } => "implementation_failed",
            Self::FormattingFailed { .. } => "formatting_failed",
            Self::TestingFailed { .. } => "testing_failed",
            Self::GitFailed { .. } => "git_failed",
            Self::KnowledgeFailed { .. } => "knowledge_failed",
            Self::WorkspaceResolution { .. } => "workspace_resolution",
        }
    }

    pub fn from_ai_planning(err: &crate::error::AiError) -> Self {
        Self::PlanningFailed {
            message: err.kind().to_string(),
        }
    }

    pub fn from_ai_implementation(err: &crate::error::AiError) -> Self {
        Self::ImplementationFailed {
            message: err.kind().to_string(),
        }
    }

    pub fn from_runtime_search(err: &crate::runtime::RuntimeError) -> Self {
        Self::SearchFailed {
            message: err.kind().to_string(),
        }
    }

    pub fn from_runtime_read(err: &crate::runtime::RuntimeError) -> Self {
        Self::SearchFailed {
            message: err.kind().to_string(),
        }
    }

    pub fn from_runtime_write(err: &crate::runtime::RuntimeError) -> Self {
        Self::ImplementationFailed {
            message: err.kind().to_string(),
        }
    }

    pub fn from_runtime_fmt(err: &crate::runtime::RuntimeError) -> Self {
        Self::FormattingFailed {
            message: err.kind().to_string(),
        }
    }

    pub fn from_runtime_test(err: &crate::runtime::RuntimeError) -> Self {
        Self::TestingFailed {
            message: err.kind().to_string(),
        }
    }

    pub fn from_runtime_git(err: &crate::runtime::RuntimeError) -> Self {
        Self::GitFailed {
            message: err.kind().to_string(),
        }
    }

    pub fn from_knowledge(err: crate::knowledge::KnowledgeError) -> Self {
        Self::KnowledgeFailed {
            message: err.message,
        }
    }

    pub fn from_workspace(err: crate::workspace::WorkspaceResolveError) -> Self {
        Self::WorkspaceResolution {
            unresolved: err.unresolved,
            expected_repository: err.expected_repository,
            message: err.message,
        }
    }
}
