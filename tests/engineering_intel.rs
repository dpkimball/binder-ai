use std::path::PathBuf;

use binder_ai::{plan, MemoryKnowledge, PlanRequest};

#[test]
fn plan_returns_bindb_context_without_opening_files() {
    let knowledge = MemoryKnowledge::for_files(
        "Where is OLLAMA_HOST configured?",
        [
            PathBuf::from("repo-dashboard:charts/dashboard/values-dev.yaml"),
            PathBuf::from("dpk-infra:charts/dev-proxies/values.yaml"),
        ],
    );
    let response = plan(
        &knowledge,
        &PlanRequest {
            workspace: None,
            request: "Where is OLLAMA_HOST configured?".into(),
        },
    )
    .unwrap();
    assert_eq!(response.primary_source, "bindb");
    assert_eq!(
        response.inspection_set,
        vec![
            "repo-dashboard:charts/dashboard/values-dev.yaml",
            "dpk-infra:charts/dev-proxies/values.yaml",
        ]
    );
    assert!(response.resolved_execution_paths.is_empty());
    assert_eq!(response.unresolved.len(), 2);
    assert_eq!(response.reasoning, "memory knowledge");
    assert_eq!(response.doctrine, vec!["workspace.acyclic-crates"]);
    assert_eq!(response.impact, vec!["fixture"]);
    assert_eq!(response.planning_source, "bindb");
    assert_eq!(
        response.planning_result,
        binder_ai::PlanningResult::Resolved
    );
    assert!(!response.fallback_required);
    assert!(!response.repository_discovery_recommended);
    let agent_json = serde_json::to_string(&response.for_agent()).unwrap();
    assert!(
        !agent_json.contains("doctrine"),
        "agent payload must omit store-wide doctrine: {agent_json}"
    );
    assert!(!agent_json.contains("workspace.acyclic-crates"));
    assert!(agent_json.contains("\"planning_result\":\"resolved\""));
    assert!(agent_json.contains("\"fallback_required\":false"));
}

#[test]
fn plan_resolves_into_the_execution_workspace() {
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
        [PathBuf::from(
            "repo-dashboard:charts/dashboard/values-dev.yaml",
        )],
    );
    let response = plan(
        &knowledge,
        &PlanRequest {
            workspace: Some(root.path().to_path_buf()),
            request: "Where is OLLAMA_HOST configured?".into(),
        },
    )
    .unwrap();
    assert_eq!(
        response.resolved_execution_paths,
        vec!["repo-dashboard/charts/dashboard/values-dev.yaml"]
    );
    assert!(response.unresolved.is_empty());
    assert_eq!(
        response.planning_result,
        binder_ai::PlanningResult::Resolved
    );
    assert!(!response.fallback_required);
}

#[test]
fn empty_request_is_rejected() {
    let knowledge = MemoryKnowledge::for_files("x", Vec::<PathBuf>::new());
    let err = plan(
        &knowledge,
        &PlanRequest {
            workspace: None,
            request: "  ".into(),
        },
    )
    .unwrap_err();
    assert_eq!(err.kind(), "empty_request");
}

#[test]
fn intel_module_does_not_search_or_name_providers() {
    let src = include_str!("../src/intel/mod.rs");
    for needle in [
        "openai",
        "anthropic",
        "binder-llm",
        "SearchRequest",
        ".search(",
    ] {
        assert!(
            !src.contains(needle),
            "intel plan must not contain {needle}"
        );
    }
}

#[test]
fn empty_inspection_set_is_unresolved_fallback() {
    let knowledge = MemoryKnowledge::for_files("unknown symbol", Vec::<PathBuf>::new());
    let response = plan(
        &knowledge,
        &PlanRequest {
            workspace: None,
            request: "unknown symbol".into(),
        },
    )
    .unwrap();
    assert_eq!(
        response.planning_result,
        binder_ai::PlanningResult::Unresolved
    );
    assert!(response.fallback_required);
    assert_eq!(
        response.fallback_reason,
        Some(binder_ai::FallbackReason::NotIndexed)
    );
    assert!(response.repository_discovery_recommended);
    assert_eq!(response.metrics.inspection_set_size, 0);
    assert!(!response.metrics.repository_search_performed);
}

#[test]
fn partial_workspace_resolution_recommends_discovery_without_requiring_it() {
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
    let response = plan(
        &knowledge,
        &PlanRequest {
            workspace: Some(root.path().to_path_buf()),
            request: "Where is OLLAMA_HOST configured?".into(),
        },
    )
    .unwrap();
    assert_eq!(response.planning_result, binder_ai::PlanningResult::Partial);
    assert!(!response.fallback_required);
    assert_eq!(
        response.fallback_reason,
        Some(binder_ai::FallbackReason::WorkspaceResolutionFailed)
    );
    assert!(response.repository_discovery_recommended);
    assert_eq!(response.resolved_execution_paths.len(), 1);
    assert_eq!(response.unresolved.len(), 1);
}

#[test]
fn classify_unresolved_workspace_targets_requires_fallback() {
    let decision = binder_ai::classify_planning(2, 0, 2, true);
    assert_eq!(decision.planning_result, binder_ai::PlanningResult::Partial);
    assert!(decision.fallback_required);
    assert_eq!(
        decision.fallback_reason,
        Some(binder_ai::FallbackReason::WorkspaceResolutionFailed)
    );
}
