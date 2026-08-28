//! Real checkout + codeintel CLI. Not MemoryRuntime.
//!
//! Enable with `HYGIENE_E2E=1` and a `codeintel` binary (`CODEINTEL_BIN` or
//! `../codeintel/target/debug/codeintel`).

use std::path::{Path, PathBuf};
use std::process::Command;

use binder_ai::{
    CodeintelKnowledge, Finding, FindingKind, HygieneConfig, HygieneWorkflow,
    HygieneWorkflowRequest, LocalRuntime, MockAiClient, OperationalContext, ProcessCodeintelCli,
};

fn codeintel_bin() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("CODEINTEL_BIN") {
        let path = PathBuf::from(explicit);
        return path.is_file().then_some(path);
    }
    let sibling = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("codeintel")
        .join("target")
        .join("debug")
        .join("codeintel");
    sibling.is_file().then_some(sibling)
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", "hygiene-e2e")
        .env("GIT_AUTHOR_EMAIL", "hygiene-e2e@example.com")
        .env("GIT_COMMITTER_NAME", "hygiene-e2e")
        .env("GIT_COMMITTER_EMAIL", "hygiene-e2e@example.com")
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write_controlled_repo(root: &Path) {
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"hygiene-e2e\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn add(a:i32,b:i32)->i32{a+b}\n",
    )
    .unwrap();
    std::fs::write(root.join("README.md"), "# hygiene-e2e\n\nhello\n").unwrap();
    git(root, &["init", "-b", "master"]);
    git(root, &["add", "Cargo.toml", "src/lib.rs", "README.md"]);
    git(root, &["commit", "-m", "initial"]);
    git(root, &["branch", "feature/old"]);
}

fn finding(root: &Path, kind: FindingKind, description: &str) -> Finding {
    let mut finding = Finding::new("hygiene-e2e", kind, description);
    finding.repository_root = root.to_path_buf();
    finding
}

#[tokio::test]
async fn hygiene_workflow_on_real_git_repo_via_codeintel() {
    if std::env::var("HYGIENE_E2E").ok().as_deref() != Some("1") {
        eprintln!("skip: set HYGIENE_E2E=1 to run the live CodeintelKnowledge proof");
        return;
    }
    let bin =
        codeintel_bin().expect("codeintel binary missing; build ../codeintel or set CODEINTEL_BIN");
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let store = tmp.path().join("bindb");
    std::fs::create_dir_all(&repo).unwrap();
    write_controlled_repo(&repo);

    let indexed = Command::new(&bin)
        .args([
            "index",
            repo.to_str().unwrap(),
            "--store",
            store.to_str().unwrap(),
            "--name",
            "hygiene-e2e",
        ])
        .output()
        .expect("codeintel index");
    assert!(
        indexed.status.success(),
        "codeintel index failed: {}",
        String::from_utf8_lossy(&indexed.stderr)
    );

    let runtime = LocalRuntime::new(&repo).unwrap();
    let knowledge = CodeintelKnowledge::new(
        &store,
        vec![("hygiene-e2e".into(), repo.clone())],
        ProcessCodeintelCli { bin: bin.clone() },
    );
    let mock = MockAiClient::json_turns_local(["{}", "{}"]);
    let workflow = HygieneWorkflow::new(
        runtime,
        knowledge,
        HygieneConfig::default(),
        mock.client.clone(),
    );

    let report = workflow
        .execute(HygieneWorkflowRequest {
            request: "hygiene hygiene-e2e".into(),
            findings: vec![
                finding(&repo, FindingKind::FeatureBranchMerged, "leftover")
                    .target_ref("feature/old"),
                finding(&repo, FindingKind::CargoFmtRequired, "fmt"),
                finding(&repo, FindingKind::MissingBadge, "badge")
                    .github_slug("dpkimball/hygiene-e2e"),
            ],
            operational: Some(OperationalContext {
                default_branch: "master".into(),
                current_branch: "master".into(),
                dirty: false,
                open_pr_heads: Vec::new(),
            }),
        })
        .await
        .expect("hygiene workflow");

    assert_eq!(mock.chat_calls(), 0, "deterministic findings must skip LLM");
    assert!(!report.search_used);
    assert_eq!(report.primary_source, "discovery");
    assert!(report.success, "outcomes: {:?}", report.hygiene.outcomes);
    assert!(report.validation_passed);
    assert!(report.history_recorded);
    assert!(report.reindex.is_some());
    assert_eq!(
        report.reindex.as_ref().unwrap().repositories,
        ["hygiene-e2e"]
    );

    let lib = std::fs::read_to_string(repo.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("pub fn add(a: i32, b: i32)"),
        "cargo fmt should have rewritten lib.rs: {lib:?}"
    );
    let readme = std::fs::read_to_string(repo.join("README.md")).unwrap();
    assert!(
        readme.contains("badge.svg"),
        "badge should be inserted: {readme:?}"
    );
    let branches = git(&repo, &["branch", "--list", "feature/old"]);
    assert!(
        branches.is_empty(),
        "feature/old should be deleted: {branches:?}"
    );

    let history = store.with_extension("hygiene-history.jsonl");
    let hist = std::fs::read_to_string(&history).unwrap_or_default();
    assert!(
        hist.contains("\"workflow\":\"hygiene\""),
        "execution history jsonl missing: {} {hist:?}",
        history.display()
    );
}
