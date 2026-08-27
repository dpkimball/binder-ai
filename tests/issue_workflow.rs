use std::path::{Path, PathBuf};
use std::sync::Arc;

use binder_ai::{
    Command, CommandResult, GitDiff, GitFileStatus, GitStatus, GitStatusEntry, Issue,
    IssueImplementationRequest, IssueImplementationWorkflow, MemoryRuntime, MockAiClient,
};

#[tokio::test]
async fn issue_workflow_runs_end_to_end_in_memory() {
    let original = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
    let updated = "pub fn add(a: i32, b: i32) -> i32 { a.saturating_add(b) }\n";
    let runtime = Arc::new(
        MemoryRuntime::new()
            .with_file("src/lib.rs", original)
            .with_file("README.md", "unrelated\n")
            .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
            .script(
                Command::new("cargo").arg("test"),
                CommandResult::success("test result: ok. 1 passed"),
            )
            .with_git_status(GitStatus {
                branch: "feature/runtime-abstraction".into(),
                entries: vec![GitStatusEntry {
                    path: PathBuf::from("src/lib.rs"),
                    status: GitFileStatus::Modified,
                }],
            })
            .with_git_diff(GitDiff {
                branch: "feature/runtime-abstraction".into(),
                patch: "diff --git a/src/lib.rs b/src/lib.rs\n+saturating_add\n".into(),
            }),
    );

    let plan = serde_json::json!({
        "files": ["src/lib.rs"],
        "steps": ["use saturating_add"],
        "summary": "Prevent overflow in add"
    });
    let edits = serde_json::json!({
        "edits": [{
            "path": "src/lib.rs",
            "contents": updated
        }],
        "summary": "Updated add to saturating_add"
    });
    let mock = MockAiClient::json_turns([plan.to_string(), edits.to_string()]);

    let workflow = IssueImplementationWorkflow::new(runtime.clone(), mock.client.clone());
    let result = workflow
        .execute(IssueImplementationRequest {
            issue: Issue::new(
                "add overflow in lib",
                "The add helper in src/lib.rs overflows on large values.",
            ),
            repository_root: PathBuf::from("."),
        })
        .await
        .unwrap();

    assert!(runtime.search_calls() >= 1, "search executed");
    assert!(runtime.read_calls() >= 1, "files read");
    assert_eq!(mock.chat_calls(), 2, "AI invoked twice (plan + implement)");
    assert!(runtime.write_calls() >= 1, "files modified");
    assert_eq!(runtime.files()[Path::new("src/lib.rs")], updated);
    let executed = runtime.executed_commands();
    assert!(
        executed
            .iter()
            .any(|(exe, args)| exe == "cargo" && args == &["fmt".to_string()]),
        "fmt executed: {executed:?}"
    );
    assert!(
        executed
            .iter()
            .any(|(exe, args)| exe == "cargo" && args == &["test".to_string()]),
        "test executed: {executed:?}"
    );
    assert!(result.git_diff.patch.contains("saturating_add"));
    assert!(result.success);
    assert_eq!(result.modified_files, vec![PathBuf::from("src/lib.rs")]);
    assert_eq!(result.command_results.len(), 2);
}

#[test]
fn workflow_modules_do_not_name_providers() {
    let src = include_str!("../src/workflows/issue/workflow.rs");
    for needle in [
        "openai",
        "anthropic",
        "ollama",
        "std::fs",
        "tokio::fs",
        "git2",
        "process::Command",
    ] {
        assert!(
            !src.to_ascii_lowercase().contains(needle),
            "workflow.rs must not contain {needle}"
        );
    }
}
