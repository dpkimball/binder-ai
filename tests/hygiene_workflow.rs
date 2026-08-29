use std::path::{Path, PathBuf};
use std::sync::Arc;

use binder_ai::{
    Command, CommandResult, Finding, FindingKind, GitDiff, HygieneConfig, HygieneWorkflow,
    HygieneWorkflowRequest, MemoryKnowledge, MemoryRuntime, MockAiClient, OperationalContext,
    WorkspaceLayout,
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
        .script(
            Command::new("git").args(["branch", "-d", "feature/old"]),
            CommandResult::success("Deleted branch feature/old"),
        )
        .script(
            Command::new("git").args(["checkout", "master"]),
            CommandResult::success("Switched to branch master"),
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
async fn deterministic_fmt_skips_bindb_plan_and_llm() {
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
            operational: None,
        })
        .await
        .unwrap();

    assert_eq!(knowledge.plan_calls(), 0);
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(mock.chat_calls(), 0);
    assert!(!report.llm_invoked);
    assert!(!report.search_used);
    assert_eq!(report.primary_source, "discovery");
    assert!(report.inspection_set.is_empty());
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
            operational: None,
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
async fn badge_reads_readme_without_bindb_inspection_set() {
    let runtime = Arc::new(cargo_fmt_runtime());
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "hygiene badge",
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
            request: "hygiene badge".into(),
            findings: vec![Finding::new("demo", FindingKind::MissingBadge, "badge")
                .github_slug("dpkimball/demo")],
            operational: None,
        })
        .await
        .unwrap();

    assert_eq!(knowledge.plan_calls(), 0);
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(mock.chat_calls(), 0);
    assert!(report.success);
    assert_eq!(runtime.read_paths(), vec![PathBuf::from("README.md")]);
    assert!(runtime.files()[Path::new("README.md")].contains("badge.svg"));
    assert_eq!(knowledge.reindex_calls(), 1);
}

#[tokio::test]
async fn branch_delete_refuses_open_pr_head() {
    let runtime = Arc::new(cargo_fmt_runtime());
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "hygiene branch",
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
            request: "hygiene branch".into(),
            findings: vec![
                Finding::new("demo", FindingKind::FeatureBranchMerged, "merged")
                    .target_ref("feature/old"),
            ],
            operational: Some(OperationalContext {
                default_branch: "master".into(),
                current_branch: "master".into(),
                dirty: false,
                open_pr_heads: vec!["feature/old".into()],
            }),
        })
        .await
        .unwrap();

    assert!(!report.success);
    assert!(report.hygiene.outcomes[0].summary.contains("open PR head"));
    assert_eq!(knowledge.reindex_calls(), 0);
    assert_eq!(mock.chat_calls(), 0);
}

#[tokio::test]
async fn branch_delete_checks_out_default_when_current_and_clean() {
    let runtime = Arc::new(cargo_fmt_runtime());
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "hygiene branch",
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
            request: "hygiene branch".into(),
            findings: vec![
                Finding::new("demo", FindingKind::FeatureBranchMerged, "merged")
                    .target_ref("feature/old"),
            ],
            operational: Some(OperationalContext {
                default_branch: "master".into(),
                current_branch: "feature/old".into(),
                dirty: false,
                open_pr_heads: Vec::new(),
            }),
        })
        .await
        .unwrap();

    assert!(report.success);
    let executed = runtime.executed_commands();
    assert!(executed
        .iter()
        .any(|(exe, args)| exe == "git" && args == &["checkout".to_string(), "master".into()]));
    assert!(executed.iter().any(|(exe, args)| {
        exe == "git" && args == &["branch".to_string(), "-d".into(), "feature/old".into()]
    }));
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
            operational: None,
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
    assert_eq!(report.primary_source, "bindb");
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
            operational: None,
        })
        .await
        .unwrap_err();

    assert_eq!(err.kind(), "knowledge_failed");
    assert_eq!(mock.chat_calls(), 0);
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(knowledge.reindex_calls(), 0);
    assert!(knowledge.execution_history().is_empty());
}

