use crate::workflows::hygiene::config::HygieneConfig;
use crate::workflows::hygiene::finding::{Finding, FindingKind, ProjectKind};
use crate::workflows::hygiene::remediation::{
    AutoRemediation, DeterministicAction, DispatchDecision,
};

/// Chooses deterministic repair, binder-ai, or report-only. No I/O.
pub fn dispatch(finding: &Finding, config: &HygieneConfig) -> DispatchDecision {
    match finding.kind {
        FindingKind::FeatureBranchMerged => match finding.target_ref.as_deref() {
            Some(name) if !name.is_empty() => {
                DispatchDecision::Deterministic(DeterministicAction::DeleteMergedBranch {
                    name: name.to_string(),
                })
            }
            _ => DispatchDecision::ReportOnly,
        },
        FindingKind::MissingBadge => match finding.github_slug.as_deref() {
            Some(slug) if !slug.is_empty() => {
                DispatchDecision::Deterministic(DeterministicAction::InsertBadge {
                    github_slug: slug.to_string(),
                })
            }
            _ => DispatchDecision::ReportOnly,
        },
        FindingKind::CargoFmtRequired => {
            DispatchDecision::Deterministic(DeterministicAction::CargoFmt)
        }
        FindingKind::ClippyWarnings => {
            DispatchDecision::Deterministic(DeterministicAction::ClippyFix)
        }
        FindingKind::MissingTests | FindingKind::RustBug | FindingKind::DocumentationIssue => {
            rust_ai_or_report(finding, config)
        }
        FindingKind::FeatureBranchStale
        | FindingKind::OpenPullRequest
        | FindingKind::DirtyRepository
        | FindingKind::BrokenPipeline
        | FindingKind::Unknown => DispatchDecision::ReportOnly,
    }
}

fn rust_ai_or_report(finding: &Finding, config: &HygieneConfig) -> DispatchDecision {
    if !config.ai_remediation {
        return DispatchDecision::ReportOnly;
    }
    if finding.project != ProjectKind::RustCargo {
        return DispatchDecision::ReportOnly;
    }
    DispatchDecision::Ai(AutoRemediation::RustImplementation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::hygiene::finding::Finding;

    fn rust_bug() -> Finding {
        Finding::new("demo", FindingKind::RustBug, "add saturates").rust_cargo()
    }

    #[test]
    fn merged_branch_is_deterministic() {
        let finding = Finding::new("demo", FindingKind::FeatureBranchMerged, "merged")
            .target_ref("feature/old");
        match dispatch(&finding, &HygieneConfig::default()) {
            DispatchDecision::Deterministic(DeterministicAction::DeleteMergedBranch { name }) => {
                assert_eq!(name, "feature/old");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rust_bug_is_ai() {
        match dispatch(&rust_bug(), &HygieneConfig::default()) {
            DispatchDecision::Ai(AutoRemediation::RustImplementation) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn python_rust_bug_is_report_only() {
        let finding = Finding::new("py", FindingKind::RustBug, "n/a");
        assert_eq!(
            dispatch(&finding, &HygieneConfig::default()),
            DispatchDecision::ReportOnly
        );
    }

    #[test]
    fn ai_disabled_reports_rust_findings() {
        let config = HygieneConfig {
            ai_remediation: false,
            ..HygieneConfig::default()
        };
        assert_eq!(dispatch(&rust_bug(), &config), DispatchDecision::ReportOnly);
    }

    #[test]
    fn fmt_is_deterministic() {
        let finding = Finding::new("demo", FindingKind::CargoFmtRequired, "fmt");
        assert!(matches!(
            dispatch(&finding, &HygieneConfig::default()),
            DispatchDecision::Deterministic(DeterministicAction::CargoFmt)
        ));
    }
}
