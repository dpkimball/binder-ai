use std::path::PathBuf;
use std::sync::Arc;

use binder_ai::{
    DocumentationReviewRequest, DocumentationReviewWorkflow, MemoryKnowledge, MemoryRuntime,
};

#[tokio::test]
async fn documentation_review_reads_bindb_readme_without_llm_or_search() {
    let runtime = Arc::new(
        MemoryRuntime::new()
            .with_file("README.md", "# Demo\n\nThis repository has a real overview.\n\nMore than eighty characters of documentation live here.\n")
            .with_file("src/lib.rs", "pub fn add() {}\n"),
    );
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "documentation review",
        [PathBuf::from("README.md")],
    ));
    let report = DocumentationReviewWorkflow::new(runtime.clone(), knowledge.clone())
        .execute(DocumentationReviewRequest {
            request: "documentation review".into(),
            repository: "demo".into(),
            repository_root: PathBuf::from("."),
            finding_kind: Some("feature_branch_stale".into()),
        })
        .await
        .unwrap();

    assert_eq!(knowledge.plan_calls(), 1);
    assert_eq!(runtime.search_calls(), 0);
    assert!(!report.llm_invoked);
    assert!(!report.search_used);
    assert_eq!(report.primary_source, "bindb");
    assert!(report.success);
    assert!(report.validation_passed);
    assert!(report.history_recorded);
    assert!(report.gaps.is_empty());
    assert!(report
        .files_inspected
        .iter()
        .any(|p| p.ends_with("README.md")));
    assert!(report.summary.contains("no gaps"));
    let history = knowledge.execution_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].workflow, "documentation_review");
    assert!(!history[0].llm_invoked);
    assert!(runtime.write_calls() == 0);
}

#[tokio::test]
async fn documentation_review_reports_missing_readme_after_search() {
    let runtime = Arc::new(MemoryRuntime::new().with_file("src/lib.rs", "pub fn add() {}\n"));
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "documentation review",
        Vec::<PathBuf>::new(),
    ));
    let report = DocumentationReviewWorkflow::new(runtime.clone(), knowledge.clone())
        .execute(DocumentationReviewRequest {
            request: "documentation review".into(),
            repository: "empty".into(),
            repository_root: PathBuf::from("."),
            finding_kind: Some("documentation_issue".into()),
        })
        .await
        .unwrap();

    assert_eq!(runtime.search_calls(), 1);
    assert!(report.search_used);
    assert_eq!(report.primary_source, "discovery");
    assert!(report.success, "gaps are results, not workflow failure");
    assert_eq!(report.gaps.len(), 1);
    assert_eq!(report.gaps[0].kind, "missing_readme");
    assert!(report.summary.contains("no README"));
}

#[tokio::test]
async fn documentation_review_flags_thin_readme() {
    let runtime = Arc::new(MemoryRuntime::new().with_file("README.md", "todo\n"));
    let knowledge = Arc::new(MemoryKnowledge::for_files(
        "documentation review",
        [PathBuf::from("README.md")],
    ));
    let report = DocumentationReviewWorkflow::new(runtime, knowledge)
        .execute(DocumentationReviewRequest {
            request: "documentation review".into(),
            repository: "thin".into(),
            repository_root: PathBuf::from("."),
            finding_kind: None,
        })
        .await
        .unwrap();

    assert_eq!(report.gaps.len(), 1);
    assert_eq!(report.gaps[0].kind, "thin_readme");
    assert!(report.summary.contains("characters"));
}
