//! Repo-scoped `/ask` prompt building and context gates (testable without HTTP/LLM).

use super::{PlanResponse, PlanningResult};
use crate::knowledge::{GuidanceSummary, LinkedDocSummary, LinkedIssueSummary};

/// Minimum token overlap for guidance-first `/ask` (repo-prefix row only).
pub const GUIDANCE_ASK_MIN_SCORE: u32 = 2;

/// Why `/ask` refused to call the LLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskRejectReason {
    NoContext,
    UnresolvedWithoutRepoContext,
    RepoNotIndexed(String),
}

impl AskRejectReason {
    pub fn message(&self) -> String {
        match self {
            Self::NoContext => {
                "no bindb context for this repo/question; index the repo or store a question first"
                    .into()
            }
            Self::UnresolvedWithoutRepoContext => {
                "plan unresolved for this repo with no repo-scoped files, docs, or guidance; \
                 index the repo or store a question first"
                    .into()
            }
            Self::RepoNotIndexed(repo) => format!("repo not indexed: {repo}"),
        }
    }
}

/// True when the repo has a bank prefix or appears in the bindb store index list.
pub fn is_repo_known(repo: &str, indexed_repos: &[String]) -> bool {
    guidance_id_prefix(repo).is_some() || indexed_repos.iter().any(|r| r == repo)
}

/// Highest-scoring repo-prefix guidance row meeting the ask threshold, if any.
pub fn select_bank_answer(
    repo: &str,
    filtered_guidance: &[GuidanceSummary],
) -> Option<(String, String, u32)> {
    let prefix = guidance_id_prefix(repo)?;
    filtered_guidance
        .iter()
        .filter(|g| g.id.starts_with(prefix))
        .max_by_key(|g| g.match_score)
        .filter(|g| g.match_score >= GUIDANCE_ASK_MIN_SCORE)
        .map(|g| (g.id.clone(), g.answer.clone(), g.match_score))
}

/// Stable id for agent corrections when no bank id was returned from `/ask`.
pub fn correction_id(question: &str) -> String {
    let mut hash: u64 = 5381;
    for b in question.trim().to_lowercase().bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(b));
    }
    format!("COR-{:08x}", hash & 0xffff_ffff)
}

/// Use explicit bank id from `/ask` guidance when present; else stable hash id.
pub fn resolve_correction_id(explicit_id: Option<&str>, question: &str) -> String {
    explicit_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| correction_id(question))
}

/// Question-bank ID prefix for a codeintel repo name (`C1` → codeintel, `A4` → binder-ai).
pub fn guidance_id_prefix(repo: &str) -> Option<&'static str> {
    match repo {
        "codeintel" => Some("C"),
        "binder-ai" => Some("A"),
        "bindb" => Some("B"),
        "foreman" => Some("F"),
        "binder-llm" => Some("L"),
        "repo-dashboard" => Some("R"),
        "dpk-build" => Some("DB"),
        "dpk-ci" => Some("DC"),
        "dpk-infra" => Some("DI"),
        _ => None,
    }
}

/// Keep paths under `{repo}/`. Never fall back to other repos (avoids wrong LLM context).
pub fn filter_paths_for_repo(paths: &[String], repo: &str) -> Vec<String> {
    let prefix = format!("{repo}/");
    paths
        .iter()
        .filter(|p| p.starts_with(&prefix))
        .cloned()
        .collect()
}

/// Prefer docs indexed for the requested repo.
pub fn filter_linked_docs_for_repo(docs: &[LinkedDocSummary], repo: &str) -> Vec<LinkedDocSummary> {
    filter_linked_docs_for_repo_impl(docs, repo, false)
}

/// Repo docs only — no fallback to other repos (used by `/ask`).
pub fn filter_linked_docs_strict_for_repo(
    docs: &[LinkedDocSummary],
    repo: &str,
) -> Vec<LinkedDocSummary> {
    filter_linked_docs_for_repo_impl(docs, repo, true)
}

fn filter_linked_docs_for_repo_impl(
    docs: &[LinkedDocSummary],
    repo: &str,
    strict: bool,
) -> Vec<LinkedDocSummary> {
    let scoped: Vec<_> = docs.iter().filter(|d| d.repo == repo).cloned().collect();
    if scoped.is_empty() && !strict {
        docs.to_vec()
    } else {
        scoped
    }
}

/// Issues for the requested repo only (`/ask` must not pull Foreman issues into repo-dashboard).
pub fn filter_linked_issues_for_repo(
    issues: &[LinkedIssueSummary],
    repo: &str,
) -> Vec<LinkedIssueSummary> {
    issues.iter().filter(|i| i.repo == repo).cloned().collect()
}

