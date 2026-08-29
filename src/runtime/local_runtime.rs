use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use git2::{DiffFormat, DiffOptions, Repository, Status, StatusOptions};

use crate::runtime::command::Command;
use crate::runtime::command_result::CommandResult;
use crate::runtime::errors::{Result, RuntimeError};
use crate::runtime::runtime::{GitDiff, GitFileStatus, GitStatus, GitStatusEntry, Runtime};
use crate::runtime::search::{SearchKind, SearchRequest};
use crate::runtime::search_result::SearchResult;

/// Disk-backed runtime rooted at a workspace directory.
#[derive(Debug, Clone)]
pub struct LocalRuntime {
    root: PathBuf,
    reads: Arc<Mutex<Vec<PathBuf>>>,
    writes: Arc<Mutex<Vec<PathBuf>>>,
}

impl LocalRuntime {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = std::fs::canonicalize(root.as_ref()).map_err(RuntimeError::from)?;
        Ok(Self {
            root,
            reads: Arc::new(Mutex::new(Vec::new())),
            writes: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &Path) -> Result<PathBuf> {
        let joined = if path.is_absolute() {
            normalize(path)
        } else {
            normalize(&self.root.join(path))
        };
        let contained = std::fs::canonicalize(&joined).unwrap_or_else(|_| {
            joined
                .parent()
                .and_then(|parent| std::fs::canonicalize(parent).ok())
                .map(|parent| parent.join(joined.file_name().unwrap_or_default()))
                .unwrap_or_else(|| joined.clone())
        });
        if !contained.starts_with(&self.root) {
            return Err(RuntimeError::PathEscape {
                path: path.display().to_string(),
            });
        }
        Ok(contained)
    }
}

#[async_trait]
impl Runtime for LocalRuntime {
    async fn read_file(&self, path: &Path) -> Result<String> {
        let requested = path.to_path_buf();
        let path = self.resolve(path)?;
        let text = tokio::fs::read_to_string(&path).await.map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                RuntimeError::NotFound {
                    path: path.display().to_string(),
                }
            } else {
                RuntimeError::from(err)
            }
        })?;
        self.reads
            .lock()
            .expect("local runtime mutex")
            .push(requested);
        Ok(text)
    }

    async fn write_file(&self, path: &Path, contents: &str) -> Result<()> {
        let requested = path.to_path_buf();
        let path = self.resolve(path)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, contents).await?;
        self.writes
            .lock()
            .expect("local runtime mutex")
            .push(requested);
        Ok(())
    }

    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchResult>> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || search_blocking(&root, request))
            .await
            .map_err(|err| RuntimeError::search(err.to_string()))?
    }

    async fn execute(&self, command: Command) -> Result<CommandResult> {
        if command.executable().is_empty() {
            return Err(RuntimeError::command("executable must not be empty"));
        }
        if command.executable().contains([' ', '\n', '\t'])
            || command.executable().contains(['\'', '"', ';', '|', '&'])
        {
            return Err(RuntimeError::command(
                "executable must be a program path, not a shell string",
            ));
        }
        let cwd = match command.working_directory() {
            Some(dir) => self.resolve(dir)?,
            None => self.root.clone(),
        };
        let started = Instant::now();
        let mut child = tokio::process::Command::new(command.executable());
        child
            .args(command.arguments())
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, value) in command.environment() {
            child.env(key, value);
        }
        let output = child
            .output()
            .await
            .map_err(|err| RuntimeError::command(format!("{}: {err}", command.executable())))?;
        tracing::info!(
            executable = command.executable(),
            exit_code = output.status.code().unwrap_or(-1),
            duration_ms = started.elapsed().as_millis() as u64,
            "runtime.command"
        );
        Ok(CommandResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration: started.elapsed(),
        })
    }

    async fn git_status(&self) -> Result<GitStatus> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || git_status_blocking(&root))
            .await
            .map_err(|err| RuntimeError::git(err.to_string()))?
    }

    async fn git_diff(&self) -> Result<GitDiff> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || git_diff_blocking(&root))
            .await
            .map_err(|err| RuntimeError::git(err.to_string()))?
    }

    fn recorded_reads(&self) -> Vec<PathBuf> {
        self.reads.lock().expect("local runtime mutex").clone()
    }

    fn recorded_writes(&self) -> Vec<PathBuf> {
        self.writes.lock().expect("local runtime mutex").clone()
    }
}

