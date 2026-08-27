use std::path::PathBuf;

/// What Hygiene observed. Inspection is deterministic; this is the typed result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FindingKind {
    FeatureBranchMerged,
    FeatureBranchStale,
    OpenPullRequest,
    DirtyRepository,
    BrokenPipeline,
    MissingBadge,
    CargoFmtRequired,
    ClippyWarnings,
    MissingTests,
    RustBug,
    DocumentationIssue,
    Unknown,
}

impl FindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FeatureBranchMerged => "feature_branch_merged",
            Self::FeatureBranchStale => "feature_branch_stale",
            Self::OpenPullRequest => "open_pull_request",
            Self::DirtyRepository => "dirty_repository",
            Self::BrokenPipeline => "broken_pipeline",
            Self::MissingBadge => "missing_badge",
            Self::CargoFmtRequired => "cargo_fmt_required",
            Self::ClippyWarnings => "clippy_warnings",
            Self::MissingTests => "missing_tests",
            Self::RustBug => "rust_bug",
            Self::DocumentationIssue => "documentation_issue",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for FindingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    Low,
    Medium,
    High,
}

/// Language/ecosystem of the repository. V1 AI remediation is Rust Cargo only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectKind {
    RustCargo,
    Other,
}

/// A single deterministic inspection result. Hygiene produces these; the
/// dispatcher decides what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub repository: String,
    pub repository_root: PathBuf,
    pub project: ProjectKind,
    pub kind: FindingKind,
    pub severity: Severity,
    pub description: String,
    pub suggested_remediation: String,
    /// Branch name for merge/stale cleanup.
    pub target_ref: Option<String>,
    /// `owner/repo` for badge insertion.
    pub github_slug: Option<String>,
}

impl Finding {
    pub fn new(
        repository: impl Into<String>,
        kind: FindingKind,
        description: impl Into<String>,
    ) -> Self {
        let repository = repository.into();
        Self {
            repository_root: PathBuf::from("."),
            repository,
            project: ProjectKind::Other,
            kind,
            severity: Severity::Medium,
            description: description.into(),
            suggested_remediation: String::new(),
            target_ref: None,
            github_slug: None,
        }
    }

    pub fn rust_cargo(mut self) -> Self {
        self.project = ProjectKind::RustCargo;
        self
    }

    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn remediation(mut self, text: impl Into<String>) -> Self {
        self.suggested_remediation = text.into();
        self
    }

    pub fn target_ref(mut self, name: impl Into<String>) -> Self {
        self.target_ref = Some(name.into());
        self
    }

    pub fn github_slug(mut self, slug: impl Into<String>) -> Self {
        self.github_slug = Some(slug.into());
        self
    }
}