/// Repo-specific guidance first, then cross-repo (`X*`). Prefix-less repos get `X*` only.
pub fn filter_guidance_for_repo(guidance: &[GuidanceSummary], repo: &str) -> Vec<GuidanceSummary> {
    let Some(prefix) = guidance_id_prefix(repo) else {
        return guidance
            .iter()
            .filter(|g| g.id.starts_with('X'))
            .cloned()
            .collect();
    };
    let mut repo_specific: Vec<_> = guidance
        .iter()
        .filter(|g| g.id.starts_with(prefix))
        .cloned()
        .collect();
    let mut cross: Vec<_> = guidance
        .iter()
        .filter(|g| g.id.starts_with('X'))
        .cloned()
        .collect();
    if repo_specific.is_empty() {
        cross
    } else {
        repo_specific.append(&mut cross);
        repo_specific
    }
}

/// Gate before calling the LLM on `/ask`.
pub fn assess_ask_context(
    repo: &str,
    plan: &PlanResponse,
    repo_files_read: &[String],
    repo_doc_sections: usize,
    repo_guidance: &[GuidanceSummary],
) -> Result<(), AskRejectReason> {
    let has_repo_files = !repo_files_read.is_empty();
    let has_repo_docs = repo_doc_sections > 0;
    let has_repo_guidance = !repo_guidance.is_empty();
    let has_any = has_repo_files || has_repo_docs || has_repo_guidance;

    if !has_any {
        return Err(AskRejectReason::NoContext);
    }

    if plan.planning_result == PlanningResult::Unresolved {
        let has_repo_specific = repo_guidance
            .iter()
            .any(|g| guidance_id_prefix(repo).is_some_and(|prefix| g.id.starts_with(prefix)));
        if !has_repo_specific && !has_repo_files && !has_repo_docs {
            return Err(AskRejectReason::UnresolvedWithoutRepoContext);
        }
    }

    let _ = repo;
    Ok(())
}

/// Build the LLM prompt for `POST /ask`.
pub fn build_ask_prompt(
    repo: &str,
    question: &str,
    plan: &PlanResponse,
    file_sections: &[String],
    guidance: &[GuidanceSummary],
    inference_mode: bool,
) -> String {
    let mut prompt = format!(
        "You are an expert software engineer. Answer this question about the **{repo}** \
         repository using the bindb metadata and source excerpts below.\n\n\
         > {question}\n\n\
         Rules:\n"
    );
    if inference_mode {
        prompt.push_str(&format!(
            "- No authoritative question-bank row matched at confidence ≥{}. Do not state \
             constraints as facts unless supported by `{repo}/` excerpts.\n",
            GUIDANCE_ASK_MIN_SCORE
        ));
    }
    prompt.push_str(&format!(
        "         - Scope your answer to **{repo}** only; do not describe other repositories unless \
           cross-repo guidance explicitly requires it.\n\
         - Write **2–5 sentences** stating engineering **constraints** directly (question-bank \
           style). Not a summary essay.\n\
         - Do not hedge (forbidden: \"it seems\", \"based on the provided\", \"appears to\", \
           \"might be worth\").\n\
         - Use only source excerpts whose paths start with `{repo}/`. Ignore all other paths.\n\
         - Pre-authored question-bank answers marked authoritative below override inference \
           from source excerpts when they conflict.\n\
         - Do not invent type names, field names, or API shapes not shown in the excerpts or \
           authoritative answers.\n\
         - If `{repo}/` excerpts and guidance do not support a confident answer, reply exactly: \
           `Insufficient indexed context for {repo} to answer this.`\n"
    ));

    if !guidance.is_empty() {
        prompt.push_str(
            "\n## Authoritative answers (question bank)\n\n\
             These pre-authored answers are **binding** for this repository. Prefer them over \
             inferring from source excerpts.\n\n",
        );
        for g in guidance {
            prompt.push_str(&format!("### {} — {}\n{}\n\n", g.id, g.question, g.answer));
        }
    }

    if !plan.matched_summary.is_empty() {
        prompt.push_str("\n## Matched context\n\n");
        prompt.push_str(&plan.matched_summary);
        prompt.push('\n');
    }
    if !plan.reasoning.is_empty() {
        prompt.push_str("\n## Why these files matter\n\n");
        prompt.push_str(&plan.reasoning);
        prompt.push('\n');
    }

    if !file_sections.is_empty() {
        prompt.push_str("\n## Source excerpts\n\n");
        prompt.push_str(&file_sections.join("\n\n"));
    }

    let issues: Vec<String> = filter_linked_issues_for_repo(&plan.linked_issues, repo)
        .iter()
        .filter(|i| !i.body_excerpt.is_empty())
        .take(3)
        .map(|i| format!("### Issue #{}: {}\n{}", i.number, i.title, i.body_excerpt))
        .collect();
    if !issues.is_empty() {
        prompt.push_str("\n\n## Related Issues\n\n");
        prompt.push_str(&issues.join("\n\n"));
    }

    let docs = filter_linked_docs_strict_for_repo(&plan.linked_docs, repo);
    let doc_lines: Vec<String> = docs
        .iter()
        .take(5)
        .map(|d| format!("- `{}` ({}/{}) — {}", d.path, d.repo, d.kind, d.title))
        .collect();
    if !doc_lines.is_empty() {
        prompt.push_str("\n\n## Related Documentation\n\n");
        prompt.push_str(&doc_lines.join("\n"));
    }

    prompt.push_str("\n\nAnswer:");
    prompt
}

