//! Engineering intelligence: bindb planning for IDEs and workflows.
//!
//! Returns structured context. Does not walk the repository, call a model,
//! or modify files. Repository discovery is a client fallback, never this
//! module.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::knowledge::{Knowledge, KnowledgeError, LinkedIssueSummary, RecentChange};
use crate::workspace::{resolve_inspection_set_best_effort, WorkspaceLayout};

/// IDE planning request. `workspace` is the execution root; bindb store is server-side.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanRequest {
    #[serde(default)]
    pub workspace: Option<PathBuf>,
    pub request: String,
    /// When set, `recent_changes` in the response lists inspection-set files that
    /// were added or removed between this SHA and HEAD. The SHA must already be indexed.
    pub since_sha: Option<String>,
}

/// Whether bindb answered, answered incompletely, or could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningResult {
    Resolved,
    Partial,
    Unresolved,
}

/// Why repository discovery is allowed. Absent when `fallback_required` is false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    NotIndexed,
    ParserCoverageInsufficient,
    WorkspaceResolutionFailed,
    SemanticUnavailable,
}

/// Observable planning vs discovery. Filled at plan time; workflows update
/// files-opened and search flags after acquire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningMetrics {
    pub planning_source: &'static str,
    pub planning_result: PlanningResult,
    pub fallback_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<FallbackReason>,
    pub inspection_set_size: usize,
    pub files_opened_from_inspection_set: usize,
    pub repository_search_performed: bool,
}

/// Engineering context only. No file contents, no edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanResponse {
    pub planning_source: &'static str,
    pub planning_result: PlanningResult,
    pub fallback_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<FallbackReason>,
    pub repository_discovery_recommended: bool,
    pub primary_source: &'static str,
    pub inspection_set: Vec<String>,
    pub resolved_execution_paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<UnresolvedTarget>,
    pub reasoning: String,
    pub doctrine: Vec<String>,
    pub impact: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_changes: Vec<RecentChange>,
    /// Top-matched issues with body excerpts for LLM explain context. Not included in AgentPlan.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub linked_issues: Vec<LinkedIssueSummary>,
    pub metrics: PlanningMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnresolvedTarget {
    pub unresolved: String,
    pub expected_repository: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IntelError {
    #[error("empty request")]
    EmptyRequest,
    #[error("knowledge failed: {0}")]
    Knowledge(String),
}

impl IntelError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::EmptyRequest => "empty_request",
            Self::Knowledge(_) => "knowledge_failed",
        }
    }
}

/// Plan from bindb. Never calls an LLM. Never searches the tree.
pub fn plan<K: Knowledge>(
    knowledge: &K,
    request: &PlanRequest,
) -> Result<PlanResponse, IntelError> {
    let text = request.request.trim();
    if text.is_empty() {
        return Err(IntelError::EmptyRequest);
    }
    let knowledge_plan = match request.since_sha.as_deref() {
        Some(sha) => knowledge.plan_since(text, sha),
        None => knowledge.plan(text),
    }
    .map_err(|err: KnowledgeError| IntelError::Knowledge(err.message))?;
    if knowledge_plan.primary_source != "bindb" {
        return Err(IntelError::Knowledge(
            "knowledge plan must declare primary_source bindb".into(),
        ));
    }

    let layout = match request.workspace.as_ref() {
        Some(root) => WorkspaceLayout::new(root),
        None => WorkspaceLayout::passthrough(),
    };
    let attempt = resolve_inspection_set_best_effort(&layout, &knowledge_plan.inspection_set);
    let inspection_set: Vec<String> = knowledge_plan
        .inspection_set
        .iter()
        .map(path_string)
        .collect();
    let resolved_execution_paths: Vec<String> = attempt
        .resolution
        .execution_paths()
        .into_iter()
        .map(path_string)
        .collect();
    let unresolved: Vec<UnresolvedTarget> = attempt
        .unresolved
        .into_iter()
        .map(|err| UnresolvedTarget {
            unresolved: err.unresolved,
            expected_repository: err.expected_repository,
            message: err.message,
        })
        .collect();

    let decision = classify_planning(
        inspection_set.len(),
        resolved_execution_paths.len(),
        unresolved.len(),
        request.workspace.is_some(),
    );
    let metrics = PlanningMetrics {
        planning_source: "bindb",
        planning_result: decision.planning_result,
        fallback_required: decision.fallback_required,
        fallback_reason: decision.fallback_reason,
        inspection_set_size: inspection_set.len(),
        files_opened_from_inspection_set: 0,
        repository_search_performed: false,
    };
    tracing::info!(
        planning_source = "bindb",
        planning_result = ?decision.planning_result,
        fallback_required = decision.fallback_required,
        fallback_reason = ?decision.fallback_reason,
        inspection_set_size = inspection_set.len(),
        repository_search_performed = false,
        "intel.planning"
    );

    Ok(PlanResponse {
        planning_source: "bindb",
        planning_result: decision.planning_result,
        fallback_required: decision.fallback_required,
        fallback_reason: decision.fallback_reason,
        repository_discovery_recommended: decision.repository_discovery_recommended,
        primary_source: "bindb",
        inspection_set,
        resolved_execution_paths,
        unresolved,
        reasoning: knowledge_plan.interpretation,
        doctrine: knowledge_plan.rules_must_remain_satisfied,
        impact: knowledge_plan.likely_affected,
        recent_changes: knowledge_plan.recent_changes,
        linked_issues: knowledge_plan.linked_issues,
        metrics,
    })
}

