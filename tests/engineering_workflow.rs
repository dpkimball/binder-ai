use std::path::{Path, PathBuf};
use std::sync::Arc;

use binder_ai::{
    Command, CommandResult, EngineeringRequest, EngineeringWorkflow, GitDiff, GitFileStatus,
    GitStatus, GitStatusEntry, MemoryKnowledge, MemoryRuntime, MockAiClient,
};

#[tokio::test]
async fn engineering_workflow_uses_bindb_then_curated_llm_context() {
    let original = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
    let updated = "pub fn add(a: i32, b: i32) -> i32 { a.saturating_add(b) }\n";
    let runtime = Arc::new(
        MemoryRuntime::new()
            .with_file("src/lib.rs", original)
            .with_file("README.md", "do not send this to the llm\n")
            .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
            .script(
                Command::new("cargo").arg("test"),
                CommandResult::success("test result: ok. 1 passed"),
            )
            .with_git_status(GitStatus {
                branch: "feat/engineering".into(),
                entries: vec![GitStatusEntry {
                    path: PathBuf::from("src/lib.rs"),
                    status: GitFileStatus::Modified,
                }],
            })
            .with_git_diff(GitDiff {
                branch: "feat/engineering".into(),
                patch: "diff --git a/src/lib.rs b/src/lib.rs\n+saturating_add\n".into(),
            }),
    );

    let knowledge = std::sync::Arc::new(MemoryKnowledge::for_files(
        "prevent overflow in add",
        [PathBuf::from("src/lib.rs")],
    ));
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

    let workflow =
        EngineeringWorkflow::new(runtime.clone(), mock.client.clone(), knowledge.clone());
    let report = workflow
        .execute(EngineeringRequest {
            request: "prevent overflow in add".into(),
            repository_root: PathBuf::from("."),
        })
        .await
        .unwrap();

    assert_eq!(runtime.search_calls(), 0, "must not walk the repository");
    assert_eq!(runtime.read_paths(), vec![PathBuf::from("src/lib.rs")]);
    assert!(!runtime
        .read_paths()
        .iter()
        .any(|p| p == Path::new("README.md")));
    assert_eq!(mock.chat_calls(), 2, "plan + implement via binder-llm");
    assert_eq!(runtime.files()[Path::new("src/lib.rs")], updated);
    assert_eq!(
        runtime.files()[Path::new("README.md")],
        "do not send this to the llm\n"
    );
    assert!(report.success);
    assert_eq!(report.primary_source, "bindb");
    assert_eq!(report.inspection_set, vec![PathBuf::from("src/lib.rs")]);
    assert!(report.llm_invoked);
    assert!(report.reindex.is_some());
    assert_eq!(report.doctrine_before.findings, 0);
    assert_eq!(report.doctrine_after.findings, 0);
    assert!(report.opened_outside_inspection_set.is_empty());
    assert_eq!(knowledge.plan_calls(), 1);
    assert_eq!(knowledge.reindex_calls(), 1);
    assert_eq!(
        report.planning.planning_result,
        binder_ai::PlanningResult::Resolved
    );
    assert!(!report.planning.fallback_required);
    assert!(!report.planning.repository_search_performed);
    assert_eq!(report.planning.files_opened_from_inspection_set, 1);
}

