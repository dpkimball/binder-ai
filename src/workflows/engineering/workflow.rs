use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::client::AiClient;
use crate::intel::{plan as bindb_plan, PlanRequest};
use crate::knowledge::Knowledge;
use crate::runtime::{Runtime, SearchKind, SearchRequest};
use crate::workflows::engineering::request::EngineeringRequest;
use crate::workflows::engineering::result::EngineeringReport;
use crate::workflows::issue::implementer;
use crate::workflows::issue::planner;
use crate::workflows::issue::validator;
use crate::workflows::issue::{Issue, WorkflowError};

const MAX_FILES: usize = 8;

/// Bindb-first loop: knowledge plan → inspect set or explicit discovery fallback → LLM → Runtime.
///
/// Repository search runs only when the planner sets `fallback_required`.
pub struct EngineeringWorkflow<R, K> {
    runtime: R,
    ai: AiClient,
    knowledge: K,
}

impl<R: Runtime, K: Knowledge> EngineeringWorkflow<R, K> {
    pub fn new(runtime: R, ai: AiClient, knowledge: K) -> Self {
        Self {
            runtime,
            ai,
            knowledge,
        }
    }

    pub async fn execute(
        &self,
        request: EngineeringRequest,
    ) -> Result<EngineeringReport, WorkflowError> {
        let doctrine_before = phase("doctrine_before", async {
            self.knowledge
                .doctrine()
                .map_err(WorkflowError::from_knowledge)
        })
        .await?;

        let mut plan = phase("knowledge_plan", async {
            bindb_plan(
                &self.knowledge,
                &PlanRequest {
                    workspace: Some(request.repository_root.clone()),
                    request: request.request.clone(),
                    since_sha: None,
                },
            )
            .map_err(|err| WorkflowError::KnowledgeFailed {
                message: err.to_string(),
            })
        })
        .await?;

        let (files, search_performed) = if plan.fallback_required {
            tracing::info!(
                fallback_reason = ?plan.fallback_reason,
                planning_result = ?plan.planning_result,
                "engineering.repository_discovery"
            );
            (
                phase("discover", async {
                    self.search_and_read(&request.request).await
                })
                .await?,
                true,
            )
        } else {
            let paths = inspect_paths(&plan);
            (
                phase("inspect", async { self.read_inspection_set(&paths).await }).await?,
                false,
            )
        };

        let files_opened_from_inspection_set = if search_performed { 0 } else { files.len() };
        plan = plan.with_acquisition(files_opened_from_inspection_set, search_performed);
        tracing::info!(
            planning_source = plan.planning_source,
            planning_result = ?plan.planning_result,
            fallback_required = plan.fallback_required,
            fallback_reason = ?plan.fallback_reason,
            inspection_set_size = plan.metrics.inspection_set_size,
            files_opened_from_inspection_set,
            repository_search_performed = search_performed,
            "intel.planning.acquire"
        );

        let allowed: BTreeSet<PathBuf> = files.iter().map(|(p, _)| normalize(p)).collect();
        let files_opened: Vec<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
        let inspection_set: Vec<PathBuf> = if search_performed {
            files_opened.clone()
        } else {
            plan.inspection_set.iter().map(PathBuf::from).collect()
        };

        let issue = Issue::new("engineering request", &request.request);
        let llm_plan = phase("reasoning_plan", async {
            planner::plan(&self.ai, &issue, &files).await
        })
        .await?;
        let generated = phase("reasoning_implement", async {
            implementer::implement(&self.ai, &issue, &llm_plan, &files).await
        })
        .await?;

        let mut modified = Vec::new();
        let mut opened_outside = Vec::new();
        for edit in &generated.edits {
            let path = normalize(&edit.path);
            if !allowed.contains(&path) {
                opened_outside.push(path);
                continue;
            }
            self.runtime
                .write_file(&edit.path, &edit.contents)
                .await
                .map_err(|err| WorkflowError::from_runtime_write(&err))?;
            modified.push(edit.path.clone());
        }

        let fmt = phase("formatting", async {
            validator::cargo_fmt(&self.runtime, &request.repository_root).await
        })
        .await?;
        let test = phase("testing", async {
            validator::cargo_test(&self.runtime, &request.repository_root).await
        })
        .await?;
        let git_diff = phase("git_diff", async {
            self.runtime
                .git_diff()
                .await
                .map_err(|err| WorkflowError::from_runtime_git(&err))
        })
        .await?;

        let success = !modified.is_empty()
            && opened_outside.is_empty()
            && fmt.is_success()
            && test.is_success();

        let reindex = if success {
            Some(
                phase("reindex", async {
                    self.knowledge
                        .reindex()
                        .map_err(WorkflowError::from_knowledge)
                })
                .await?,
            )
        } else {
            None
        };

        let doctrine_after = phase("doctrine_after", async {
            self.knowledge
                .doctrine()
                .map_err(WorkflowError::from_knowledge)
        })
        .await?;

        let summary = if generated.summary.is_empty() {
            llm_plan.summary.clone()
        } else {
            generated.summary.clone()
        };

        Ok(EngineeringReport {
            success,
            inspection_set,
            files_opened,
            files_modified: modified,
            opened_outside_inspection_set: opened_outside,
            llm_invoked: true,
            doctrine_before,
            doctrine_after,
            reindex,
            command_results: vec![fmt, test],
            git_diff,
            summary,
            primary_source: "bindb",
            planning: plan.metrics,
        })
    }

