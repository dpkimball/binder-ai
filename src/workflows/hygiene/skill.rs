//! JSON contract for the Cursor hygiene skill. Discovery stays outside this crate.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::workflows::hygiene::finding::{Finding, FindingKind, ProjectKind};
use crate::workflows::hygiene::operational::OperationalContext;
use crate::workflows::hygiene::workflow::{HygieneWorkflowReport, HygieneWorkflowRequest};

/// Skill-shaped apply document. Operational findings are not read from bindb.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillApplyDocument {
    pub request: String,
    pub findings: Vec<SkillFindingJson>,
    #[serde(default)]
    pub operational: Option<SkillOperationalJson>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillFindingJson {
    pub repository: String,
    pub repository_root: PathBuf,
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub target_ref: Option<String>,
    #[serde(default)]
    pub github_slug: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillOperationalJson {
    pub default_branch: String,
    pub current_branch: String,
    #[serde(default)]
    pub dirty: bool,
    #[serde(default)]
    pub open_pr_heads: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillApplyReport {
    pub success: bool,
    pub llm_invoked: bool,
    pub validation_passed: bool,
    pub history_recorded: bool,
    pub search_used: bool,
    pub primary_source: &'static str,
    pub planning_ms: u64,
    pub reindex_ms: u64,
    pub inspection_set: Vec<String>,
    pub resolved_execution_paths: Vec<String>,
    pub files_opened: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_repository: Option<String>,
    pub outcomes: Vec<SkillOutcomeJson>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillOutcomeJson {
    pub repository: String,
    pub finding: String,
    pub remediation: String,
    pub success: bool,
    pub summary: String,
}

impl SkillApplyDocument {
    pub fn parse(raw: &str) -> Result<Self, String> {
        serde_json::from_str(raw).map_err(|err| format!("skill apply json: {err}"))
    }

    pub fn into_request(self) -> Result<HygieneWorkflowRequest, String> {
        let mut findings = Vec::with_capacity(self.findings.len());
        for row in self.findings {
            findings.push(row.into_finding()?);
        }
        Ok(HygieneWorkflowRequest {
            request: self.request,
            findings,
            operational: self.operational.map(SkillOperationalJson::into_context),
        })
    }
}

impl SkillFindingJson {
    fn into_finding(self) -> Result<Finding, String> {
        let kind = FindingKind::parse(&self.kind)
            .ok_or_else(|| format!("unknown finding kind `{}`", self.kind))?;
        let mut finding = Finding::new(self.repository, kind, self.description);
        finding.repository_root = self.repository_root;
        if let Some(project) = self.project.as_deref() {
            finding.project = ProjectKind::parse(project)
                .ok_or_else(|| format!("unknown project kind `{project}`"))?;
        }
        if let Some(name) = self.target_ref {
            finding.target_ref = Some(name);
        }
        if let Some(slug) = self.github_slug {
            finding.github_slug = Some(slug);
        }
        Ok(finding)
    }
}

impl SkillOperationalJson {
    fn into_context(self) -> OperationalContext {
        OperationalContext {
            default_branch: self.default_branch,
            current_branch: self.current_branch,
            dirty: self.dirty,
            open_pr_heads: self.open_pr_heads,
        }
    }
}

impl SkillApplyReport {
    pub fn from_workflow(report: &HygieneWorkflowReport) -> Self {
        Self {
            success: report.success,
            llm_invoked: report.llm_invoked,
            validation_passed: report.validation_passed,
            history_recorded: report.history_recorded,
            search_used: report.search_used,
            primary_source: report.primary_source,
            planning_ms: report.planning_ms,
            reindex_ms: report.reindex_ms,
            inspection_set: report
                .inspection_set
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            resolved_execution_paths: report
                .resolved_execution_paths
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            files_opened: report
                .files_opened
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            error_kind: None,
            unresolved: None,
            expected_repository: None,
            outcomes: report
                .hygiene
                .outcomes
                .iter()
                .map(|row| SkillOutcomeJson {
                    repository: row.repository.clone(),
                    finding: row.finding.as_str().into(),
                    remediation: row.remediation.as_str().into(),
                    success: row.success,
                    summary: row.summary.clone(),
                })
                .collect(),
        }
    }

    pub fn from_workspace(
        err: &crate::workflows::issue::WorkflowError,
        request: &HygieneWorkflowRequest,
    ) -> Option<Self> {
        let crate::workflows::issue::WorkflowError::WorkspaceResolution {
            unresolved,
            expected_repository,
            message,
        } = err
        else {
            return None;
        };
        let summary = format!(
            "workspace_resolution: {unresolved} (expected repository {expected_repository}): {message}"
        );
        Some(Self {
            success: false,
            llm_invoked: false,
            validation_passed: false,
            history_recorded: false,
            search_used: false,
            primary_source: "bindb",
            planning_ms: 0,
            reindex_ms: 0,
            inspection_set: Vec::new(),
            resolved_execution_paths: Vec::new(),
            files_opened: Vec::new(),
            error_kind: Some("workspace_resolution"),
            unresolved: Some(unresolved.clone()),
            expected_repository: Some(expected_repository.clone()),
            outcomes: request
                .findings
                .iter()
                .map(|finding| SkillOutcomeJson {
                    repository: finding.repository.clone(),
                    finding: finding.kind.as_str().into(),
                    remediation: "ai".into(),
                    success: false,
                    summary: summary.clone(),
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::hygiene::finding::{Finding, FindingKind, ProjectKind};
    use crate::workflows::hygiene::workflow::HygieneWorkflowRequest;

    #[test]
    fn parses_skill_shaped_discovery_json() {
        let raw = r#"{
            "request": "hygiene hygiene-skill",
            "findings": [
                {
                    "repository": "hygiene-skill",
                    "repository_root": "/tmp/repo",
                    "kind": "feature_branch_merged",
                    "description": "leftover",
                    "target_ref": "feature/old"
                },
                {
                    "repository": "hygiene-skill",
                    "repository_root": "/tmp/repo",
                    "kind": "cargo_fmt_required",
                    "description": "fmt",
                    "project": "rust_cargo"
                },
                {
                    "repository": "hygiene-skill",
                    "repository_root": "/tmp/repo",
                    "kind": "missing_badge",
                    "github_slug": "dpkimball/hygiene-skill"
                }
            ],
            "operational": {
                "default_branch": "master",
                "current_branch": "master",
                "dirty": false,
                "open_pr_heads": []
            }
        }"#;
        let request = SkillApplyDocument::parse(raw)
            .unwrap()
            .into_request()
            .unwrap();
        assert_eq!(request.request, "hygiene hygiene-skill");
        assert_eq!(request.findings.len(), 3);
        assert_eq!(request.findings[0].kind, FindingKind::FeatureBranchMerged);
        assert_eq!(
            request.findings[0].target_ref.as_deref(),
            Some("feature/old")
        );
        assert_eq!(request.findings[1].kind, FindingKind::CargoFmtRequired);
        assert_eq!(request.findings[1].project, ProjectKind::RustCargo);
        assert_eq!(request.findings[2].kind, FindingKind::MissingBadge);
        let op = request.operational.expect("operational");
        assert_eq!(op.default_branch, "master");
        assert!(!op.dirty);
        assert!(op.open_pr_heads.is_empty());
    }

    #[test]
    fn rejects_unknown_finding_kind() {
        let raw = r#"{
            "request": "hygiene x",
            "findings": [{
                "repository": "x",
                "repository_root": ".",
                "kind": "not_a_kind"
            }]
        }"#;
        let err = SkillApplyDocument::parse(raw)
            .unwrap()
            .into_request()
            .unwrap_err();
        assert!(err.contains("unknown finding kind"), "{err}");
    }

    #[test]
    fn workspace_resolution_report_is_structured() {
        let request = HygieneWorkflowRequest {
            request: "hygiene rust bug".into(),
            findings: vec![Finding::new("demo", FindingKind::RustBug, "bug")],
            operational: None,
        };
        let err = crate::workflows::issue::WorkflowError::WorkspaceResolution {
            unresolved: "binder-server:Dockerfile".into(),
            expected_repository: "binder-server".into(),
            message: "no checkout for repository in the execution workspace".into(),
        };
        let report = SkillApplyReport::from_workspace(&err, &request).unwrap();
        assert!(!report.success);
        assert_eq!(report.error_kind, Some("workspace_resolution"));
        assert_eq!(
            report.unresolved.as_deref(),
            Some("binder-server:Dockerfile")
        );
        assert_eq!(report.expected_repository.as_deref(), Some("binder-server"));
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("workspace_resolution"));
        assert!(!json.contains("knowledge_failed"));
        assert_eq!(report.outcomes[0].finding, "rust_bug");
    }
}
