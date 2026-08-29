//! Map bindb semantic inspect targets onto an execution workspace.
//!
//! bindb returns location-independent `repo:path` strings. This module does
//! not plan. It only resolves those strings into files under a checkout map.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Checkout map for one execution. Indexing paths are irrelevant here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceLayout {
    root: Option<PathBuf>,
    binds: BTreeMap<String, PathBuf>,
}

impl WorkspaceLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
            binds: BTreeMap::new(),
        }
    }

    /// Unqualified paths pass through. Qualified `repo:path` needs a bind or root child.
    pub fn passthrough() -> Self {
        Self::default()
    }

    pub fn bind(mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.binds.insert(name.into(), path.into());
        self
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn checkout(&self, repo: &str) -> Option<PathBuf> {
        if let Some(path) = self.binds.get(repo) {
            return Some(path.clone());
        }
        let root = self.root.as_ref()?;
        let candidate = root.join(repo);
        candidate.is_dir().then_some(candidate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub semantic: String,
    pub repository: String,
    pub execution_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectionResolution {
    pub semantic: Vec<PathBuf>,
    pub resolved: Vec<ResolvedTarget>,
    pub binds: Vec<(String, PathBuf)>,
}

impl InspectionResolution {
    pub fn execution_paths(&self) -> Vec<PathBuf> {
        self.resolved
            .iter()
            .map(|row| row.execution_path.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("workspace resolution failed: {unresolved} (expected repository {expected_repository}): {message}")]
pub struct WorkspaceResolveError {
    pub unresolved: String,
    pub expected_repository: String,
    pub message: String,
}

pub fn resolve_inspection_set(
    layout: &WorkspaceLayout,
    semantic: &[PathBuf],
) -> Result<InspectionResolution, WorkspaceResolveError> {
    let attempt = resolve_inspection_set_best_effort(layout, semantic);
    if let Some(err) = attempt.unresolved.into_iter().next() {
        return Err(err);
    }
    Ok(attempt.resolution)
}

/// Resolve every target. Missing checkouts stay in `unresolved` (planning / IDE).
pub fn resolve_inspection_set_best_effort(
    layout: &WorkspaceLayout,
    semantic: &[PathBuf],
) -> InspectionAttempt {
    let mut resolved = Vec::new();
    let mut binds: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut unresolved = Vec::new();
    for path in semantic {
        match resolve_one(layout, path) {
            Ok(row) => {
                if !row.repository.is_empty() {
                    if let Some(checkout) = layout.checkout(&row.repository) {
                        binds.insert(row.repository.clone(), checkout);
                    }
                }
                resolved.push(row);
            }
            Err(err) => unresolved.push(err),
        }
    }
    InspectionAttempt {
        resolution: InspectionResolution {
            semantic: semantic.to_vec(),
            resolved,
            binds: binds.into_iter().collect(),
        },
        unresolved,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectionAttempt {
    pub resolution: InspectionResolution,
    pub unresolved: Vec<WorkspaceResolveError>,
}

fn resolve_one(
    layout: &WorkspaceLayout,
    path: &Path,
) -> Result<ResolvedTarget, WorkspaceResolveError> {
    let target = parse_semantic(path)?;
    match target.repository {
        None => Ok(ResolvedTarget {
            semantic: semantic_string(path),
            repository: String::new(),
            execution_path: target.relative,
        }),
        Some(repo) => {
            let checkout = layout
                .checkout(&repo)
                .ok_or_else(|| WorkspaceResolveError {
                    unresolved: semantic_string(path),
                    expected_repository: repo.clone(),
                    message: "no checkout for repository in the execution workspace".into(),
                })?;
            if let Some(root) = layout.root() {
                if !checkout_inside_root(&checkout, root) {
                    return Err(WorkspaceResolveError {
                        unresolved: semantic_string(path),
                        expected_repository: repo,
                        message: "checkout is not inside the execution workspace".into(),
                    });
                }
            }
            let file = checkout.join(&target.relative);
            if !file.is_file() {
                return Err(WorkspaceResolveError {
                    unresolved: semantic_string(path),
                    expected_repository: repo,
                    message: format!("file not found at {}", file.display()),
                });
            }
            Ok(ResolvedTarget {
                semantic: semantic_string(path),
                repository: repo.clone(),
                execution_path: PathBuf::from(&repo).join(&target.relative),
            })
        }
    }
}

struct ParsedTarget {
    repository: Option<String>,
    relative: PathBuf,
}

fn parse_semantic(path: &Path) -> Result<ParsedTarget, WorkspaceResolveError> {
    let raw = semantic_string(path);
    if raw.is_empty() {
        return Err(WorkspaceResolveError {
            unresolved: raw,
            expected_repository: String::new(),
            message: "empty inspect target".into(),
        });
    }
    match raw.split_once(':') {
        None => {
            let relative = safe_relative(&raw, &raw, "")?;
            Ok(ParsedTarget {
                repository: None,
                relative,
            })
        }
        Some((repo, rest)) => {
            if repo.is_empty() || rest.is_empty() {
                return Err(WorkspaceResolveError {
                    unresolved: raw.clone(),
                    expected_repository: repo.to_string(),
                    message: "semantic target must be repo:relative-path".into(),
                });
            }
            if repo.contains('/') || repo.contains('\\') {
                return Err(WorkspaceResolveError {
                    unresolved: raw.clone(),
                    expected_repository: repo.to_string(),
                    message: "repository name must be a single path segment".into(),
                });
            }
            let relative = safe_relative(&raw, rest, repo)?;
            Ok(ParsedTarget {
                repository: Some(repo.to_string()),
                relative,
            })
        }
    }
}

fn safe_relative(
    unresolved: &str,
    relative: &str,
    expected_repository: &str,
) -> Result<PathBuf, WorkspaceResolveError> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err(WorkspaceResolveError {
            unresolved: unresolved.into(),
            expected_repository: expected_repository.into(),
            message: "relative path must not be absolute".into(),
        });
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(WorkspaceResolveError {
                    unresolved: unresolved.into(),
                    expected_repository: expected_repository.into(),
                    message: "relative path must stay inside the checkout".into(),
                });
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(WorkspaceResolveError {
            unresolved: unresolved.into(),
            expected_repository: expected_repository.into(),
            message: "relative path is empty".into(),
        });
    }
    Ok(out)
}

fn semantic_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn checkout_inside_root(checkout: &Path, root: &Path) -> bool {
    let checkout = std::fs::canonicalize(checkout).unwrap_or_else(|_| checkout.to_path_buf());
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    checkout.starts_with(&root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn unqualified_paths_pass_through() {
        let layout = WorkspaceLayout::passthrough();
        let resolution = resolve_inspection_set(&layout, &[PathBuf::from("src/lib.rs")]).unwrap();
        assert_eq!(
            resolution.execution_paths(),
            vec![PathBuf::from("src/lib.rs")]
        );
        assert!(resolution.binds.is_empty());
    }

    #[test]
    fn qualified_path_resolves_under_workspace_root() {
        let root = tempfile::tempdir().unwrap();
        let server = root.path().join("binder-server");
        fs::create_dir_all(&server).unwrap();
        fs::write(server.join("Dockerfile"), "FROM rust\n").unwrap();
        let layout = WorkspaceLayout::new(root.path());
        let resolution =
            resolve_inspection_set(&layout, &[PathBuf::from("binder-server:Dockerfile")]).unwrap();
        assert_eq!(
            resolution.execution_paths(),
            vec![PathBuf::from("binder-server/Dockerfile")]
        );
        assert_eq!(resolution.binds[0].0, "binder-server");
    }

    #[test]
    fn bind_overrides_index_time_location() {
        let indexed = tempfile::tempdir().unwrap();
        let execution = tempfile::tempdir().unwrap();
        fs::write(indexed.path().join("Dockerfile"), "INDEXED\n").unwrap();
        fs::write(execution.path().join("Dockerfile"), "EXEC\n").unwrap();
        let layout =
            WorkspaceLayout::passthrough().bind("binder-server", execution.path().to_path_buf());
        let resolution =
            resolve_inspection_set(&layout, &[PathBuf::from("binder-server:Dockerfile")]).unwrap();
        assert_eq!(
            resolution.execution_paths(),
            vec![PathBuf::from("binder-server/Dockerfile")]
        );
        let text = fs::read_to_string(layout.checkout("binder-server").unwrap().join("Dockerfile"))
            .unwrap();
        assert_eq!(text, "EXEC\n");
        assert_ne!(execution.path(), indexed.path());
    }

    #[test]
    fn missing_checkout_is_structured() {
        let layout = WorkspaceLayout::new(tempfile::tempdir().unwrap().path());
        let err = resolve_inspection_set(&layout, &[PathBuf::from("binder-server:Dockerfile")])
            .unwrap_err();
        assert_eq!(err.unresolved, "binder-server:Dockerfile");
        assert_eq!(err.expected_repository, "binder-server");
        assert!(err.message.contains("no checkout"));
    }

    #[test]
    fn parent_dir_is_rejected() {
        let layout = WorkspaceLayout::passthrough();
        let err = resolve_inspection_set(&layout, &[PathBuf::from("binder-server:../secret")])
            .unwrap_err();
        assert!(err.message.contains("inside the checkout"));
    }

    #[test]
    fn bind_outside_workspace_root_is_rejected() {
        let workspace = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        fs::write(other.path().join("Dockerfile"), "FROM rust\n").unwrap();
        let layout = WorkspaceLayout::new(workspace.path())
            .bind("binder-server", other.path().to_path_buf());
        let err = resolve_inspection_set(&layout, &[PathBuf::from("binder-server:Dockerfile")])
            .unwrap_err();
        assert_eq!(err.expected_repository, "binder-server");
        assert!(err.message.contains("inside the execution workspace"));
    }

    #[test]
    fn best_effort_keeps_resolved_when_one_target_is_missing() {
        let root = tempfile::tempdir().unwrap();
        let server = root.path().join("binder-server");
        fs::create_dir_all(&server).unwrap();
        fs::write(server.join("Dockerfile"), "FROM rust\n").unwrap();
        let layout = WorkspaceLayout::new(root.path());
        let attempt = resolve_inspection_set_best_effort(
            &layout,
            &[
                PathBuf::from("binder-server:Dockerfile"),
                PathBuf::from("missing-repo:Cargo.toml"),
            ],
        );
        assert_eq!(
            attempt.resolution.execution_paths(),
            vec![PathBuf::from("binder-server/Dockerfile")]
        );
        assert_eq!(attempt.unresolved.len(), 1);
        assert_eq!(attempt.unresolved[0].expected_repository, "missing-repo");
    }
}