#[tokio::test]
async fn engineering_workflow_does_not_apply_edits_outside_inspection_set() {
    let runtime = Arc::new(
        MemoryRuntime::new()
            .with_file("src/lib.rs", "pub fn add() {}\n")
            .with_file("README.md", "untouched\n")
            .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
            .script(
                Command::new("cargo").arg("test"),
                CommandResult::success("ok"),
            )
            .with_git_diff(GitDiff {
                branch: "feat/engineering".into(),
                patch: String::new(),
            }),
    );
    let knowledge = MemoryKnowledge::for_files("change add", [PathBuf::from("src/lib.rs")]);
    let plan = serde_json::json!({
        "files": ["src/lib.rs", "README.md"],
        "steps": ["also rewrite readme"],
        "summary": "too broad"
    });
    let edits = serde_json::json!({
        "edits": [
            {"path": "src/lib.rs", "contents": "pub fn add() {}\n"},
            {"path": "README.md", "contents": "leaked context rewrite\n"}
        ],
        "summary": "touched readme"
    });
    let mock = MockAiClient::json_turns([plan.to_string(), edits.to_string()]);
    let workflow = EngineeringWorkflow::new(runtime.clone(), mock.client.clone(), knowledge);
    let report = workflow
        .execute(EngineeringRequest {
            request: "change add".into(),
            repository_root: PathBuf::from("."),
        })
        .await
        .unwrap();

    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(runtime.files()[Path::new("README.md")], "untouched\n");
    assert!(report
        .opened_outside_inspection_set
        .iter()
        .any(|p| p == Path::new("README.md")));
    assert!(!report.success);
    assert!(
        report.reindex.is_none(),
        "do not update bindb after a failed run"
    );
}

#[tokio::test]
async fn engineering_workflow_searches_only_after_planner_fallback() {
    let original = "pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
    let updated = "pub fn add(a: i32, b: i32) -> i32 { a.saturating_add(b) }\n";
    let runtime = Arc::new(
        MemoryRuntime::new()
            .with_file("src/lib.rs", original)
            .with_file("README.md", "do not send this to the llm\n")
            .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
            .script(
                Command::new("cargo").arg("test"),
                CommandResult::success("test result: ok. 1 passed"),
            )
            .with_git_diff(GitDiff {
                branch: "feat/engineering".into(),
                patch: "diff --git a/src/lib.rs b/src/lib.rs\n+saturating_add\n".into(),
            }),
    );
    let knowledge = MemoryKnowledge::for_files("prevent overflow in add", Vec::<PathBuf>::new());
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
    let workflow = EngineeringWorkflow::new(runtime.clone(), mock.client.clone(), knowledge);
    let report = workflow
        .execute(EngineeringRequest {
            request: "prevent overflow in add".into(),
            repository_root: PathBuf::from("."),
        })
        .await
        .unwrap();

    assert!(
        runtime.search_calls() >= 1,
        "fallback must be explicit repository discovery"
    );
    assert!(report.planning.fallback_required);
    assert_eq!(
        report.planning.fallback_reason,
        Some(binder_ai::FallbackReason::NotIndexed)
    );
    assert!(report.planning.repository_search_performed);
    assert_eq!(report.planning.files_opened_from_inspection_set, 0);
    assert!(report.success);
}

#[tokio::test]
async fn engineering_workflow_fallback_search_fails_observably_when_nothing_matches() {
    let runtime = Arc::new(MemoryRuntime::new().with_file("src/lib.rs", "fn x() {}\n"));
    let knowledge = MemoryKnowledge::for_files("no files", Vec::<PathBuf>::new());
    let mock = MockAiClient::json_turns(["{}", "{}"]);
    let workflow = EngineeringWorkflow::new(runtime.clone(), mock.client.clone(), knowledge);
    let err = workflow
        .execute(EngineeringRequest {
            request: "xyzzy-not-in-tree".into(),
            repository_root: PathBuf::from("."),
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind(), "search_failed");
    assert!(runtime.search_calls() >= 1);
    assert_eq!(
        mock.chat_calls(),
        0,
        "must not invoke reasoning when discovery also finds nothing"
    );
}

#[test]
fn engineering_workflow_does_not_name_providers() {
    let src = include_str!("../src/workflows/engineering/workflow.rs");
    for needle in [
        "openai",
        "anthropic",
        "ollama",
        "std::fs",
        "tokio::fs",
        "git2",
    ] {
        assert!(
            !src.contains(needle),
            "engineering workflow must not contain {needle}"
        );
    }
    assert!(
        src.contains("fallback_required"),
        "search must be gated on planner fallback"
    );
    assert!(src.contains("SearchRequest"));
}