/// Build the LLM prompt for `POST /explain` (workspace-wide, not repo-scoped).
pub fn build_explain_prompt(
    question: &str,
    plan: &PlanResponse,
    file_sections: &[String],
) -> String {
    let mut prompt = format!(
        "You are an expert software engineer. Answer this question about the codebase:\n\n\
         > {question}\n\n\
         Cite specific function names and file paths. Be concise.\n"
    );

    if !file_sections.is_empty() {
        prompt.push_str("\n\n## Source Files\n\n");
        prompt.push_str(&file_sections.join("\n\n"));
    }

    let issues: Vec<String> = plan
        .linked_issues
        .iter()
        .filter(|i| !i.body_excerpt.is_empty())
        .take(3)
        .map(|i| format!("### Issue #{}: {}\n{}", i.number, i.title, i.body_excerpt))
        .collect();
    if !issues.is_empty() {
        prompt.push_str("\n\n## Related Issues\n\n");
        prompt.push_str(&issues.join("\n\n"));
    }

    let docs: Vec<String> = plan
        .linked_docs
        .iter()
        .take(3)
        .map(|d| format!("- `{}` ({}/{})", d.path, d.repo, d.kind))
        .collect();
    if !docs.is_empty() {
        prompt.push_str("\n\n## Related Documentation\n\n");
        prompt.push_str(&docs.join("\n"));
    }

    prompt.push_str("\n\nAnswer:");
    prompt
}

pub fn lang_hint(path: &str) -> &'static str {
    if path.ends_with(".rs") {
        "rust"
    } else if path.ends_with(".ts") || path.ends_with(".tsx") {
        "typescript"
    } else if path.ends_with(".py") {
        "python"
    } else if path.ends_with(".md") {
        "markdown"
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        "yaml"
    } else if path.ends_with(".toml") {
        "toml"
    } else if path.ends_with(".json") {
        "json"
    } else {
        ""
    }
}

pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intel::PlanningMetrics;
    use crate::knowledge::LinkedDocSummary;
    use crate::knowledge::LinkedIssueSummary;

    fn sample_plan(
        planning_result: PlanningResult,
        guidance: Vec<GuidanceSummary>,
    ) -> PlanResponse {
        PlanResponse {
            planning_source: "bindb",
            planning_result,
            fallback_required: false,
            fallback_reason: None,
            repository_discovery_recommended: false,
            primary_source: "bindb",
            inspection_set: vec![],
            resolved_execution_paths: vec![],
            unresolved: vec![],
            reasoning: String::new(),
            doctrine: vec![],
            impact: vec![],
            recent_changes: vec![],
            linked_issues: vec![],
            linked_prs: vec![],
            linked_docs: vec![],
            origin_issues: vec![],
            matched_kind: String::new(),
            file_roles: vec![],
            touch_sequence: vec![],
            matched_summary: String::new(),
            task_hint: String::new(),
            metrics: PlanningMetrics {
                planning_source: "bindb",
                planning_result,
                fallback_required: false,
                fallback_reason: None,
                inspection_set_size: 0,
                files_opened_from_inspection_set: 0,
                repository_search_performed: false,
            },
            guidance,
        }
    }

    #[test]
    fn correction_id_is_stable_for_same_question() {
        let a = correction_id("What is the bindb write boundary?");
        let b = correction_id("What is the bindb write boundary?");
        assert_eq!(a, b);
        assert!(a.starts_with("COR-"));
    }

    #[test]
    fn resolve_correction_id_prefers_explicit_bank_id() {
        assert_eq!(resolve_correction_id(Some("C1"), "any question"), "C1");
        assert!(resolve_correction_id(None, "What is make b?").starts_with("COR-"));
    }

    #[test]
    fn filter_paths_for_repo_excludes_other_repos() {
        let paths = vec![
            "apple-health-binder/README.md".into(),
            "codeintel/src/architecture.rs".into(),
            "bindb/README.md".into(),
        ];
        let scoped = filter_paths_for_repo(&paths, "codeintel");
        assert_eq!(scoped, vec!["codeintel/src/architecture.rs"]);
    }

    #[test]
    fn filter_guidance_prefers_repo_prefix() {
        let guidance = vec![
            GuidanceSummary {
                id: "A4".into(),
                question: "plan vs serve?".into(),
                answer: "Neither calls a model for plan().".into(),
                version: 1,
                match_score: 0,
            },
            GuidanceSummary {
                id: "C1".into(),
                question: "boundary?".into(),
                answer: "codeintel writes bindb.".into(),
                version: 1,
                match_score: 3,
            },
            GuidanceSummary {
                id: "X1".into(),
                question: "SPIDER?".into(),
                answer: "Specify Plan Implement Deploy Evaluate Retro.".into(),
                version: 1,
                match_score: 0,
            },
        ];
        let filtered = filter_guidance_for_repo(&guidance, "codeintel");
        assert_eq!(filtered[0].id, "C1");
        assert!(filtered.iter().any(|g| g.id == "X1"));
        assert!(!filtered.iter().any(|g| g.id == "A4"));
    }

    #[test]
    fn assess_rejects_cross_only_guidance_when_unresolved() {
        let guidance = filter_guidance_for_repo(
            &[GuidanceSummary {
                id: "X11".into(),
                question: "tests?".into(),
                answer: "mock only".into(),
                version: 1,
                match_score: 0,
            }],
            "foreman",
        );
        let plan = sample_plan(PlanningResult::Unresolved, guidance.clone());
        let err = assess_ask_context("foreman", &plan, &[], 0, &guidance).unwrap_err();
        assert_eq!(err, AskRejectReason::UnresolvedWithoutRepoContext);
    }

    #[test]
    fn assess_rejects_unresolved_without_repo_context() {
        let plan = sample_plan(
            PlanningResult::Unresolved,
            vec![GuidanceSummary {
                id: "X11".into(),
                question: "tests?".into(),
                answer: "mock only".into(),
                version: 1,
                match_score: 0,
            }],
        );
        let err = assess_ask_context("foreman", &plan, &[], 0, &[]).unwrap_err();
        assert_eq!(err, AskRejectReason::NoContext);
    }

    #[test]
    fn assess_allows_guidance_only_context() {
        let guidance = filter_guidance_for_repo(
            &[GuidanceSummary {
                id: "F9".into(),
                question: "make b?".into(),
                answer: "Build and deploy to dev.".into(),
                version: 1,
                match_score: 0,
            }],
            "foreman",
        );
        let plan = sample_plan(PlanningResult::Unresolved, guidance.clone());
        assert!(assess_ask_context("foreman", &plan, &[], 0, &guidance).is_ok());
    }

    #[test]
    fn filter_guidance_for_repo_dashboard_uses_r_prefix() {
        let guidance = vec![
            GuidanceSummary {
                id: "R1".into(),
                question: "boundary?".into(),
                answer: "repo-dashboard scans git; do not add Foreman intake.".into(),
                version: 1,
                match_score: 0,
            },
            GuidanceSummary {
                id: "C1".into(),
                question: "codeintel?".into(),
                answer: "domain layer.".into(),
                version: 1,
                match_score: 0,
            },
        ];
        let filtered = filter_guidance_for_repo(&guidance, "repo-dashboard");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "R1");
    }

    #[test]
    fn assess_allows_repo_dashboard_guidance_only() {
        let guidance = filter_guidance_for_repo(
            &[GuidanceSummary {
                id: "R1".into(),
                question: "boundary?".into(),
                answer: "repo-dashboard must not mutate QA.".into(),
                version: 1,
                match_score: 0,
            }],
            "repo-dashboard",
        );
        let plan = sample_plan(PlanningResult::Unresolved, guidance.clone());
        assert!(assess_ask_context("repo-dashboard", &plan, &[], 0, &guidance).is_ok());
    }

    #[test]
    fn ask_prompt_marks_guidance_authoritative_and_scopes_repo() {
        let guidance = vec![GuidanceSummary {
            id: "C1".into(),
            question: "What is the boundary?".into(),
            answer: "codeintel writes a Git-synced semantic graph into bindb.".into(),
            version: 1,
            match_score: 0,
        }];
        let prompt = build_ask_prompt(
            "codeintel",
            "What is the bindb write boundary?",
            &sample_plan(PlanningResult::Resolved, guidance.clone()),
            &[],
            &guidance,
            false,
        );
        assert!(prompt.contains("Authoritative answers"));
        assert!(prompt.contains("codeintel writes a Git-synced semantic graph"));
        assert!(prompt.contains("Scope your answer to **codeintel**"));
        assert!(prompt.contains("Do not hedge"));
        assert!(prompt.contains("Insufficient indexed context for codeintel"));
    }

    #[test]
    fn strict_docs_do_not_fallback_to_other_repos() {
        let docs = vec![LinkedDocSummary {
            path: "codev/plans/foo.md".into(),
            title: "plan".into(),
            repo: "foreman".into(),
            kind: "doc".into(),
        }];
        assert!(filter_linked_docs_strict_for_repo(&docs, "repo-dashboard").is_empty());
        assert_eq!(filter_linked_docs_for_repo(&docs, "repo-dashboard"), docs);
    }

    #[test]
    fn filter_issues_for_repo_excludes_other_repos() {
        let issues = vec![
            LinkedIssueSummary {
                number: 1,
                title: "foreman".into(),
                url: String::new(),
                repo: "foreman".into(),
                body_excerpt: "body".into(),
            },
            LinkedIssueSummary {
                number: 2,
                title: "dashboard".into(),
                url: String::new(),
                repo: "repo-dashboard".into(),
                body_excerpt: "body".into(),
            },
        ];
        let scoped = filter_linked_issues_for_repo(&issues, "repo-dashboard");
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].number, 2);
    }

    #[test]
    fn golden_binder_ai_plan_no_llm_in_guidance_prompt() {
        let guidance = vec![GuidanceSummary {
            id: "A4".into(),
            question: "Difference between binder-ai-plan and binder-ai-serve?".into(),
            answer: "Neither calls a model for plan(). binder-ai-serve exposes POST /plan and POST /explain.".into(),
            version: 1,
            match_score: 0,
        }];
        let prompt = build_ask_prompt(
            "binder-ai",
            "Does POST /plan call an LLM?",
            &sample_plan(PlanningResult::Resolved, guidance.clone()),
            &[],
            &guidance,
            false,
        );
        assert!(prompt.contains("A4"));
        assert!(prompt.contains("Neither calls a model for plan()"));
    }

    #[test]
    fn select_bank_answer_requires_min_score() {
        let guidance = vec![GuidanceSummary {
            id: "C1".into(),
            question: "boundary?".into(),
            answer: "bank text".into(),
            version: 1,
            match_score: 1,
        }];
        assert!(select_bank_answer("codeintel", &guidance).is_none());
        let guidance = vec![GuidanceSummary {
            id: "C1".into(),
            question: "boundary?".into(),
            answer: "bank text".into(),
            version: 1,
            match_score: 2,
        }];
        let hit = select_bank_answer("codeintel", &guidance).unwrap();
        assert_eq!(hit.0, "C1");
        assert_eq!(hit.1, "bank text");
        assert_eq!(hit.2, 2);
    }

    #[test]
    fn prefix_less_repo_gets_x_guidance_only() {
        let guidance = vec![
            GuidanceSummary {
                id: "C1".into(),
                question: "codeintel?".into(),
                answer: "domain".into(),
                version: 1,
                match_score: 3,
            },
            GuidanceSummary {
                id: "X3".into(),
                question: "propagation?".into(),
                answer: "cross".into(),
                version: 1,
                match_score: 2,
            },
        ];
        let filtered = filter_guidance_for_repo(&guidance, "keepsake");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "X3");
    }

    #[test]
    fn is_repo_known_accepts_prefix_or_indexed() {
        assert!(is_repo_known("codeintel", &[]));
        assert!(is_repo_known("keepsake", &["keepsake".into()]));
        assert!(!is_repo_known("keepsake", &[]));
        assert!(!is_repo_known(
            "nonexistent-repo-xyz",
            &["codeintel".into()]
        ));
    }

    #[test]
    fn inference_prompt_includes_low_confidence_disclaimer() {
        let prompt = build_ask_prompt(
            "codeintel",
            "kafka?",
            &sample_plan(PlanningResult::Resolved, vec![]),
            &[],
            &[],
            true,
        );
        assert!(prompt.contains("No authoritative question-bank row matched"));
        assert!(prompt.contains("codeintel/"));
    }
}
