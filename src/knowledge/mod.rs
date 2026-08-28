//! Facts from bindb. Not reasoning and not repository traversal.

mod memory;

pub use memory::MemoryKnowledge;

use std::path::PathBuf;

/// Bindb-backed architectural facts. Implementations must not call an LLM.
pub trait Knowledge: Send + Sync {
    fn plan(&self, request: &str) -> std::result::Result<KnowledgePlan, KnowledgeError>;
    fn doctrine(&self) -> std::result::Result<DoctrineSnapshot, KnowledgeError>;
    fn reindex(&self) -> std::result::Result<ReindexStats, KnowledgeError>;
    fn record_execution(&self, record: ExecutionRecord) -> std::result::Result<(), KnowledgeError>;
    fn execution_history(&self) -> std::result::Result<Vec<ExecutionRecord>, KnowledgeError>;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRecord {
    pub workflow: String,
    pub request: String,
    pub inspection_set: Vec<PathBuf>,
    pub files_opened: Vec<PathBuf>,
    pub files_modified: Vec<PathBuf>,
    pub llm_invoked: bool,
    pub validation_passed: bool,
}
