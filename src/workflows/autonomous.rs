//! Unattended issue implementation entry (CLI). Bindb-first; never searches.

use std::path::PathBuf;

use serde::Serialize;

use crate::client::AiClient;
use crate::intel::{plan, AgentPlan, IntelError, PlanRequest, PlanResponse, PlanningResult};
use crate::knowledge::Knowledge;
use crate::runtime::{CommandResult, Runtime};
use crate::workflows::issue::{
    Issue, IssueImplementationRequest, IssueImplementationWorkflow, WorkflowError,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommandOutcome {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Dry-run prints the inspect set. Execute writes via [`IssueImplementationWorkflow`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImplementReport {
    pub success: bool,
    pub declined: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decline_reason: Option<String>,
    pub dry_run: bool,
    pub llm_invoked: bool,
    pub summary: String,
    pub inspection_set: Vec<String>,
    pub resolved_execution_paths: Vec<String>,
    pub files_modified: Vec<String>,
    pub planning: AgentPlan,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub command_results: Vec<CommandOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_diff: Option<String>,
}

const MAX_COMMAND_TEXT: usize = 8000;

fn truncate_cmd(text: &str) -> String {
    if text.len() <= MAX_COMMAND_TEXT {
        text.to_string()
    } else {
        format!("{}…", &text[..MAX_COMMAND_TEXT])
    }
}

fn command_outcomes(results: &[CommandResult]) -> Vec<CommandOutcome> {
    results
        .iter()
        .map(|result| CommandOutcome {
            exit_code: result.exit_code,
            stdout: truncate_cmd(&result.stdout),
            stderr: truncate_cmd(&result.stderr),
        })
        .collect()
}

fn summary_from_failed_commands(results: &[CommandOutcome]) -> String {
    results
        .iter()
        .find(|result| result.exit_code != 0)
        .map(|result| {
            let body = if !result.stderr.trim().is_empty() {
                result.stderr.trim()
            } else {
                result.stdout.trim()
            };
            format!("command exit {}: {body}", result.exit_code)
        })
        .unwrap_or_default()
}

/// Why this issue must not run unattended. `None` when the inspect set is usable.
pub fn autonomous_decline_reason(plan: &PlanResponse) -> Option<&'static str> {
    if plan.fallback_required {
        return Some("fallback_required");
    }
    if plan.planning_result != PlanningResult::Resolved {
        return Some("planning_not_resolved");
    }
    if plan.inspection_set.is_empty() {
        return Some("empty_inspection_set");
    }
    None
}

/// Bindb `repo:path` execution paths are `repo/relative`. A single-repo checkout
/// (Foreman) needs `relative` so [`crate::runtime::LocalRuntime`] can open files.
pub fn checkout_relative_paths(plan: &PlanResponse, repo_name: &str) -> Vec<PathBuf> {
    let raw = if !plan.resolved_execution_paths.is_empty() {
        &plan.resolved_execution_paths
    } else {
        &plan.inspection_set
    };
    let prefix = format!("{}/", repo_name.trim().trim_end_matches('/'));
    raw.iter()
        .map(|path| strip_repo_prefix(path, repo_name, &prefix))
        .collect()
}

fn strip_repo_prefix(path: &str, repo_name: &str, prefix: &str) -> PathBuf {
    let normalized = path.replace('\\', "/");
    if repo_name.is_empty() {
        return PathBuf::from(&normalized);
    }
    if normalized == repo_name {
        return PathBuf::from(".");
    }
    if let Some(rest) = normalized.strip_prefix(prefix) {
        if rest.is_empty() {
            PathBuf::from(".")
        } else {
            PathBuf::from(rest)
        }
    } else {
        PathBuf::from(normalized)
    }
}

/// Plan only. Never calls a model. Never writes.
pub fn plan_for_implement<K: Knowledge>(
    knowledge: &K,
    request: &str,
    workspace: PathBuf,
    binds: Vec<(String, PathBuf)>,
) -> Result<PlanResponse, IntelError> {
    plan(
        knowledge,
        &PlanRequest {
            workspace: Some(workspace),
            request: request.to_string(),
            since_sha: None,
            binds,
        },
    )
}

pub fn report_from_plan(plan: PlanResponse, repo_name: &str, dry_run: bool) -> ImplementReport {
    let inspection_set = plan.inspection_set.clone();
    let resolved_execution_paths = plan.resolved_execution_paths.clone();
    let planning = plan.for_agent();
    if let Some(reason) = autonomous_decline_reason(&plan) {
        return ImplementReport {
            success: false,
            declined: true,
            decline_reason: Some(reason.to_string()),
            dry_run,
            llm_invoked: false,
            summary: format!(
                "declined: {reason}; needs a human or external agent (no repository search)"
            ),
            inspection_set,
            resolved_execution_paths,
            files_modified: Vec::new(),
            planning,
            command_results: Vec::new(),
            git_diff: None,
        };
    }
    let relative = checkout_relative_paths(&plan, repo_name);
    let n = relative.len();
    ImplementReport {
        success: true,
        declined: false,
        decline_reason: None,
        dry_run,
        llm_invoked: false,
        summary: format!("dry-run: {n} file(s) in inspection set; no writes"),
        inspection_set,
        resolved_execution_paths,
        files_modified: Vec::new(),
        planning,
        command_results: Vec::new(),
        git_diff: None,
    }
}

/// Execute after a resolved plan. Inspection set only; no repository search.
pub async fn execute_implement<R: Runtime>(
    runtime: R,
    ai: AiClient,
    issue: Issue,
    repository_root: PathBuf,
    repo_name: &str,
    plan: &PlanResponse,
) -> Result<ImplementReport, WorkflowError> {
    if let Some(reason) = autonomous_decline_reason(plan) {
        return Ok(report_from_plan(plan.clone(), repo_name, false).with_reason_override(reason));
    }
    let files = checkout_relative_paths(plan, repo_name);
    let workflow = IssueImplementationWorkflow::new(runtime, ai);
    let result = workflow
        .execute(IssueImplementationRequest::new(issue, repository_root).with_inspection_set(files))
        .await?;
    let command_results = command_outcomes(&result.command_results);
    let mut summary = result.summary;
    if !result.success && summary.trim().is_empty() {
        summary = summary_from_failed_commands(&command_results);
    }
    Ok(ImplementReport {
        success: result.success,
        declined: false,
        decline_reason: None,
        dry_run: false,
        llm_invoked: true,
        summary,
        inspection_set: plan.inspection_set.clone(),
        resolved_execution_paths: plan.resolved_execution_paths.clone(),
        files_modified: result
            .modified_files
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect(),
        planning: plan.for_agent(),
        command_results,
        git_diff: Some(result.git_diff.patch),
    })
}

/// JSON the CLI prints when the workflow returns an error (Foreman must not see empty stdout).
pub fn report_from_workflow_error(plan: &PlanResponse, err: &WorkflowError) -> ImplementReport {
    ImplementReport {
        success: false,
        declined: false,
        decline_reason: None,
        dry_run: false,
        llm_invoked: true,
        summary: format!("{}: {}", err.kind(), err),
        inspection_set: plan.inspection_set.clone(),
        resolved_execution_paths: plan.resolved_execution_paths.clone(),
        files_modified: Vec::new(),
        planning: plan.for_agent(),
        command_results: Vec::new(),
        git_diff: None,
    }
}

impl ImplementReport {
    fn with_reason_override(mut self, reason: &str) -> Self {
        self.dry_run = false;
        self.decline_reason = Some(reason.to_string());
        self
    }
}

#[cfg(test)]
mod prefix_tests {
    use super::{report_from_workflow_error, strip_repo_prefix};
    use crate::intel::{PlanResponse, PlanningMetrics, PlanningResult};
    use crate::workflows::issue::WorkflowError;
    use std::path::PathBuf;

    #[test]
    fn strips_bound_repo_prefix() {
        assert_eq!(
            strip_repo_prefix("binder-ai/src/lib.rs", "binder-ai", "binder-ai/"),
            PathBuf::from("src/lib.rs")
        );
        assert_eq!(
            strip_repo_prefix("src/lib.rs", "binder-ai", "binder-ai/"),
            PathBuf::from("src/lib.rs")
        );
    }

    #[test]
    fn workflow_error_report_is_json_failure_with_kind() {
        let plan = PlanResponse {
            planning_source: "bindb",
            planning_result: PlanningResult::Resolved,
            fallback_required: false,
            fallback_reason: None,
            repository_discovery_recommended: false,
            primary_source: "bindb",
            inspection_set: vec!["src/lib.rs".into()],
            resolved_execution_paths: vec!["src/lib.rs".into()],
            unresolved: vec![],
            reasoning: String::new(),
            doctrine: vec![],
            impact: vec![],
            recent_changes: vec![],
            linked_issues: vec![],
            linked_prs: vec![],
            linked_docs: vec![],
            linked_skills: vec![],
            origin_issues: vec![],
            matched_kind: String::new(),
            file_roles: vec![],
            touch_sequence: vec![],
            matched_summary: String::new(),
            task_hint: String::new(),
            metrics: PlanningMetrics {
                planning_source: "bindb",
                planning_result: PlanningResult::Resolved,
                fallback_required: false,
                fallback_reason: None,
                inspection_set_size: 1,
                files_opened_from_inspection_set: 0,
                repository_search_performed: false,
            },
            guidance: vec![],
        };
        let err = WorkflowError::ImplementationFailed {
            message: "no_eligible_model".into(),
        };
        let report = report_from_workflow_error(&plan, &err);
        assert!(!report.success);
        assert!(!report.declined);
        assert!(report.summary.contains("implementation_failed"));
        assert!(report.summary.contains("no_eligible_model"));
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("no_eligible_model"));
    }

    #[test]
    fn failed_command_summary_uses_stderr() {
        let results = vec![super::CommandOutcome {
            exit_code: 101,
            stdout: String::new(),
            stderr: "error: cannot find type `AiClient` in this scope".into(),
        }];
        let summary = super::summary_from_failed_commands(&results);
        assert!(summary.contains("command exit 101"));
        assert!(summary.contains("cannot find type"));
    }

    #[test]
    fn implement_report_json_includes_command_results() {
        let plan = PlanResponse {
            planning_source: "bindb",
            planning_result: PlanningResult::Resolved,
            fallback_required: false,
            fallback_reason: None,
            repository_discovery_recommended: false,
            primary_source: "bindb",
            inspection_set: vec!["src/lib.rs".into()],
            resolved_execution_paths: vec!["src/lib.rs".into()],
            unresolved: vec![],
            reasoning: String::new(),
            doctrine: vec![],
            impact: vec![],
            recent_changes: vec![],
            linked_issues: vec![],
            linked_prs: vec![],
            linked_docs: vec![],
            linked_skills: vec![],
            origin_issues: vec![],
            matched_kind: String::new(),
            file_roles: vec![],
            touch_sequence: vec![],
            matched_summary: String::new(),
            task_hint: String::new(),
            metrics: PlanningMetrics {
                planning_source: "bindb",
                planning_result: PlanningResult::Resolved,
                fallback_required: false,
                fallback_reason: None,
                inspection_set_size: 1,
                files_opened_from_inspection_set: 0,
                repository_search_performed: false,
            },
            guidance: vec![],
        };
        let err = WorkflowError::ImplementationFailed {
            message: "empty_edits".into(),
        };
        let mut report = report_from_workflow_error(&plan, &err);
        report.command_results = vec![super::CommandOutcome {
            exit_code: 101,
            stdout: String::new(),
            stderr: "error: cannot find type `AiClient` in this scope".into(),
        }];
        report.summary = super::summary_from_failed_commands(&report.command_results);
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"command_results\""));
        assert!(json.contains("\"exit_code\":101"));
        assert!(json.contains("cannot find type"));
        assert!(report.summary.contains("command exit 101"));
    }
}
