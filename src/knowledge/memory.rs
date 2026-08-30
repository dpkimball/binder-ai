use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::{
    DoctrineSnapshot, ExecutionRecord, Knowledge, KnowledgeError, KnowledgePlan, ReindexStats,
};

/// In-memory bindb stand-in for tests. Never walks a repository or calls an LLM.
#[derive(Debug)]
pub struct MemoryKnowledge {
    plan: KnowledgePlan,
    doctrine: DoctrineSnapshot,
    reindex_calls: AtomicU32,
    plan_calls: AtomicU32,
    history: Mutex<Vec<ExecutionRecord>>,
    execution_binds: Mutex<Vec<(String, PathBuf)>>,
}

impl MemoryKnowledge {
    pub fn new(plan: KnowledgePlan, doctrine: DoctrineSnapshot) -> Self {
        Self {
            plan,
            doctrine,
            reindex_calls: AtomicU32::new(0),
            plan_calls: AtomicU32::new(0),
            history: Mutex::new(Vec::new()),
            execution_binds: Mutex::new(Vec::new()),
        }
    }

    pub fn for_files(request: impl Into<String>, files: impl IntoIterator<Item = PathBuf>) -> Self {
        let request = request.into();
        let inspection_set: Vec<PathBuf> = files.into_iter().collect();
        Self::new(
            KnowledgePlan {
                request: request.clone(),
                interpretation: "memory knowledge".into(),
                repositories: vec!["fixture".into()],
                inspection_set,
                rules_must_remain_satisfied: vec!["workspace.acyclic-crates".into()],
                likely_affected: vec!["fixture".into()],
                primary_source: "bindb",
                recent_changes: Vec::new(),
                linked_issues: Vec::new(),
            },
            DoctrineSnapshot {
                repository: "fixture".into(),
                doctrine: "workspace".into(),
                findings: 0,
                rules_evaluated: 1,
                primary_source: "bindb",
            },
        )
    }

    pub fn reindex_calls(&self) -> u32 {
        self.reindex_calls.load(Ordering::SeqCst)
    }

    pub fn plan_calls(&self) -> u32 {
        self.plan_calls.load(Ordering::SeqCst)
    }

    pub fn execution_history(&self) -> Vec<ExecutionRecord> {
        self.history.lock().expect("memory knowledge mutex").clone()
    }

    pub fn execution_binds(&self) -> Vec<(String, PathBuf)> {
        self.execution_binds
            .lock()
            .expect("memory knowledge mutex")
            .clone()
    }
}

impl Knowledge for MemoryKnowledge {
    fn plan(&self, _request: &str) -> std::result::Result<KnowledgePlan, KnowledgeError> {
        self.plan_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.plan.clone())
    }

    fn doctrine(&self) -> std::result::Result<DoctrineSnapshot, KnowledgeError> {
        Ok(self.doctrine.clone())
    }

    fn reindex(&self) -> std::result::Result<ReindexStats, KnowledgeError> {
        self.reindex_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ReindexStats {
            repositories: self.plan.repositories.clone(),
            skipped: false,
        })
    }

    fn record_execution(&self, record: ExecutionRecord) -> std::result::Result<(), KnowledgeError> {
        self.history
            .lock()
            .expect("memory knowledge mutex")
            .push(record);
        Ok(())
    }

    fn execution_history(&self) -> std::result::Result<Vec<ExecutionRecord>, KnowledgeError> {
        Ok(self.history.lock().expect("memory knowledge mutex").clone())
    }

    fn bind_execution_workspace(
        &self,
        binds: Vec<(String, PathBuf)>,
    ) -> std::result::Result<(), KnowledgeError> {
        *self.execution_binds.lock().expect("memory knowledge mutex") = binds;
        Ok(())
    }
}
