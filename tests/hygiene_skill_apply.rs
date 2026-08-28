//! Skill-shaped discovery JSON → HygieneWorkflow on a real checkout.
//!
//! Enable with `HYGIENE_SKILL=1` and a `codeintel` binary (`CODEINTEL_BIN` or
//! `../codeintel/target/debug/codeintel`).

use std::path::{Path, PathBuf};
use std::process::Command;

use binder_ai::{
    CodeintelKnowledge, HygieneConfig, HygieneWorkflow, LocalRuntime, MockAiClient,
    ProcessCodeintelCli, SkillApplyDocument,
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

fn git(repo: &Path, args: &[String]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", "hygiene-skill")
        .env("GIT_AUTHOR_EMAIL", "hygiene-skill@example.com")
        .env("GIT_COMMITTER_NAME", "hygiene-skill")
        .env("GIT_COMMITTER_EMAIL", "hygiene-skill@example.com")
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
        "[package]\nname = \"hygiene-skill\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn add(a:i32,b:i32)->i32{a+b}\n",
    )
    .unwrap();
    std::fs::write(root.join("README.md"), "# hygiene-skill\n\nhello\n").unwrap();
    git(root, &["init".into(), "-b".into(), "master".into()]);
    git(
        root,
        &[
            "add".into(),
            "Cargo.toml".into(),
            "src/lib.rs".into(),
            "README.md".into(),
        ],
    );
    git(root, &["commit".into(), "-m".into(), "initial".into()]);
    git(root, &["branch".into(), "feature/old".into()]);
}

/// Same checks the Cursor skill runs: leftover local branch, fmt, README badge.
fn discover_skill_json(repo: &Path) -> String {
    let leftover = git(
        repo,
        &["branch".into(), "--list".into(), "feature/old".into()],
    );
    assert!(
        leftover.contains("feature/old"),
        "skill discovery expected leftover branch, got {leftover:?}"
    );
    let fmt = Command::new("cargo")
        .args(["fmt", "--check"])
        .current_dir(repo)
        .status()
        .expect("cargo fmt --check");
    assert!(!fmt.success(), "skill discovery expected fmt drift");
    let readme = std::fs::read_to_string(repo.join("README.md")).unwrap();
    assert!(
        !readme.contains("badge.svg"),
        "skill discovery expected missing badge"
    );
    let root = repo.to_str().expect("utf8 repo path");
    serde_json::to_string(&serde_json::json!({
        "request": "hygiene hygiene-skill",
        "findings": [
            {
                "repository": "hygiene-skill",
                "repository_root": root,
                "kind": "feature_branch_merged",
                "description": "leftover local branch",
                "target_ref": "feature/old"
            },
            {
                "repository": "hygiene-skill",
                "repository_root": root,
                "kind": "cargo_fmt_required",
                "description": "cargo fmt --check failed",
                "project": "rust_cargo"
            },
            {
                "repository": "hygiene-skill",
                "repository_root": root,
                "kind": "missing_badge",
                "description": "README has no CICD badge",
                "github_slug": "dpkimball/hygiene-skill"
            }
        ],
        "operational": {
            "default_branch": "master",
            "current_branch": "master",
            "dirty": false,
            "open_pr_heads": []
        }
    }))
    .expect("skill json")
}

#[tokio::test]
async fn skill_discovery_json_applies_via_hygiene_workflow() {
    if std::env::var("HYGIENE_SKILL").ok().as_deref() != Some("1") {
        eprintln!("skip: set HYGIENE_SKILL=1 to run the Cursor skill apply proof");
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
            "hygiene-skill",
        ])
        .output()
        .expect("codeintel index");
    assert!(
        indexed.status.success(),
        "codeintel index failed: {}",
        String::from_utf8_lossy(&indexed.stderr)
    );

    let json = discover_skill_json(&repo);
    let request = SkillApplyDocument::parse(&json)
        .expect("skill json")
        .into_request()
        .expect("skill request");

    let runtime = LocalRuntime::new(&repo).unwrap();
    let knowledge = CodeintelKnowledge::new(
        &store,
        vec![("hygiene-skill".into(), repo.clone())],
        ProcessCodeintelCli { bin: bin.clone() },
    );
    let mock = MockAiClient::json_turns_local(["{}", "{}"]);
    let workflow = HygieneWorkflow::new(
        runtime,
        knowledge,
        HygieneConfig::default(),
        mock.client.clone(),
    );
    let report = workflow.execute(request).await.expect("hygiene workflow");

    assert_eq!(mock.chat_calls(), 0);
    assert!(!report.search_used);
    assert_eq!(report.primary_source, "discovery");
    assert!(report.success, "outcomes: {:?}", report.hygiene.outcomes);
    assert!(report.history_recorded);

    let lib = std::fs::read_to_string(repo.join("src/lib.rs")).unwrap();
    assert!(lib.contains("pub fn add(a: i32, b: i32)"), "{lib:?}");
    let readme = std::fs::read_to_string(repo.join("README.md")).unwrap();
    assert!(readme.contains("badge.svg"), "{readme:?}");
    let branches = git(
        &repo,
        &["branch".into(), "--list".into(), "feature/old".into()],
    );
    assert!(branches.is_empty(), "{branches:?}");
}