#[tokio::test]
async fn qualified_inspect_set_without_workspace_fails_before_llm() {
    let runtime = Arc::new(cargo_fmt_runtime());
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "hygiene rust bug",
        [PathBuf::from("binder-server:Dockerfile")],
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
            operational: None,
        })
        .await
        .unwrap_err();

    assert_eq!(err.kind(), "workspace_resolution");
    assert!(err.to_string().contains("binder-server:Dockerfile"));
    assert!(err.to_string().contains("binder-server"));
    assert_eq!(mock.chat_calls(), 0);
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(runtime.read_calls(), 0);
    assert_eq!(knowledge.plan_calls(), 1);
    assert_eq!(knowledge.reindex_calls(), 0);
    assert!(knowledge.execution_history().is_empty());
    assert!(runtime.write_paths().is_empty());
}

#[tokio::test]
async fn semantic_targets_resolve_into_temporary_workspace() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("binder-server")).unwrap();
    std::fs::write(
        root.path().join("binder-server").join("Dockerfile"),
        "FROM rust\n",
    )
    .unwrap();

    let runtime = Arc::new(
        MemoryRuntime::new()
            .with_file("binder-server/Dockerfile", "FROM rust\n")
            .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
            .script(
                Command::new("cargo").arg("test"),
                CommandResult::success("ok"),
            )
            .with_git_diff(GitDiff {
                branch: "master".into(),
                patch: "diff --git a/binder-server/Dockerfile b/binder-server/Dockerfile\n+WORKDIR /app\n".into(),
            }),
    );
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "how should a Python service ship a Rust binary",
        [PathBuf::from("binder-server:Dockerfile")],
    ));
    let plan = serde_json::json!({
        "files": ["binder-server/Dockerfile"],
        "steps": ["use extra_args build context"],
        "summary": "Ship via Docker extra_args"
    })
    .to_string();
    let edits = serde_json::json!({
        "edits": [{
            "path": "binder-server/Dockerfile",
            "contents": "FROM rust\nWORKDIR /app\n"
        }],
        "summary": "Updated Dockerfile"
    })
    .to_string();
    let mock = MockAiClient::json_turns_local([plan, edits]);
    let workflow = HygieneWorkflow::new(
        runtime.clone(),
        knowledge.clone(),
        HygieneConfig::default(),
        mock.client.clone(),
    )
    .with_workspace(WorkspaceLayout::new(root.path()));
    let report = workflow
        .execute(HygieneWorkflowRequest {
            request: "how should a Python service ship a Rust binary".into(),
            findings: vec![rust_bug()],
            operational: None,
        })
        .await
        .unwrap();

    assert_eq!(knowledge.plan_calls(), 1);
    assert_eq!(report.primary_source, "bindb");
    assert!(!report.search_used);
    assert_eq!(
        report.inspection_set,
        vec![PathBuf::from("binder-server:Dockerfile")]
    );
    assert_eq!(
        report.resolved_execution_paths,
        vec![PathBuf::from("binder-server/Dockerfile")]
    );
    assert_eq!(
        runtime.read_paths(),
        vec![PathBuf::from("binder-server/Dockerfile")]
    );
    assert_eq!(runtime.search_calls(), 0);
    assert_eq!(mock.chat_calls(), 2);
    assert!(report.llm_invoked);
    assert!(report.success);
    assert!(report.validation_passed);
    assert!(report.history_recorded);
    assert_eq!(knowledge.reindex_calls(), 1);
    assert_eq!(knowledge.execution_binds()[0].0, "binder-server");
    assert_eq!(
        knowledge.execution_binds()[0].1,
        root.path().join("binder-server")
    );
    assert_eq!(
        runtime.files()[Path::new("binder-server/Dockerfile")],
        "FROM rust\nWORKDIR /app\n"
    );
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
