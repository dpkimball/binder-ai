use std::path::PathBuf;
use std::time::Instant;

use crate::client::AiClient;
use crate::runtime::{Runtime, SearchKind, SearchRequest};
use crate::workflows::issue::implementer;
use crate::workflows::issue::planner::{self, ImplementationPlan};
use crate::workflows::issue::request::IssueImplementationRequest;
use crate::workflows::issue::result::{IssueImplementationResult, WorkflowError};
use crate::workflows::issue::validator;

const MAX_FILES: usize = 8;

/// Concrete issue → plan → edit → fmt → test → diff. Not a generic engine.
pub struct IssueImplementationWorkflow<R> {
    runtime: R,
    ai: AiClient,
}

impl<R: Runtime> IssueImplementationWorkflow<R> {
    pub fn new(runtime: R, ai: AiClient) -> Self {
        Self { runtime, ai }
    }

    pub async fn execute(
        &self,
        request: IssueImplementationRequest,
    ) -> Result<IssueImplementationResult, WorkflowError> {
        let files = phase("search", async { self.search_and_read(&request).await }).await?;
        let plan = phase("planning", async {
            planner::plan(&self.ai, &request.issue, &files).await
        })
        .await?;
        let generated = phase("implementation", async {
            implementer::implement(&self.ai, &request.issue, &plan, &files).await
        })
        .await?;

        let mut modified = Vec::new();
        for edit in &generated.edits {
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

        let success = !modified.is_empty() && fmt.is_success() && test.is_success();
        Ok(IssueImplementationResult {
            success,
            modified_files: modified,
            command_results: vec![fmt, test],
            git_diff,
            summary: summary_text(&plan, &generated.summary),
        })
    }

    async fn search_and_read(
        &self,
        request: &IssueImplementationRequest,
    ) -> Result<Vec<(PathBuf, String)>, WorkflowError> {
        let terms = planner::search_terms(&request.issue);
        let mut ordered: Vec<PathBuf> = Vec::new();
        for term in &terms {
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
            let filename = SearchRequest::filename(term.clone()).exclude("target");
            push_hits(
                &mut ordered,
                self.runtime
                    .search(filename)
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
                        phase = "search",
                        path = %path.display(),
                        error_kind = err.kind(),
                        "workflow.read_skip"
                    );
                }
            }
        }
        Ok(files)
    }
}

fn push_hits(ordered: &mut Vec<PathBuf>, hits: Vec<crate::runtime::SearchResult>) {
    for hit in hits {
        if !ordered.iter().any(|p| p == &hit.path) {
            ordered.push(hit.path);
        }
    }
}

fn summary_text(plan: &ImplementationPlan, implementation: &str) -> String {
    if implementation.is_empty() {
        plan.summary.clone()
    } else {
        implementation.to_string()
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
        "workflow.phase"
    );
    result
}
