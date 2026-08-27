use std::path::Path;

use crate::runtime::{Command, CommandResult, Runtime};
use crate::workflows::hygiene::remediation::DeterministicAction;

/// Apply a deterministic repair through [`Runtime`]. Never calls [`crate::AiClient`].
pub async fn apply<R: Runtime>(
    runtime: &R,
    action: &DeterministicAction,
    cwd: &Path,
    dry_run: bool,
) -> Result<ApplyOutcome, ApplyError> {
    if dry_run {
        return Ok(ApplyOutcome {
            summary: format!("dry_run: {}", action_name(action)),
            git_diff: None,
        });
    }
    match action {
        DeterministicAction::DeleteMergedBranch { name } => {
            let result = execute(
                runtime,
                Command::new("git").args(["branch", "-d", name]).cwd(cwd),
            )
            .await?;
            if !result.is_success() {
                return Err(ApplyError {
                    summary: format!("git branch -d failed: exit {}", result.exit_code),
                });
            }
            Ok(ApplyOutcome {
                summary: format!("deleted local branch {name}"),
                git_diff: None,
            })
        }
        DeterministicAction::CargoFmt => {
            let result = execute(runtime, Command::new("cargo").arg("fmt").cwd(cwd)).await?;
            if !result.is_success() {
                return Err(ApplyError {
                    summary: format!("cargo fmt failed: exit {}", result.exit_code),
                });
            }
            Ok(ApplyOutcome {
                summary: "cargo fmt".into(),
                git_diff: None,
            })
        }
        DeterministicAction::ClippyFix => {
            let result = execute(
                runtime,
                Command::new("cargo")
                    .args(["clippy", "--fix", "--allow-dirty", "--allow-staged"])
                    .cwd(cwd),
            )
            .await?;
            if !result.is_success() {
                return Err(ApplyError {
                    summary: format!("cargo clippy --fix failed: exit {}", result.exit_code),
                });
            }
            Ok(ApplyOutcome {
                summary: "cargo clippy --fix".into(),
                git_diff: None,
            })
        }
        DeterministicAction::InsertBadge { github_slug } => {
            insert_badge(runtime, cwd, github_slug).await
        }
    }
}

fn action_name(action: &DeterministicAction) -> &'static str {
    match action {
        DeterministicAction::DeleteMergedBranch { .. } => "delete_merged_branch",
        DeterministicAction::CargoFmt => "cargo_fmt",
        DeterministicAction::ClippyFix => "clippy_fix",
        DeterministicAction::InsertBadge { .. } => "insert_badge",
    }
}

async fn execute<R: Runtime>(runtime: &R, command: Command) -> Result<CommandResult, ApplyError> {
    runtime.execute(command).await.map_err(|err| ApplyError {
        summary: format!("runtime execute failed: {}", err.kind()),
    })
}

async fn insert_badge<R: Runtime>(
    runtime: &R,
    _cwd: &Path,
    slug: &str,
) -> Result<ApplyOutcome, ApplyError> {
    let path = Path::new("README.md");
    let existing = runtime.read_file(path).await.map_err(|err| ApplyError {
        summary: format!("readme read failed: {}", err.kind()),
    })?;
    if existing.contains("badge.svg") {
        return Ok(ApplyOutcome {
            summary: "ci badge already present".into(),
            git_diff: None,
        });
    }
    let badge = format!(
        "[![CI](https://github.com/{slug}/actions/workflows/ci.yml/badge.svg)](https://github.com/{slug}/actions/workflows/ci.yml)"
    );
    let updated = insert_badge_line(&existing, &badge);
    runtime
        .write_file(path, &updated)
        .await
        .map_err(|err| ApplyError {
            summary: format!("readme write failed: {}", err.kind()),
        })?;
    Ok(ApplyOutcome {
        summary: "inserted ci badge".into(),
        git_diff: None,
    })
}

fn insert_badge_line(readme: &str, badge: &str) -> String {
    let mut lines: Vec<&str> = readme.lines().collect();
    if lines.first().is_some_and(|line| line.starts_with('#')) {
        lines.insert(1, "");
        lines.insert(2, badge);
        if lines.get(3).is_some_and(|line| !line.is_empty()) {
            lines.insert(3, "");
        }
        let mut out = lines.join("\n");
        if readme.ends_with('\n') {
            out.push('\n');
        }
        out
    } else {
        format!("{badge}\n\n{readme}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyOutcome {
    pub summary: String,
    pub git_diff: Option<crate::runtime::GitDiff>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyError {
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::insert_badge_line;

    #[test]
    fn badge_follows_title() {
        let out = insert_badge_line("# Demo\n\nHello\n", "[![CI](https://example/badge.svg)](x)");
        assert!(out.starts_with("# Demo\n\n[![CI]"));
        assert!(out.contains("Hello"));
    }
}
