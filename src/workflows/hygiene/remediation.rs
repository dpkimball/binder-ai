/// Future AI remediation kinds. V1 implements only [`Self::RustImplementation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoRemediation {
    RustImplementation,
    PythonImplementation,
    DocumentationUpdate,
    CiRepair,
    DependencyUpgrade,
    SecurityFix,
}

impl AutoRemediation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RustImplementation => "rust_implementation",
            Self::PythonImplementation => "python_implementation",
            Self::DocumentationUpdate => "documentation_update",
            Self::CiRepair => "ci_repair",
            Self::DependencyUpgrade => "dependency_upgrade",
            Self::SecurityFix => "security_fix",
        }
    }
}

/// Deterministic actions. Never invoke [`crate::AiClient`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeterministicAction {
    DeleteMergedBranch { name: String },
    CargoFmt,
    ClippyFix,
    InsertBadge { github_slug: String },
}

/// How Hygiene will treat a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchDecision {
    Deterministic(DeterministicAction),
    Ai(AutoRemediation),
    ReportOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemediationType {
    Deterministic,
    Ai,
    ReportOnly,
}

impl RemediationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deterministic => "deterministic",
            Self::Ai => "ai",
            Self::ReportOnly => "report_only",
        }
    }
}

impl std::fmt::Display for RemediationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
