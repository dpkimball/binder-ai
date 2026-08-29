use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::knowledge::{ExecutionRecord, Knowledge};
use crate::runtime::{Runtime, SearchKind, SearchRequest};
use crate::workflows::documentation::request::DocumentationReviewRequest;
use crate::workflows::documentation::result::{DocumentationGap, DocumentationReviewReport};
use crate::workflows::issue::WorkflowError;
use crate::workspace::{resolve_inspection_set_best_effort, WorkspaceLayout};

const WELL_KNOWN: &[&str] = &[
    "README.md",
    "README",
    "docs/README.md",
    "docs/as-built/architecture.md",
    "docs/as-built/mermaid.md",
];

const THIN_README_CHARS: usize = 80;

/// Bindb-first documentation review. No LLM. Does not mutate the tree.
pub struct DocumentationReviewWorkflow<R, K> {
    runtime: R,
    knowledge: K,
    layout: WorkspaceLayout,
}

impl<R: Runtime, K: Knowledge> DocumentationReviewWorkflow<R, K> {
    pub fn new(runtime: R, knowledge: K) -> Self {
        Self {
            runtime,
            knowledge,
            layout: WorkspaceLayout::passthrough(),
        }
    }

    pub fn with_workspace(mut self, layout: WorkspaceLayout) -> Self {
        self.layout = layout;
        self
    }

    pub async fn execute(
        &self,
        request: DocumentationReviewRequest,
    ) -> Result<DocumentationReviewReport, WorkflowError> {
        let plan = phase("knowledge_plan", async {
            self.knowledge
                .plan(&request.request)
                .map_err(WorkflowError::from_knowledge)
        })
        .await?;

        let mut ordered: Vec<PathBuf> = Vec::new();
        let mut seen = BTreeSet::new();
        let resolution = resolve_inspection_set_best_effort(&self.layout, &plan.inspection_set);
        if !resolution.resolution.binds.is_empty() {
            self.knowledge
                .bind_execution_workspace(resolution.resolution.binds.clone())
                .map_err(WorkflowError::from_knowledge)?;
        }
        for path in resolution.resolution.execution_paths() {
            push_unique(&mut ordered, &mut seen, path);
        }
        for path in &plan.inspection_set {
            push_unique(&mut ordered, &mut seen, path.clone());
        }
        ordered.retain(|path| is_documentation_path(path));
        for known in WELL_KNOWN {
            push_unique(&mut ordered, &mut seen, PathBuf::from(known));
        }

        let mut files: Vec<(PathBuf, String)> = Vec::new();
        for path in &ordered {
            if let Ok(contents) = self.runtime.read_file(path).await {
                files.push((path.clone(), contents));
            }
        }

        let mut search_used = false;
        if !has_readme(&files) {
            search_used = true;
            let hits = phase("discover_readme", async {
                self.runtime
                    .search(SearchRequest {
                        query: "README".into(),
                        kind: SearchKind::Filename,
                        glob: None,
                        extensions: Vec::new(),
                        exclusions: vec!["target".into(), "node_modules".into()],
                        max_results: Some(8),
                    })
                    .await
                    .map_err(|err| WorkflowError::from_runtime_search(&err))
            })
            .await?;
            for hit in hits {
                if !is_documentation_path(&hit.path) {
                    continue;
                }
                if files.iter().any(|(p, _)| p == &hit.path) {
                    continue;
                }
                if let Ok(contents) = self.runtime.read_file(&hit.path).await {
                    files.push((hit.path, contents));
                }
            }
        }

        let gaps = gaps_from(&files);
        let files_inspected: Vec<String> = files
            .iter()
            .map(|(path, _)| path.display().to_string())
            .collect();
        let summary = summarize(&request, &files_inspected, &gaps);
        let files_opened: Vec<PathBuf> = files.iter().map(|(path, _)| path.clone()).collect();
        let used_bindb_docs = plan.inspection_set.iter().any(|path| {
            is_documentation_path(path)
                && files_opened
                    .iter()
                    .any(|opened| opened == path || opened.ends_with(path))
        });
        let primary_source = if used_bindb_docs {
            "bindb"
        } else {
            "discovery"
        };

        let record = ExecutionRecord {
            workflow: "documentation_review".into(),
            request: request.request.clone(),
            inspection_set: plan.inspection_set.clone(),
            files_opened: files_opened.clone(),
            files_modified: Vec::new(),
            llm_invoked: false,
            validation_passed: true,
        };
        phase("history", async {
            self.knowledge
                .record_execution(record)
                .map_err(WorkflowError::from_knowledge)
        })
        .await?;

        Ok(DocumentationReviewReport {
            success: true,
            workflow: "documentation_review",
            llm_invoked: false,
            validation_passed: true,
            history_recorded: true,
            search_used,
            primary_source,
            files_inspected,
            gaps,
            summary,
        })
    }
}

fn push_unique(ordered: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>, path: PathBuf) {
    let key = normalize(&path);
    if seen.insert(key) {
        ordered.push(path);
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
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

fn is_documentation_path(path: &Path) -> bool {
    let rendered = path.to_string_lossy().replace('\\', "/");
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.starts_with("readme")
        || name.starts_with("changelog")
        || name.starts_with("contributing")
    {
        return true;
    }
    let in_docs = rendered.contains("/docs/") || rendered.starts_with("docs/");
    in_docs && is_doc_filename(&name)
}

fn is_doc_filename(name: &str) -> bool {
    name.ends_with(".md")
        || name.ends_with(".rst")
        || name.ends_with(".adoc")
        || name.ends_with(".txt")
        || !name.contains('.')
}

fn has_readme(files: &[(PathBuf, String)]) -> bool {
    files.iter().any(|(path, _)| {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(|name| name.to_ascii_lowercase().starts_with("readme"))
            .unwrap_or(false)
    })
}

fn gaps_from(files: &[(PathBuf, String)]) -> Vec<DocumentationGap> {
    let mut gaps = Vec::new();
    if !has_readme(files) {
        gaps.push(DocumentationGap {
            kind: "missing_readme".into(),
            summary: "no README found".into(),
        });
        return gaps;
    }
    let readme = files.iter().find(|(path, _)| {
        path.file_name()
            .and_then(|value| value.to_str())
            .map(|name| name.to_ascii_lowercase().starts_with("readme"))
            .unwrap_or(false)
    });
    if let Some((path, body)) = readme {
        let chars = body.trim().chars().count();
        if chars < THIN_README_CHARS {
            gaps.push(DocumentationGap {
                kind: "thin_readme".into(),
                summary: format!(
                    "{} is {chars} characters (below {THIN_README_CHARS})",
                    path.display()
                ),
            });
        }
    }
    gaps
}

fn summarize(
    request: &DocumentationReviewRequest,
    files: &[String],
    gaps: &[DocumentationGap],
) -> String {
    let repo = if request.repository.is_empty() {
        "repository"
    } else {
        request.repository.as_str()
    };
    let finding = request
        .finding_kind
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("documentation");
    if files.is_empty() && gaps.is_empty() {
        return format!("{repo}: no documentation files inspected ({finding})");
    }
    if gaps.is_empty() {
        return format!(
            "{repo}: {} documentation file(s) inspected; no gaps ({finding})",
            files.len()
        );
    }
    let gap_text = gaps
        .iter()
        .map(|gap| gap.summary.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "{repo}: {} documentation file(s); {gap_text} ({finding})",
        files.len()
    )
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
        "documentation_review.phase"
    );
    result
}