    async fn read_inspection_set(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<(PathBuf, String)>, WorkflowError> {
        let mut files = Vec::new();
        for path in paths {
            match self.runtime.read_file(path).await {
                Ok(contents) => files.push((path.clone(), contents)),
                Err(err) => {
                    tracing::info!(
                        phase = "inspect",
                        path = %path.display(),
                        error_kind = err.kind(),
                        "engineering.read_skip"
                    );
                }
            }
        }
        if files.is_empty() {
            return Err(WorkflowError::KnowledgeFailed {
                message: "inspection set files were not readable".into(),
            });
        }
        Ok(files)
    }

    async fn search_and_read(
        &self,
        request: &str,
    ) -> Result<Vec<(PathBuf, String)>, WorkflowError> {
        let mut ordered: Vec<PathBuf> = Vec::new();
        for term in discovery_terms(request) {
            let content = SearchRequest {
                query: term.clone(),
                kind: SearchKind::Content,
                glob: None,
                extensions: vec!["rs".into()],
                exclusions: vec!["target".into()],
                max_results: Some(20),
            };
            push_hits(
                &mut ordered,
                self.runtime
                    .search(content)
                    .await
                    .map_err(|err| WorkflowError::from_runtime_search(&err))?,
            );
            if ordered.len() >= MAX_FILES {
                break;
            }
        }
        ordered.truncate(MAX_FILES);

        let mut files = Vec::new();
        for path in ordered {
            match self.runtime.read_file(&path).await {
                Ok(contents) => files.push((path, contents)),
                Err(err) => {
                    tracing::info!(
                        phase = "discover",
                        path = %path.display(),
                        error_kind = err.kind(),
                        "engineering.read_skip"
                    );
                }
            }
        }
        if files.is_empty() {
            return Err(WorkflowError::SearchFailed {
                message: "repository discovery found no files after planner fallback".into(),
            });
        }
        Ok(files)
    }
}

fn inspect_paths(plan: &crate::intel::PlanResponse) -> Vec<PathBuf> {
    if !plan.resolved_execution_paths.is_empty() {
        return plan
            .resolved_execution_paths
            .iter()
            .map(PathBuf::from)
            .collect();
    }
    plan.inspection_set.iter().map(PathBuf::from).collect()
}

fn discovery_terms(request: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in request.split(|c: char| !c.is_alphanumeric()) {
        let token = raw.trim();
        if token.len() < 3 {
            continue;
        }
        let key = token.to_lowercase();
        if seen.insert(key) {
            terms.push(token.to_string());
        }
    }
    if terms.is_empty() {
        terms.push(request.trim().to_string());
    }
    terms
}

fn push_hits(ordered: &mut Vec<PathBuf>, hits: Vec<crate::runtime::SearchResult>) {
    for hit in hits {
        if !ordered.iter().any(|p| p == &hit.path) {
            ordered.push(hit.path);
        }
    }
}

fn normalize(path: impl AsRef<Path>) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {}
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::Normal(s) => out.push(s),
        }
    }
    out
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
        "engineering.phase"
    );
    result
}
