use std::path::{Path, PathBuf};
use std::sync::Arc;

use binder_ai::{
    autonomous_decline_reason, execute_implement, plan_for_implement, report_from_plan, Command,
    CommandResult, GitDiff, Issue, MemoryKnowledge, MemoryRuntime, MockAiClient, PlanningResult,
};

#[test]
fn dry_run_does_not_invoke_llm_when_inspect_set_resolves() {
    let knowledge =
        MemoryKnowledge::for_files("prevent overflow in add", [PathBuf::from("src/lib.rs")]);
    let plan = plan_for_implement(
        &knowledge,
        "prevent overflow in add",
        PathBuf::from("."),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(plan.planning_result, PlanningResult::Resolved);
    assert!(autonomous_decline_reason(&plan).is_none());
    let report = report_from_plan(plan, "fixture", true);
    assert!(report.success);
    assert!(!report.declined);
    assert!(report.dry_run);
    assert!(!report.llm_invoked);
    assert!(report.files_modified.is_empty());
}

#[test]
fn unattended_run_declines_when_bindb_is_not_indexed() {
    let knowledge = MemoryKnowledge::for_files("mystery", Vec::<PathBuf>::new());
    let plan = plan_for_implement(&knowledge, "mystery", PathBuf::from("."), Vec::new()).unwrap();
    assert_eq!(autonomous_decline_reason(&plan), Some("fallback_required"));
    let report = report_from_plan(plan, "fixture", true);
    assert!(report.declined);
    assert!(!report.success);
    assert!(!report.llm_invoked);
    assert_eq!(report.decline_reason.as_deref(), Some("fallback_required"));
}

#[test]
fn partial_plan_is_declined_unattended() {
    let root = tempfile::tempdir().unwrap();
    let dash = root.path().join("repo-dashboard");
    std::fs::create_dir_all(dash.join("charts/dashboard")).unwrap();
    std::fs::write(
        dash.join("charts/dashboard/values-dev.yaml"),
        "ollama:\n  host: example\n",
    )
    .unwrap();
    let knowledge = MemoryKnowledge::for_files(
        "Where is OLLAMA_HOST configured?",
        [
            PathBuf::from("repo-dashboard:charts/dashboard/values-dev.yaml"),
            PathBuf::from("missing-repo:values.yaml"),
        ],
    );
    let plan = plan_for_implement(
        &knowledge,
        "Where is OLLAMA_HOST configured?",
        root.path().to_path_buf(),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(plan.planning_result, PlanningResult::Partial);
    assert_eq!(
        autonomous_decline_reason(&plan),
        Some("planning_not_resolved")
    );
}

#[tokio::test]
async fn execute_uses_inspection_set_without_search() {
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
            .with_git_diff(GitDiff {
                branch: "feat/implement".into(),
                patch: "diff --git a/src/lib.rs b/src/lib.rs\n+saturating_add\n".into(),
            }),
    );
    let knowledge =
        MemoryKnowledge::for_files("prevent overflow in add", [PathBuf::from("src/lib.rs")]);
    let plan = plan_for_implement(
        &knowledge,
        "prevent overflow in add",
        PathBuf::from("."),
        Vec::new(),
    )
    .unwrap();
    let llm_plan = serde_json::json!({
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
    let mock = MockAiClient::json_turns([llm_plan.to_string(), edits.to_string()]);
    let report = execute_implement(
        runtime.clone(),
        mock.client.clone(),
        Issue::new("add overflow", "The add helper overflows."),
        PathBuf::from("."),
        "fixture",
        &plan,
    )
    .await
    .unwrap();

    assert_eq!(runtime.search_calls(), 0);
    assert!(report.success);
    assert!(report.llm_invoked);
    assert!(!report.dry_run);
    assert_eq!(runtime.files()[Path::new("src/lib.rs")], updated);
}

#[test]
fn bind_resolves_qualified_inspect_targets_on_a_single_checkout() {
    let checkout = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(checkout.path().join("src")).unwrap();
    std::fs::write(checkout.path().join("src/lib.rs"), "pub fn add() {}\n").unwrap();
    let knowledge = MemoryKnowledge::for_files("fix add", [PathBuf::from("binder-ai:src/lib.rs")]);
    let plan = plan_for_implement(
        &knowledge,
        "fix add",
        checkout.path().to_path_buf(),
        vec![("binder-ai".into(), checkout.path().to_path_buf())],
    )
    .unwrap();
    assert_eq!(plan.planning_result, PlanningResult::Resolved);
    assert!(autonomous_decline_reason(&plan).is_none());
    assert_eq!(plan.resolved_execution_paths, vec!["binder-ai/src/lib.rs"]);
    let relative = binder_ai::checkout_relative_paths(&plan, "binder-ai");
    assert_eq!(relative, vec![PathBuf::from("src/lib.rs")]);
}