/// Shared classify so HTTP, CLI, and workflows agree.
pub fn classify_planning(
    inspection_set_len: usize,
    resolved_len: usize,
    unresolved_len: usize,
    workspace_provided: bool,
) -> PlanningDecision {
    if inspection_set_len == 0 {
        return PlanningDecision {
            planning_result: PlanningResult::Unresolved,
            fallback_required: true,
            fallback_reason: Some(FallbackReason::NotIndexed),
            repository_discovery_recommended: true,
        };
    }
    if !workspace_provided {
        return PlanningDecision {
            planning_result: PlanningResult::Resolved,
            fallback_required: false,
            fallback_reason: None,
            repository_discovery_recommended: false,
        };
    }
    if unresolved_len == 0 {
        return PlanningDecision {
            planning_result: PlanningResult::Resolved,
            fallback_required: false,
            fallback_reason: None,
            repository_discovery_recommended: false,
        };
    }
    if resolved_len > 0 {
        return PlanningDecision {
            planning_result: PlanningResult::Partial,
            fallback_required: false,
            fallback_reason: Some(FallbackReason::WorkspaceResolutionFailed),
            repository_discovery_recommended: true,
        };
    }
    PlanningDecision {
        planning_result: PlanningResult::Partial,
        fallback_required: true,
        fallback_reason: Some(FallbackReason::WorkspaceResolutionFailed),
        repository_discovery_recommended: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanningDecision {
    pub planning_result: PlanningResult,
    pub fallback_required: bool,
    pub fallback_reason: Option<FallbackReason>,
    pub repository_discovery_recommended: bool,
}

impl PlanResponse {
    /// Compact payload for IDE agents: skip the store-wide doctrine dump.
    pub fn for_agent(&self) -> AgentPlan {
        AgentPlan {
            planning_source: self.planning_source,
            planning_result: self.planning_result,
            fallback_required: self.fallback_required,
            fallback_reason: self.fallback_reason,
            repository_discovery_recommended: self.repository_discovery_recommended,
            primary_source: self.primary_source,
            inspection_set: self.inspection_set.clone(),
            resolved_execution_paths: self.resolved_execution_paths.clone(),
            unresolved: self.unresolved.clone(),
            reasoning: self.reasoning.clone(),
            impact: self.impact.clone(),
            recent_changes: self.recent_changes.clone(),
            metrics: self.metrics.clone(),
        }
    }

    pub fn with_acquisition(
        mut self,
        files_opened_from_inspection_set: usize,
        repository_search_performed: bool,
    ) -> Self {
        self.metrics.files_opened_from_inspection_set = files_opened_from_inspection_set;
        self.metrics.repository_search_performed = repository_search_performed;
        self
    }
}

/// Inspect-set payload without store-wide doctrine (token-expensive, not request-local).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentPlan {
    pub planning_source: &'static str,
    pub planning_result: PlanningResult,
    pub fallback_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<FallbackReason>,
    pub repository_discovery_recommended: bool,
    pub primary_source: &'static str,
    pub inspection_set: Vec<String>,
    pub resolved_execution_paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<UnresolvedTarget>,
    pub reasoning: String,
    pub impact: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recent_changes: Vec<RecentChange>,
    pub metrics: PlanningMetrics,
}

fn path_string(path: impl AsRef<std::path::Path>) -> String {
    path.as_ref().to_string_lossy().replace('\\', "/")
}
