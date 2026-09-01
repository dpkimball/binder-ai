//! Facts from bindb. Not reasoning and not repository traversal.

mod codeintel;
mod domain;
mod memory;
mod record_ref;

pub use codeintel::{CodeintelCli, CodeintelKnowledge, ProcessCodeintelCli};
pub use domain::{parse_scoped_ask, DomainError, KnowledgeDomain, KnowledgeScope, ScopedAskBody};
pub use memory::MemoryKnowledge;
pub use record_ref::{
    decode_record_ref, encode_record_ref, engineering_question_uri, RecordRefError,
};

use std::path::PathBuf;

/// Bindb-backed architectural facts. Implementations must not call an LLM.
pub trait Knowledge: Send + Sync {
    fn plan(&self, request: &str) -> std::result::Result<KnowledgePlan, KnowledgeError>;
    /// Like `plan`, but compares against `since_sha` to populate `recent_changes`.
    /// Default falls back to `plan` (no drift data). Override in production impls.
    fn plan_since(
        &self,
        request: &str,
        _since_sha: &str,
    ) -> std::result::Result<KnowledgePlan, KnowledgeError> {
        self.plan(request)
    }
    fn doctrine(&self) -> std::result::Result<DoctrineSnapshot, KnowledgeError>;
    fn reindex(&self) -> std::result::Result<ReindexStats, KnowledgeError>;
    fn record_execution(&self, record: ExecutionRecord) -> std::result::Result<(), KnowledgeError>;
    fn execution_history(&self) -> std::result::Result<Vec<ExecutionRecord>, KnowledgeError>;
    /// Point re-index at the execution checkouts, not the paths used when indexing.
    fn bind_execution_workspace(
        &self,
        _binds: Vec<(String, PathBuf)>,
    ) -> std::result::Result<(), KnowledgeError> {
        Ok(())
    }
    /// Repo names present in the bindb store (`architecture summary`).
    fn indexed_repos(&self) -> std::result::Result<Vec<String>, KnowledgeError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("knowledge error: {message}")]
pub struct KnowledgeError {
    pub message: String,
}

impl KnowledgeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl<K: Knowledge + ?Sized> Knowledge for std::sync::Arc<K> {
    fn plan(&self, request: &str) -> std::result::Result<KnowledgePlan, KnowledgeError> {
        (**self).plan(request)
    }

    fn plan_since(
        &self,
        request: &str,
        since_sha: &str,
    ) -> std::result::Result<KnowledgePlan, KnowledgeError> {
        (**self).plan_since(request, since_sha)
    }

    fn doctrine(&self) -> std::result::Result<DoctrineSnapshot, KnowledgeError> {
        (**self).doctrine()
    }

    fn reindex(&self) -> std::result::Result<ReindexStats, KnowledgeError> {
        (**self).reindex()
    }

    fn record_execution(&self, record: ExecutionRecord) -> std::result::Result<(), KnowledgeError> {
        (**self).record_execution(record)
    }

    fn execution_history(&self) -> std::result::Result<Vec<ExecutionRecord>, KnowledgeError> {
        (**self).execution_history()
    }

    fn bind_execution_workspace(
        &self,
        binds: Vec<(String, PathBuf)>,
    ) -> std::result::Result<(), KnowledgeError> {
        (**self).bind_execution_workspace(binds)
    }

    fn indexed_repos(&self) -> std::result::Result<Vec<String>, KnowledgeError> {
        (**self).indexed_repos()
    }
}

/// One file from the inspection set that changed between `since_sha` and HEAD.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentChange {
    pub path: String,
    pub repo: String,
    /// "added" or "removed"
    pub change: String,
}

/// Linked issue with a body excerpt for LLM context.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkedIssueSummary {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub repo: String,
    /// First 1 000 chars of the issue body.
    pub body_excerpt: String,
}

/// Linked PR (title + state; no body).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkedPrSummary {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub repo: String,
}

/// Linked doc file (CLAUDE.md, spec, Helm chart, CI workflow).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LinkedDocSummary {
    pub path: String,
    pub title: String,
    pub repo: String,
    pub kind: String,
}

/// Per-file role classification relative to the matched symbol.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileRoleSummary {
    pub path: String,
    /// "definition", "call_site", "implementation", "test", "entry_point", "dependency"
    pub role: String,
    pub note: String,
}

/// One ordered change site for structural task patterns.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TouchPointSummary {
    pub file: String,
    pub role: String,
    pub action: String,
}

/// Pre-authored Q&A guidance item surfaced by plan() when query tokens overlap.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GuidanceSummary {
    pub id: String,
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub match_score: u32,
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// Graph-derived inspection plan. Paths are payload strings from bindb, not file reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgePlan {
    pub request: String,
    pub interpretation: String,
    pub repositories: Vec<String>,
    pub inspection_set: Vec<PathBuf>,
    pub rules_must_remain_satisfied: Vec<String>,
    pub likely_affected: Vec<String>,
    pub primary_source: &'static str,
    /// Populated only when `plan_since` is used. Empty otherwise.
    pub recent_changes: Vec<RecentChange>,
    /// Top-matched issues with body excerpts for LLM explain context.
    pub linked_issues: Vec<LinkedIssueSummary>,
    /// Top-matched PRs.
    pub linked_prs: Vec<LinkedPrSummary>,
    /// Top-matched doc files (CLAUDE.md, specs, Helm, CI).
    pub linked_docs: Vec<LinkedDocSummary>,
    /// Issue numbers linked to inspection-set files via git log REL_RESOLVES edges.
    pub origin_issues: Vec<u64>,
    /// Impact granularity: "symbol", "module", "crate", "file", "config", "none".
    pub matched_kind: String,
    /// Per-file role classification relative to the matched symbol.
    pub file_roles: Vec<FileRoleSummary>,
    /// Ordered change sites for structural tasks (symbol/module matches only).
    pub touch_sequence: Vec<TouchPointSummary>,
    /// One-sentence summary of definition and top call sites.
    pub matched_summary: String,
    /// Advice when touch_sequence is non-empty.
    pub task_hint: String,
    /// Pre-authored Q&A pairs from the question bank. Empty when no questions matched.
    pub guidance: Vec<GuidanceSummary>,
}

/// Doctrine evaluation as stored in bindb (finding counts, not source).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctrineSnapshot {
    pub repository: String,
    pub doctrine: String,
    pub findings: usize,
    pub rules_evaluated: usize,
    pub primary_source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReindexStats {
    pub repositories: Vec<String>,
    pub skipped: bool,
}

/// One completed (or attempted) agent workflow, stored as bindb fact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionRecord {
    pub workflow: String,
    pub request: String,
    pub inspection_set: Vec<PathBuf>,
    pub files_opened: Vec<PathBuf>,
    pub files_modified: Vec<PathBuf>,
    pub llm_invoked: bool,
    pub validation_passed: bool,
}
