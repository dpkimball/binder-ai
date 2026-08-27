use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;

use crate::runtime::command::Command;
use crate::runtime::command_result::CommandResult;
use crate::runtime::errors::{Result, RuntimeError};
use crate::runtime::runtime::{GitDiff, GitStatus, GitStatusEntry, Runtime};
use crate::runtime::search::{SearchKind, SearchRequest};
use crate::runtime::search_result::SearchResult;

/// In-memory runtime for deterministic tests. No repository or process required.
#[derive(Debug)]
pub struct MemoryRuntime {
    files: Mutex<HashMap<PathBuf, String>>,
    commands: Mutex<HashMap<(String, Vec<String>), CommandResult>>,
    git_status: Mutex<GitStatus>,
    git_diff: Mutex<GitDiff>,
    search_calls: AtomicU32,
    read_calls: AtomicU32,
    write_calls: AtomicU32,
    executed: Mutex<Vec<(String, Vec<String>)>>,
}

impl Default for MemoryRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryRuntime {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
            commands: Mutex::new(HashMap::new()),
            git_status: Mutex::new(GitStatus {
                branch: "master".into(),
                entries: Vec::new(),
            }),
            git_diff: Mutex::new(GitDiff {
                branch: "master".into(),
                patch: String::new(),
            }),
            search_calls: AtomicU32::new(0),
            read_calls: AtomicU32::new(0),
            write_calls: AtomicU32::new(0),
            executed: Mutex::new(Vec::new()),
        }
    }

    pub fn with_file(self, path: impl AsRef<Path>, contents: impl Into<String>) -> Self {
        self.files
            .lock()
            .expect("memory runtime mutex")
            .insert(normalize_key(path.as_ref()), contents.into());
        self
    }

    pub fn script(self, command: Command, result: CommandResult) -> Self {
        self.commands.lock().expect("memory runtime mutex").insert(
            (
                command.executable().to_string(),
                command.arguments().to_vec(),
            ),
            result,
        );
        self
    }

    pub fn with_git_status(self, status: GitStatus) -> Self {
        *self.git_status.lock().expect("memory runtime mutex") = status;
        self
    }

    pub fn with_git_diff(self, diff: GitDiff) -> Self {
        *self.git_diff.lock().expect("memory runtime mutex") = diff;
        self
    }

    pub fn set_git_status(&self, entries: Vec<GitStatusEntry>) {
        self.git_status
            .lock()
            .expect("memory runtime mutex")
            .entries = entries;
    }

    pub fn set_git_diff(&self, patch: impl Into<String>) {
        self.git_diff.lock().expect("memory runtime mutex").patch = patch.into();
    }

    pub fn files(&self) -> HashMap<PathBuf, String> {
        self.files.lock().expect("memory runtime mutex").clone()
    }

    pub fn search_calls(&self) -> u32 {
        self.search_calls.load(Ordering::SeqCst)
    }

    pub fn read_calls(&self) -> u32 {
        self.read_calls.load(Ordering::SeqCst)
    }

    pub fn write_calls(&self) -> u32 {
        self.write_calls.load(Ordering::SeqCst)
    }

    pub fn executed_commands(&self) -> Vec<(String, Vec<String>)> {
        self.executed.lock().expect("memory runtime mutex").clone()
    }
}

#[async_trait]
impl Runtime for MemoryRuntime {
    async fn read_file(&self, path: &Path) -> Result<String> {
        self.read_calls.fetch_add(1, Ordering::SeqCst);
        let key = normalize_key(path);
        self.files
            .lock()
            .expect("memory runtime mutex")
            .get(&key)
            .cloned()
            .ok_or_else(|| RuntimeError::NotFound {
                path: key.display().to_string(),
            })
    }

