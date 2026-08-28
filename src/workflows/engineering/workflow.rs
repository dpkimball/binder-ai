use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::client::AiClient;
use crate::knowledge::Knowledge;
use crate::runtime::Runtime;
use crate::workflows::engineering::request::EngineeringRequest;
use crate::workflows::engineering::result::EngineeringReport;
use crate::workflows::issue::implementer;
use crate::workflows::issue::planner;
use crate::workflows::issue::validator;
use crate::workflows::issue::{Issue, WorkflowError};

/// Bindb-first loop: knowledge → curated LLM context → Runtime → re-index.
///
/// Does not search the repository. Inspection paths come from [`Knowledge`].
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

        let plan = phase("knowledge_plan", async {
            self.knowledge
                .plan(&request.request)
                .map_err(WorkflowError::from_knowledge)
        })
        .await?;
        if plan.primary_source != "bindb" {
            return Err(WorkflowError::KnowledgeFailed {
                message: "knowledge plan must declare primary_source bindb".into(),
            });
        }
        if plan.inspection_set.is_empty() {
            return Err(WorkflowError::KnowledgeFailed {
                message: "empty inspection set; refusing to invoke reasoning without bindb context"
                    .into(),
            });
        }

        let allowed: BTreeSet<PathBuf> = plan.inspection_set.iter().map(normalize).collect();
        let files = phase("inspect", async {
            self.read_inspection_set(&plan.inspection_set).await
        })
        .await?;
        let files_opened: Vec<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();

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
            inspection_set: plan.inspection_set,
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
