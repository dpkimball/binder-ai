use std::path::Path;

use async_trait::async_trait;

use crate::runtime::command::Command;
use crate::runtime::command_result::CommandResult;
use crate::runtime::errors::Result;
use crate::runtime::search::SearchRequest;
use crate::runtime::search_result::SearchResult;

/// Execution environment for AI workflows.
///
/// Implementations talk to a workspace (local disk, in-memory test store, later
/// Docker/SSH). They do not route models or own prompts.
#[async_trait]
pub trait Runtime: Send + Sync {
    async fn read_file(&self, path: &Path) -> Result<String>;

    async fn write_file(&self, path: &Path, contents: &str) -> Result<()>;

    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>>;

    async fn execute(&self, command: Command) -> Result<CommandResult>;

    async fn git_status(&self) -> Result<GitStatus>;

    async fn git_diff(&self) -> Result<GitDiff>;
}

/// Working-tree status plus the current branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: String,
    pub entries: Vec<GitStatusEntry>,
}

impl GitStatus {
    pub fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusEntry {
    pub path: std::path::PathBuf,
    pub status: GitFileStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
    Renamed,
}

/// Unified diff of the workspace against HEAD (empty if no commits / no changes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiff {
    pub branch: String,
    pub patch: String,
}

#[async_trait]
impl<T> Runtime for std::sync::Arc<T>
where
    T: Runtime + ?Sized,
{
    async fn read_file(&self, path: &Path) -> Result<String> {
        (**self).read_file(path).await
    }

    async fn write_file(&self, path: &Path, contents: &str) -> Result<()> {
        (**self).write_file(path, contents).await
    }

    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        (**self).search(request).await
    }

    async fn execute(&self, command: Command) -> Result<CommandResult> {
        (**self).execute(command).await
    }

    async fn git_status(&self) -> Result<GitStatus> {
        (**self).git_status().await
    }

    async fn git_diff(&self) -> Result<GitDiff> {
        (**self).git_diff().await
    }
}
