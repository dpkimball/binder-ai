use std::path::PathBuf;

use crate::intel::PlanningMetrics;
use crate::knowledge::{DoctrineSnapshot, ReindexStats};
use crate::runtime::{CommandResult, GitDiff};

/// Outcome of [`super::EngineeringWorkflow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineeringReport {
    pub success: bool,
    pub inspection_set: Vec<PathBuf>,
    pub files_opened: Vec<PathBuf>,
    pub files_modified: Vec<PathBuf>,
    pub opened_outside_inspection_set: Vec<PathBuf>,
    pub llm_invoked: bool,
    pub doctrine_before: DoctrineSnapshot,
    pub doctrine_after: DoctrineSnapshot,
    pub reindex: Option<ReindexStats>,
    pub command_results: Vec<CommandResult>,
    pub git_diff: GitDiff,
    pub summary: String,
    pub primary_source: &'static str,
    pub planning: PlanningMetrics,
}
