use std::path::Path;
use std::sync::Arc;

use binder_ai::{
    Command, CommandResult, Finding, FindingKind, GitDiff, HygieneConfig, HygieneEngine,
    MemoryRuntime, MockAiClient, ProjectKind, RemediationType, RoutingPolicy,
};

fn cargo_memory() -> MemoryRuntime {
    MemoryRuntime::new()
        .with_file(
            "src/lib.rs",
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
        .script(
            Command::new("cargo").arg("test"),
            CommandResult::success("ok"),
        )
        .script(
            Command::new("git").args(["branch", "-d", "feature/old"]),
            CommandResult::success("Deleted branch feature/old"),
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

fn cargo_runtime() -> Arc<MemoryRuntime> {
    Arc::new(cargo_memory())
}

fn rust_bug(repo: &str) -> Finding {
    Finding::new(repo, FindingKind::RustBug, "add overflows in src/lib.rs").rust_cargo()
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
async fn deterministic_findings_never_invoke_ai() {
    let runtime = Arc::new(cargo_memory().with_file("README.md", "# Demo\n\nHello\n"));
    let (plan, edits) = plan_and_edits();
    let mock = MockAiClient::json_turns_local([plan, edits]);
    let engine = HygieneEngine::new(HygieneConfig::default(), mock.client.clone());

    let findings = vec![
        Finding::new("demo", FindingKind::FeatureBranchMerged, "merged").target_ref("feature/old"),
        Finding::new("demo", FindingKind::CargoFmtRequired, "fmt"),
        Finding::new("demo", FindingKind::ClippyWarnings, "clippy"),
        Finding::new("demo", FindingKind::MissingBadge, "badge").github_slug("dpkimball/demo"),
        Finding::new("demo", FindingKind::DirtyRepository, "dirty"),
        Finding::new("demo", FindingKind::OpenPullRequest, "pr"),
        Finding::new("demo", FindingKind::BrokenPipeline, "ci"),
        Finding::new("demo", FindingKind::FeatureBranchStale, "stale"),
        Finding::new("demo", FindingKind::Unknown, "unknown"),
    ];
    let report = engine.process_findings(runtime.clone(), &findings).await;
    assert_eq!(mock.chat_calls(), 0);
    assert_eq!(report.outcomes.len(), 9);
    assert!(report
        .outcomes
        .iter()
        .all(|row| row.remediation != RemediationType::Ai));
    assert!(runtime
        .executed_commands()
        .iter()
        .any(|(exe, args)| exe == "cargo" && args == &["fmt".to_string()]));
}

#[tokio::test]
async fn rust_findings_invoke_issue_workflow() {
    let runtime = cargo_runtime();
    let (plan, edits) = plan_and_edits();
    let mock = MockAiClient::json_turns_local([plan, edits]);
    let engine = HygieneEngine::new(HygieneConfig::default(), mock.client.clone());
    let report = engine
        .process_findings(runtime.clone(), &[rust_bug("demo")])
        .await;

    assert_eq!(mock.chat_calls(), 2);
    assert!(runtime.search_calls() >= 1);
    assert!(runtime.write_calls() >= 1);
    assert_eq!(report.outcomes.len(), 1);
    let row = &report.outcomes[0];
    assert_eq!(row.remediation, RemediationType::Ai);
    assert!(row.success);
    assert!(row.diff_generated);
    assert_eq!(row.tests_passed, Some(true));
    assert_eq!(
        runtime.files()[Path::new("src/lib.rs")],
        "pub fn add(a: i32, b: i32) -> i32 { a.saturating_add(b) }\n"
    );
}

#[tokio::test]
async fn local_only_routing_is_required() {
    let runtime = cargo_runtime();
    let (plan, edits) = plan_and_edits();
    let mock = MockAiClient::json_turns([plan, edits]);
    assert_eq!(
        mock.client.config().routing_policy(),
        RoutingPolicy::Balanced
    );
    let engine = HygieneEngine::new(HygieneConfig::default(), mock.client.clone());
    let report = engine.process_findings(runtime, &[rust_bug("demo")]).await;
    assert_eq!(mock.chat_calls(), 0);
    assert!(!report.outcomes[0].success);
    assert!(report.outcomes[0].summary.contains("LocalOnly"));
}

#[tokio::test]
async fn workflow_failure_is_reported_and_hygiene_continues() {
    let runtime = cargo_runtime();
    let mock = MockAiClient::json_turns_local(["not-json"]);
    let engine = HygieneEngine::new(HygieneConfig::default(), mock.client.clone());
    let findings = vec![
        rust_bug("demo"),
        Finding::new("demo", FindingKind::CargoFmtRequired, "fmt"),
    ];
    let report = engine.process_findings(runtime.clone(), &findings).await;
    assert_eq!(report.outcomes.len(), 2);
    assert_eq!(report.outcomes[0].remediation, RemediationType::Ai);
    assert!(!report.outcomes[0].success);
    assert!(report.outcomes[0].summary.contains("workflow failed"));
    assert_eq!(
        report.outcomes[1].remediation,
        RemediationType::Deterministic
    );
    assert!(report.outcomes[1].success);
    assert!(mock.chat_calls() >= 1);
}

#[tokio::test]
async fn max_ai_repositories_skips_later_repos() {
    let (plan, edits) = plan_and_edits();
    let mock = MockAiClient::json_turns_local([plan, edits]);
    let config = HygieneConfig {
        max_ai_repositories: 1,
        ..HygieneConfig::default()
    };
    let engine = HygieneEngine::new(config, mock.client.clone());
    let first = engine
        .process_findings(cargo_runtime(), &[rust_bug("one")])
        .await;
    let second = engine
        .process_findings(cargo_runtime(), &[rust_bug("two")])
        .await;
    assert!(first.outcomes[0].success);
    assert!(!second.outcomes[0].success);
    assert!(second.outcomes[0].summary.contains("max_ai_repositories"));
    assert_eq!(mock.chat_calls(), 2);
}

#[tokio::test]
async fn empty_diff_fails_validation_without_retry() {
    let runtime = Arc::new(
        MemoryRuntime::new()
            .with_file(
                "src/lib.rs",
                "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
            )
            .script(Command::new("cargo").arg("fmt"), CommandResult::success(""))
            .script(
                Command::new("cargo").arg("test"),
                CommandResult::success("ok"),
            ),
    );
    let (plan, edits) = plan_and_edits();
    let mock = MockAiClient::json_turns_local([plan, edits]);
    let engine = HygieneEngine::new(HygieneConfig::default(), mock.client.clone());
    let report = engine.process_findings(runtime, &[rust_bug("demo")]).await;
    assert!(!report.outcomes[0].success);
    assert!(!report.outcomes[0].diff_generated);
    assert!(report.outcomes[0].summary.contains("empty git diff"));
    assert_eq!(mock.chat_calls(), 2);
}

#[tokio::test]
async fn non_rust_projects_are_report_only() {
    let runtime = cargo_runtime();
    let (plan, edits) = plan_and_edits();
    let mock = MockAiClient::json_turns_local([plan, edits]);
    let engine = HygieneEngine::new(HygieneConfig::default(), mock.client.clone());
    let finding = Finding::new("web", FindingKind::MissingTests, "add tests");
    assert_eq!(finding.project, ProjectKind::Other);
    let report = engine.process_findings(runtime, &[finding]).await;
    assert_eq!(mock.chat_calls(), 0);
    assert_eq!(report.outcomes[0].remediation, RemediationType::ReportOnly);
}

#[tokio::test]
async fn dry_run_does_not_call_ai_or_execute() {
    let runtime = cargo_runtime();
    let (plan, edits) = plan_and_edits();
    let mock = MockAiClient::json_turns_local([plan, edits]);
    let config = HygieneConfig {
        dry_run: true,
        ..HygieneConfig::default()
    };
    let engine = HygieneEngine::new(config, mock.client.clone());
    let findings = vec![
        rust_bug("demo"),
        Finding::new("demo", FindingKind::CargoFmtRequired, "fmt"),
    ];
    let report = engine.process_findings(runtime.clone(), &findings).await;
    assert_eq!(mock.chat_calls(), 0);
    assert!(runtime.executed_commands().is_empty());
    assert!(report.outcomes.iter().all(|row| row.success));
}

#[test]
fn default_config_is_local_only() {
    let config = HygieneConfig::default();
    assert!(config.ai_remediation);
    assert_eq!(config.routing_policy, RoutingPolicy::LocalOnly);
    assert!(!config.dry_run);
}

#[test]
fn hygiene_modules_use_runtime_exclusively() {
    let sources = [
        include_str!("../src/workflows/hygiene/engine.rs"),
        include_str!("../src/workflows/hygiene/deterministic.rs"),
        include_str!("../src/workflows/hygiene/dispatcher.rs"),
        include_str!("../src/workflows/hygiene/finding.rs"),
        include_str!("../src/workflows/hygiene/config.rs"),
        include_str!("../src/workflows/hygiene/remediation.rs"),
        include_str!("../src/workflows/hygiene/result.rs"),
        include_str!("../src/workflows/hygiene/mod.rs"),
        include_str!("../src/workflows/hygiene/workflow.rs"),
    ];
    let joined = sources.join("\n").to_ascii_lowercase();
    for needle in [
        "std::fs",
        "tokio::fs",
        "git2",
        "process::command",
        "openai",
        "anthropic",
        "ollama",
        "git commit",
        "git push",
        "git merge",
        "gh pr",
    ] {
        assert!(
            !joined.contains(needle),
            "hygiene must not contain {needle}"
        );
    }
}