fn search_blocking(root: &Path, request: SearchRequest) -> Result<Vec<SearchResult>> {
    let mut results = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .build();
    for entry in walker {
        let entry = entry.map_err(|err| RuntimeError::search(err.to_string()))?;
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !request.matches_path(&relative) {
            continue;
        }
        match request.kind {
            SearchKind::Filename => {
                if relative.contains(&request.query)
                    || file_stem(&relative).contains(&request.query)
                {
                    results.push(SearchResult::file(PathBuf::from(&relative)));
                }
            }
            SearchKind::Content => {
                let Ok(text) = std::fs::read_to_string(path) else {
                    continue;
                };
                for (idx, line) in text.lines().enumerate() {
                    if line.contains(&request.query) {
                        results.push(SearchResult {
                            path: PathBuf::from(&relative),
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
    Ok(results)
}

fn file_stem(relative: &str) -> &str {
    relative.rsplit(['/', '\\']).next().unwrap_or(relative)
}

fn git_status_blocking(root: &Path) -> Result<GitStatus> {
    let repo = Repository::discover(root).map_err(|err| RuntimeError::git(err.to_string()))?;
    let branch = current_branch(&repo);
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|err| RuntimeError::git(err.to_string()))?;
    let mut entries = Vec::new();
    for entry in statuses.iter() {
        let Some(path) = entry.path() else {
            continue;
        };
        let Some(status) = map_status(entry.status()) else {
            continue;
        };
        entries.push(GitStatusEntry {
            path: PathBuf::from(path),
            status,
        });
    }
    Ok(GitStatus { branch, entries })
}

fn git_diff_blocking(root: &Path) -> Result<GitDiff> {
    let repo = Repository::discover(root).map_err(|err| RuntimeError::git(err.to_string()))?;
    let branch = current_branch(&repo);
    let mut patch = String::new();
    let mut opts = DiffOptions::new();
    opts.include_untracked(true);
    let diff = match repo.head() {
        Ok(head) => {
            let tree = head
                .peel_to_tree()
                .map_err(|err| RuntimeError::git(err.to_string()))?;
            repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))
                .map_err(|err| RuntimeError::git(err.to_string()))?
        }
        Err(_) => repo
            .diff_tree_to_workdir_with_index(None, Some(&mut opts))
            .map_err(|err| RuntimeError::git(err.to_string()))?,
    };
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if matches!(origin, '+' | '-' | ' ' | '\\') {
            patch.push(origin);
        }
        patch.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        true
    })
    .map_err(|err| RuntimeError::git(err.to_string()))?;
    Ok(GitDiff { branch, patch })
}

fn current_branch(repo: &Repository) -> String {
    repo.head()
        .ok()
        .and_then(|head| head.shorthand().map(str::to_string))
        .unwrap_or_else(|| "HEAD".into())
}

fn map_status(status: Status) -> Option<GitFileStatus> {
    if status.is_wt_deleted() || status.is_index_deleted() {
        Some(GitFileStatus::Deleted)
    } else if status.is_wt_renamed() || status.is_index_renamed() {
        Some(GitFileStatus::Renamed)
    } else if status.is_index_new() {
        Some(GitFileStatus::Added)
    } else if status.is_wt_new() {
        Some(GitFileStatus::Untracked)
    } else if status.is_wt_modified() || status.is_index_modified() || status.is_conflicted() {
        Some(GitFileStatus::Modified)
    } else {
        None
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_write_and_reject_escape() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = LocalRuntime::new(dir.path()).unwrap();
        runtime
            .write_file(Path::new("src/lib.rs"), "fn ok() {}\n")
            .await
            .unwrap();
        let text = runtime.read_file(Path::new("src/lib.rs")).await.unwrap();
        assert_eq!(text, "fn ok() {}\n");
        assert_eq!(runtime.recorded_reads(), vec![PathBuf::from("src/lib.rs")]);
        let err = runtime.read_file(Path::new("../secret")).await.unwrap_err();
        assert_eq!(err.kind(), "path_escape");
    }

    #[tokio::test]
    async fn search_content_and_filename() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = LocalRuntime::new(dir.path()).unwrap();
        runtime
            .write_file(Path::new("src/main.rs"), "fn hello() {}\n")
            .await
            .unwrap();
        runtime
            .write_file(Path::new("README.md"), "no match\n")
            .await
            .unwrap();
        let hits = runtime
            .search(
                SearchRequest::content("fn hello")
                    .extension("rs")
                    .exclude("target"),
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, PathBuf::from("src/main.rs"));
        let names = runtime
            .search(SearchRequest::filename("main.rs"))
            .await
            .unwrap();
        assert_eq!(names.len(), 1);
    }

    #[tokio::test]
    async fn execute_echo_without_a_shell() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = LocalRuntime::new(dir.path()).unwrap();
        let result = runtime
            .execute(Command::new("/bin/echo").arg("hello-runtime"))
            .await
            .unwrap();
        assert!(result.is_success());
        assert!(result.stdout.contains("hello-runtime"));
        let err = runtime
            .execute(Command::new("echo hello"))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), "command");
    }

    #[tokio::test]
    async fn execute_accepts_absolute_cwd_that_is_the_repo() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = LocalRuntime::new(dir.path()).unwrap();
        let result = runtime
            .execute(Command::new("/bin/echo").cwd(dir.path()).arg("cwd-ok"))
            .await
            .unwrap();
        assert!(result.is_success());
        assert!(result.stdout.contains("cwd-ok"));
    }

    #[tokio::test]
    async fn git_status_lists_untracked() {
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        let runtime = LocalRuntime::new(dir.path()).unwrap();
        runtime
            .write_file(Path::new("a.txt"), "hello\n")
            .await
            .unwrap();
        let status = runtime.git_status().await.unwrap();
        assert!(status
            .entries
            .iter()
            .any(|entry| entry.path.ends_with("a.txt")));
        let diff = runtime.git_diff().await.unwrap();
        assert_eq!(diff.branch, status.branch);
    }
}
