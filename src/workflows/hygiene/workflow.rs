use std::path::PathBuf;
use std::time::Instant;

use crate::client::AiClient;
use crate::knowledge::{DoctrineSnapshot, ExecutionRecord, Knowledge, ReindexStats};
use crate::runtime::Runtime;
use crate::workflows::hygiene::config::HygieneConfig;
use crate::workflows::hygiene::dispatcher;
use crate::workflows::hygiene::engine::HygieneEngine;
use crate::workflows::hygiene::finding::Finding;
use crate::workflows::hygiene::remediation::{DispatchDecision, RemediationType};
use crate::workflows::hygiene::result::HygieneReport;
use crate::workflows::issue::WorkflowError;

/// Hygiene request. Inspection paths come from bindb, not from this struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneWorkflowRequest {
    pub request: String,
    pub findings: Vec<Finding>,
}

/// Outcome of [`HygieneWorkflow`]. Metrics prove bindb-first execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneWorkflowReport {
    pub success: bool,
    pub hygiene: HygieneReport,
    pub planning_ms: u64,
    pub inspection_set: Vec<PathBuf>,
    pub files_opened: Vec<PathBuf>,
    pub files_modified: Vec<PathBuf>,
    pub llm_invoked: bool,
    pub validation_passed: bool,
    pub reindex_ms: u64,
    pub doctrine_before: DoctrineSnapshot,
    pub doctrine_after: DoctrineSnapshot,
    pub reindex: Option<ReindexStats>,
    pub history_recorded: bool,
    pub search_used: bool,
    pub primary_source: &'static str,
}

/// Bindb-first Hygiene: plan → dispatch existing findings → skip LLM when deterministic → re-index.
///
/// Does not search the repository. Inspection paths come from [`Knowledge`].
pub struct HygieneWorkflow<R, K> {
    runtime: R,
    knowledge: K,
    engine: HygieneEngine,
}

impl<R: Runtime + Clone, K: Knowledge> HygieneWorkflow<R, K> {
    pub fn new(runtime: R, knowledge: K, config: HygieneConfig, ai: AiClient) -> Self {
        Self {
            runtime,
            knowledge,
            engine: HygieneEngine::new(config, ai),
        }
    }

    pub async fn execute(
        &self,
        request: HygieneWorkflowRequest,
    ) -> Result<HygieneWorkflowReport, WorkflowError> {
        let doctrine_before = phase("doctrine_before", async {
            self.knowledge
                .doctrine()
                .map_err(WorkflowError::from_knowledge)
        })
        .await?;

        let planning_started = Instant::now();
        let plan = phase("knowledge_plan", async {
            self.knowledge
                .plan(&request.request)
                .map_err(WorkflowError::from_knowledge)
        })
        .await?;
        let planning_ms = planning_started.elapsed().as_millis() as u64;

        if plan.primary_source != "bindb" {
            return Err(WorkflowError::KnowledgeFailed {
                message: "knowledge plan must declare primary_source bindb".into(),
            });
        }

        let needs_llm = request.findings.iter().any(|finding| {
            matches!(
                dispatcher::dispatch(finding, self.engine.config()),
                DispatchDecision::Ai(_)
            )
        });
        if needs_llm && plan.inspection_set.is_empty() {
            return Err(WorkflowError::KnowledgeFailed {
                message: "empty inspection set; refusing to invoke reasoning without bindb context"
                    .into(),
            });
        }

        let mut hygiene = HygieneReport::default();
        for finding in &request.findings {
            let outcome = self
                .engine
                .process_one_from_graph(self.runtime.clone(), finding, &plan.inspection_set)
                .await;
            tracing::info!(
                repository = %outcome.repository,
                finding = %outcome.finding,
                remediation = %outcome.remediation,
                success = outcome.success,
                "hygiene.workflow.finding"
            );
            hygiene.push(outcome);
        }

        let validation_passed = hygiene.outcomes.iter().all(|row| row.success);
        let llm_invoked = hygiene.outcomes.iter().any(|row| {
            row.remediation == RemediationType::Ai
                && !row.summary.starts_with("dry_run:")
                && !row.summary.starts_with("ai blocked:")
                && !row.summary.starts_with("empty inspection set")
        });

        let (reindex, reindex_ms) = if validation_passed {
            let started = Instant::now();
            let stats = phase("reindex", async {
                self.knowledge
                    .reindex()
                    .map_err(WorkflowError::from_knowledge)
            })
            .await?;
            (Some(stats), started.elapsed().as_millis() as u64)
        } else {
            (None, 0)
        };

        let doctrine_after = phase("doctrine_after", async {
            self.knowledge
                .doctrine()
                .map_err(WorkflowError::from_knowledge)
        })
        .await?;

        let files_opened = self.runtime.recorded_reads();
        let files_modified = self.runtime.recorded_writes();

        let mut history_recorded = false;
        if validation_passed {
            let record = ExecutionRecord {
                workflow: "hygiene".into(),
                request: request.request.clone(),
                inspection_set: plan.inspection_set.clone(),
                files_opened: files_opened.clone(),
                files_modified: files_modified.clone(),
                llm_invoked,
                validation_passed,
            };
            phase("record_execution", async {
                self.knowledge
                    .record_execution(record)
                    .map_err(WorkflowError::from_knowledge)
            })
            .await?;
            history_recorded = true;
        }

        Ok(HygieneWorkflowReport {
            success: validation_passed,
            hygiene,
            planning_ms,
            inspection_set: plan.inspection_set,
            files_opened,
            files_modified,
            llm_invoked,
            validation_passed,
            reindex_ms,
            doctrine_before,
            doctrine_after,
            reindex,
            history_recorded,
            search_used: false,
            primary_source: "bindb",
        })
    }
}

async fn phase<T, F>(name: &'static str, work: F) -> Result<T, WorkflowError>
where
    F: std::future::Future<Output = Result<T, WorkflowError>>,
{
    let started = Instant::now();
    let result = work.await;
    tracing::info!(
        phase = name,
        duration_ms = started.elapsed().as_millis() as u64,
        ok = result.is_ok(),
        "hygiene.workflow.phase"
    );
    result
}
