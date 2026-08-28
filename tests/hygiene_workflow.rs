use std::path::{Path, PathBuf};
use std::sync::Arc;

use binder_ai::{
    Command, CommandResult, Finding, FindingKind, GitDiff, HygieneConfig, HygieneWorkflow,
    HygieneWorkflowRequest, MemoryKnowledge, MemoryRuntime, MockAiClient,
};

fn cargo_fmt_runtime() -> MemoryRuntime {
    MemoryRuntime::new()
        .with_file(
            "src/lib.rs",
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .with_file("README.md", "# Demo\n\nHello\n")
        .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
        .script(
            Command::new("cargo").arg("test"),
            CommandResult::success("ok"),
        )
        .script(
            Command::new("cargo").args(["clippy", "--fix", "--allow-dirty", "--allow-staged"]),
            CommandResult::success(""),
        )
        .with_git_diff(GitDiff {
            branch: "master".into(),
            patch: "diff --git a/src/lib.rs b/src/lib.rs\n+saturating_add\n".into(),
        })
}

fn rust_bug() -> Finding {
    Finding::new("demo", FindingKind::RustBug, "add overflows in src/lib.rs").rust_cargo()
}

fn plan_and_edits() -> (String, String) {
    let plan = serde_json::json!({
        "files": ["src/lib.rs"],
        "steps": ["use saturating_add"],
        "summary": "Prevent overflow"
    });
    let edits = serde_json::json!({
        "edits": [{
            "path": "src/lib.rs",
            "contents": "pub fn add(a: i32, b: i32) -> i32 { a.saturating_add(b) }\n"
        }],
        "summary": "Updated add"
    });
    (plan.to_string(), edits.to_string())
}

#[tokio::test]
async fn deterministic_fmt_queries_bindb_and_skips_llm() {
    let runtime = Arc::new(cargo_fmt_runtime());
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "hygiene fmt",
        [PathBuf::from("src/lib.rs")],
    ));
    let mock = MockAiClient::json_turns_local(["{}", "{}"]);
    let workflow = HygieneWorkflow::new(
        runtime.clone(),
        knowledge.clone(),
        HygieneConfig::default(),
        mock.client.clone(),
    );
    let report = workflow
        .execute(HygieneWorkflowRequest {
            request: "hygiene fmt".into(),
            findings: vec![Finding::new("demo", FindingKind::CargoFmtRequired, "fmt")],
        })
        .await
        .unwrap();

    assert_eq!(knowledge.plan_calls(), 1);
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(mock.chat_calls(), 0);
    assert!(!report.llm_invoked);
    assert!(!report.search_used);
    assert_eq!(report.primary_source, "bindb");
    assert_eq!(report.inspection_set, vec![PathBuf::from("src/lib.rs")]);
    assert!(report.files_opened.is_empty(), "fmt does not inspect files");
    assert!(report.files_modified.is_empty());
    assert!(report.validation_passed);
    assert!(report.success);
    assert!(report.reindex.is_some());
    assert_eq!(knowledge.reindex_calls(), 1);
    assert!(report.history_recorded);
    assert_eq!(report.doctrine_before.findings, 0);
    assert_eq!(report.doctrine_after.findings, 0);
    let history = knowledge.execution_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].workflow, "hygiene");
    assert!(!history[0].llm_invoked);
    assert!(runtime
        .executed_commands()
        .iter()
        .any(|(exe, args)| exe == "cargo" && args == &["fmt".to_string()]));
}

#[tokio::test]
async fn empty_inspection_set_is_ok_when_hygiene_is_deterministic() {
    let runtime = Arc::new(cargo_fmt_runtime());
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "hygiene fmt",
        Vec::<PathBuf>::new(),
    ));
    let mock = MockAiClient::json_turns_local(["{}", "{}"]);
    let workflow = HygieneWorkflow::new(
        runtime.clone(),
        knowledge.clone(),
        HygieneConfig::default(),
        mock.client.clone(),
    );
    let report = workflow
        .execute(HygieneWorkflowRequest {
            request: "hygiene fmt".into(),
            findings: vec![Finding::new("demo", FindingKind::CargoFmtRequired, "fmt")],
        })
        .await
        .unwrap();

    assert!(report.success);
    assert!(report.inspection_set.is_empty());
    assert_eq!(mock.chat_calls(), 0);
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(knowledge.reindex_calls(), 1);
}

#[tokio::test]
async fn ai_hygiene_reads_only_the_inspection_set() {
    let runtime = Arc::new(cargo_fmt_runtime());
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "hygiene rust bug",
        [PathBuf::from("src/lib.rs")],
    ));
    let (plan, edits) = plan_and_edits();
    let mock = MockAiClient::json_turns_local([plan, edits]);
    let workflow = HygieneWorkflow::new(
        runtime.clone(),
        knowledge.clone(),
        HygieneConfig::default(),
        mock.client.clone(),
    );
    let report = workflow
        .execute(HygieneWorkflowRequest {
            request: "hygiene rust bug".into(),
            findings: vec![rust_bug()],
        })
        .await
        .unwrap();

    assert_eq!(knowledge.plan_calls(), 1);
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(runtime.read_paths(), vec![PathBuf::from("src/lib.rs")]);
    assert!(!runtime
        .read_paths()
        .iter()
        .any(|p| p == Path::new("README.md")));
    assert_eq!(mock.chat_calls(), 2);
    assert!(report.llm_invoked);
    assert!(report.success);
    assert_eq!(
        runtime.files()[Path::new("src/lib.rs")],
        "pub fn add(a: i32, b: i32) -> i32 { a.saturating_add(b) }\n"
    );
    assert_eq!(runtime.files()[Path::new("README.md")], "# Demo\n\nHello\n");
    assert_eq!(knowledge.reindex_calls(), 1);
    assert!(report.history_recorded);
}

#[tokio::test]
async fn empty_inspection_set_refuses_ai_without_llm() {
    let runtime = Arc::new(cargo_fmt_runtime());
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "hygiene rust bug",
        Vec::<PathBuf>::new(),
    ));
    let mock = MockAiClient::json_turns_local(["{}", "{}"]);
    let workflow = HygieneWorkflow::new(
        runtime.clone(),
        knowledge.clone(),
        HygieneConfig::default(),
        mock.client.clone(),
    );
    let err = workflow
        .execute(HygieneWorkflowRequest {
            request: "hygiene rust bug".into(),
            findings: vec![rust_bug()],
        })
        .await
        .unwrap_err();

    assert_eq!(err.kind(), "knowledge_failed");
    assert_eq!(mock.chat_calls(), 0);
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(knowledge.reindex_calls(), 0);
    assert!(knowledge.execution_history().is_empty());
}

#[test]
fn hygiene_workflow_does_not_search_or_name_providers() {
    let src = include_str!("../src/workflows/hygiene/workflow.rs");
    for needle in [
        "openai",
        "anthropic",
        "ollama",
        "std::fs",
        "tokio::fs",
        "git2",
        "SearchRequest",
        ".search(",
    ] {
        assert!(
            !src.contains(needle),
            "hygiene workflow must not contain {needle}"
        );
    }
}
