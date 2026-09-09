use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
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
        let files = phase("inspect", async { self.inspect(&request).await }).await?;
        let plan = phase("planning", async {
            planner::plan(&self.ai, &request.issue, &files).await
        })
        .await?;
        let generated = phase("implementation", async {
            implementer::implement(&self.ai, &request.issue, &plan, &files).await
        })
        .await?;

        let allowed = request
            .inspection_set
            .as_ref()
            .map(|paths| paths.iter().map(normalize).collect::<BTreeSet<PathBuf>>());

        let mut modified = Vec::new();
        for edit in &generated.edits {
            if let Some(allowed) = &allowed {
                if !allowed.contains(&normalize(&edit.path)) {
                    return Err(WorkflowError::ImplementationFailed {
                        message: "edit outside inspection set".into(),
                    });
                }
            }
            self.runtime
                .write_file(&edit.path, &edit.contents)
                .await
                .map_err(|err| WorkflowError::from_runtime_write(&err))?;
            modified.push(edit.path.clone());
        }

        let rust_touched = modified
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "rs"));
        let fmt = if rust_touched {
            phase("formatting", async {
                validator::cargo_fmt(&self.runtime, &request.repository_root).await
            })
            .await?
        } else {
            crate::runtime::CommandResult::success("skipped: no rust edits")
        };
        let test = if rust_touched {
            phase("testing", async {
                validator::cargo_test(&self.runtime, &request.repository_root).await
            })
            .await?
        } else {
            crate::runtime::CommandResult::success("skipped: no rust edits")
        };
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

    async fn inspect(
        &self,
        request: &IssueImplementationRequest,
    ) -> Result<Vec<(PathBuf, String)>, WorkflowError> {
        if let Some(paths) = &request.inspection_set {
            return self.read_listed(paths).await;
        }
        self.search_and_read(request).await
    }

    async fn read_listed(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<(PathBuf, String)>, WorkflowError> {
        let mut files = Vec::new();
        for path in paths.iter().take(MAX_FILES) {
            match self.runtime.read_file(path).await {
                Ok(contents) => files.push((path.clone(), contents)),
                Err(err) => {
                    tracing::info!(
                        phase = "inspect",
                        path = %path.display(),
                        error_kind = err.kind(),
                        "workflow.read_skip"
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
        "workflow.phase"
    );
    result
}
