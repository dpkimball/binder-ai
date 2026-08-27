use std::collections::HashSet;
use std::sync::Mutex;

use crate::client::AiClient;
use crate::runtime::Runtime;
use crate::workflows::hygiene::config::HygieneConfig;
use crate::workflows::hygiene::deterministic;
use crate::workflows::hygiene::dispatcher;
use crate::workflows::hygiene::finding::Finding;
use crate::workflows::hygiene::remediation::{DispatchDecision, RemediationType};
use crate::workflows::hygiene::result::{FindingOutcome, HygieneReport};
use crate::workflows::issue::{
    Issue, IssueImplementationRequest, IssueImplementationResult, IssueImplementationWorkflow,
};

/// Orchestrates dispatch + repair. Inspection is not performed here.
pub struct HygieneEngine {
    config: HygieneConfig,
    ai: AiClient,
    ai_repos_used: Mutex<HashSet<String>>,
}

impl HygieneEngine {
    pub fn new(config: HygieneConfig, ai: AiClient) -> Self {
        Self {
            config,
            ai,
            ai_repos_used: Mutex::new(HashSet::new()),
        }
    }

    /// Default production client: catalog discovery, no hardcoded model.
    pub fn with_discovered_client(config: HygieneConfig) -> Self {
        let ai = AiClient::builder()
            .routing_policy(config.routing_policy)
            .build();
        Self::new(config, ai)
    }

    pub fn config(&self) -> &HygieneConfig {
        &self.config
    }

    pub fn ai(&self) -> &AiClient {
        &self.ai
    }

    /// Process findings against one runtime (one repository checkout).
    /// Failures are recorded; remaining findings still run.
    pub async fn process_findings<R>(&self, runtime: R, findings: &[Finding]) -> HygieneReport
    where
        R: Runtime + Clone,
    {
        let mut report = HygieneReport::default();
        for finding in findings {
            let outcome = self.process_one(runtime.clone(), finding).await;
            tracing::info!(
                repository = %outcome.repository,
                finding = %outcome.finding,
                remediation = %outcome.remediation,
                success = outcome.success,
                "hygiene.finding"
            );
            report.push(outcome);
        }
        report
    }

    async fn process_one<R>(&self, runtime: R, finding: &Finding) -> FindingOutcome
    where
        R: Runtime + Clone,
    {
        match dispatcher::dispatch(finding, &self.config) {
            DispatchDecision::ReportOnly => report_only(finding, "report_only"),
            DispatchDecision::Deterministic(action) => {
                match deterministic::apply(
                    &runtime,
                    &action,
                    &finding.repository_root,
                    self.config.dry_run,
                )
                .await
                {
                    Ok(applied) => FindingOutcome {
                        repository: finding.repository.clone(),
                        finding: finding.kind,
                        remediation: RemediationType::Deterministic,
                        success: true,
                        diff_generated: false,
                        tests_passed: None,
                        summary: applied.summary,
                        git_diff: applied.git_diff,
                    },
                    Err(err) => FindingOutcome {
                        repository: finding.repository.clone(),
                        finding: finding.kind,
                        remediation: RemediationType::Deterministic,
                        success: false,
                        diff_generated: false,
                        tests_passed: None,
                        summary: err.summary,
                        git_diff: None,
                    },
                }
            }
            DispatchDecision::Ai(kind) => self.remediate_ai(runtime, finding, kind.as_str()).await,
        }
    }

    async fn remediate_ai<R>(&self, runtime: R, finding: &Finding, kind: &str) -> FindingOutcome
    where
        R: Runtime,
    {
        if self.config.dry_run {
            return FindingOutcome {
                repository: finding.repository.clone(),
                finding: finding.kind,
                remediation: RemediationType::Ai,
                success: true,
                diff_generated: false,
                tests_passed: None,
                summary: format!("dry_run: would invoke {kind}"),
                git_diff: None,
            };
        }
        if let Some(reason) = self.ai_blocked_reason(finding) {
            return FindingOutcome {
                repository: finding.repository.clone(),
                finding: finding.kind,
                remediation: RemediationType::Ai,
                success: false,
                diff_generated: false,
                tests_passed: None,
                summary: reason,
                git_diff: None,
            };
        }
        self.mark_ai_repo(&finding.repository);

        let workflow = IssueImplementationWorkflow::new(runtime, self.ai.clone());
        let request = IssueImplementationRequest {
            issue: Issue::new(
                format!("{} in {}", finding.kind, finding.repository),
                issue_body(finding),
            ),
            repository_root: finding.repository_root.clone(),
        };
        match workflow.execute(request).await {
            Ok(result) => validate_ai_result(finding, result),
            Err(err) => FindingOutcome {
                repository: finding.repository.clone(),
                finding: finding.kind,
                remediation: RemediationType::Ai,
                success: false,
                diff_generated: false,
                tests_passed: None,
                summary: format!("workflow failed: {}", err.kind()),
                git_diff: None,
            },
        }
    }

    fn ai_blocked_reason(&self, finding: &Finding) -> Option<String> {
        if !self.config.routing_policy.is_private() {
            return Some("ai blocked: config routing_policy is not LocalOnly/Private".into());
        }
        if !self.ai.config().routing_policy().is_private() {
            return Some("ai blocked: AiClient routing_policy is not LocalOnly/Private".into());
        }
        let used = self.ai_repos_used.lock().expect("hygiene ai budget mutex");
        if used.contains(&finding.repository) {
            return None;
        }
        if used.len() >= self.config.max_ai_repositories {
            return Some("ai blocked: max_ai_repositories".into());
        }
        None
    }

    fn mark_ai_repo(&self, repository: &str) {
        self.ai_repos_used
            .lock()
            .expect("hygiene ai budget mutex")
            .insert(repository.to_string());
    }
}

fn issue_body(finding: &Finding) -> String {
    let mut body = finding.description.clone();
    if !finding.suggested_remediation.is_empty() {
        body.push_str("\n\nSuggested remediation:\n");
        body.push_str(&finding.suggested_remediation);
    }
    body
}

fn report_only(finding: &Finding, summary: impl Into<String>) -> FindingOutcome {
    FindingOutcome {
        repository: finding.repository.clone(),
        finding: finding.kind,
        remediation: RemediationType::ReportOnly,
        success: true,
        diff_generated: false,
        tests_passed: None,
        summary: summary.into(),
        git_diff: None,
    }
}

fn validate_ai_result(finding: &Finding, result: IssueImplementationResult) -> FindingOutcome {
    let fmt_ok = result
        .command_results
        .first()
        .is_some_and(crate::runtime::CommandResult::is_success);
    let tests_passed = result.command_results.get(1).map(|c| c.is_success());
    let diff_generated = !result.git_diff.patch.trim().is_empty();
    let success = result.success && fmt_ok && tests_passed.unwrap_or(false) && diff_generated;
    let summary = if success {
        result.summary
    } else if !fmt_ok {
        "validation failed: cargo fmt".into()
    } else if tests_passed != Some(true) {
        "validation failed: cargo test".into()
    } else if !diff_generated {
        "validation failed: empty git diff".into()
    } else {
        result.summary
    };
    FindingOutcome {
        repository: finding.repository.clone(),
        finding: finding.kind,
        remediation: RemediationType::Ai,
        success,
        diff_generated,
        tests_passed,
        summary,
        git_diff: Some(result.git_diff),
    }
}