    async fn write_file(&self, path: &Path, contents: &str) -> Result<()> {
        self.write_calls.fetch_add(1, Ordering::SeqCst);
        if escapes_workspace(path) {
            return Err(RuntimeError::PathEscape {
                path: path.display().to_string(),
            });
        }
        self.files
            .lock()
            .expect("memory runtime mutex")
            .insert(normalize_key(path), contents.to_string());
        Ok(())
    }

    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        self.search_calls.fetch_add(1, Ordering::SeqCst);
        let files = self.files.lock().expect("memory runtime mutex").clone();
        let mut results = Vec::new();
        for (path, contents) in files {
            let relative = path.to_string_lossy().replace('\\', "/");
            if !request.matches_path(&relative) {
                continue;
            }
            match request.kind {
                SearchKind::Filename => {
                    if relative.contains(&request.query) {
                        results.push(SearchResult::file(path));
                    }
                }
                SearchKind::Content => {
                    for (idx, line) in contents.lines().enumerate() {
                        if line.contains(&request.query) {
                            results.push(SearchResult {
                                path: path.clone(),
                                line: Some((idx + 1) as u32),
                                preview: Some(line.trim().to_string()),
                            });
                            if request.max_results.is_some_and(|max| results.len() >= max) {
                                return Ok(results);
                            }
                        }
                    }
                }
            }
            if request.max_results.is_some_and(|max| results.len() >= max) {
                break;
            }
        }
        results.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        Ok(results)
    }

    async fn execute(&self, command: Command) -> Result<CommandResult> {
        let started = Instant::now();
        self.executed.lock().expect("memory runtime mutex").push((
            command.executable().to_string(),
            command.arguments().to_vec(),
        ));
        let key = (
            command.executable().to_string(),
            command.arguments().to_vec(),
        );
        let mut result = self
            .commands
            .lock()
            .expect("memory runtime mutex")
            .get(&key)
            .cloned()
            .ok_or_else(|| RuntimeError::Unsupported {
                message: format!(
                    "no scripted result for {} {:?}",
                    command.executable(),
                    command.arguments()
                ),
            })?;
        result.duration = started.elapsed();
        Ok(result)
    }

    async fn git_status(&self) -> Result<GitStatus> {
        Ok(self
            .git_status
            .lock()
            .expect("memory runtime mutex")
            .clone())
    }

    async fn git_diff(&self) -> Result<GitDiff> {
        Ok(self.git_diff.lock().expect("memory runtime mutex").clone())
    }
}

fn normalize_key(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {}
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::Normal(part) => out.push(part),
        }
    }
    out
}

fn escapes_workspace(path: &Path) -> bool {
    let mut depth: i32 = 0;
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => depth -= 1,
            std::path::Component::Normal(_) => depth += 1,
            _ => {}
        }
        if depth < 0 {
            return true;
        }
    }
    path.is_absolute()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::runtime::GitFileStatus;

    #[tokio::test]
    async fn memory_roundtrip_search_and_git() {
        let runtime = MemoryRuntime::new()
            .with_file(
                "src/lib.rs",
                "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
            )
            .with_file("README.md", "hello\n")
            .script(
                Command::new("cargo").arg("test"),
                CommandResult::success("ok"),
            )
            .with_git_diff(GitDiff {
                branch: "feature/runtime".into(),
                patch: "diff --git a/src/lib.rs b/src/lib.rs\n".into(),
            })
            .with_git_status(GitStatus {
                branch: "feature/runtime".into(),
                entries: vec![GitStatusEntry {
                    path: PathBuf::from("src/lib.rs"),
                    status: GitFileStatus::Modified,
                }],
            });

        let hits = runtime
            .search(SearchRequest::content("fn add").extension("rs"))
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, Some(1));

        runtime
            .write_file(
                Path::new("src/lib.rs"),
                "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
            )
            .await
            .unwrap();
        let cargo = runtime
            .execute(Command::new("cargo").arg("test"))
            .await
            .unwrap();
        assert!(cargo.is_success());
        assert_eq!(
            runtime.git_status().await.unwrap().branch,
            "feature/runtime"
        );
        assert!(runtime
            .git_diff()
            .await
            .unwrap()
            .patch
            .contains("src/lib.rs"));
    }

    #[tokio::test]
    async fn memory_rejects_path_escape() {
        let runtime = MemoryRuntime::new();
        let err = runtime
            .write_file(Path::new("../outside.rs"), "x")
            .await
            .unwrap_err();
        assert_eq!(err.kind(), "path_escape");
    }
}
