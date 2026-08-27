use crate::policy::RoutingPolicy;

/// Hygiene run options. Inspection is not configured here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneConfig {
    pub ai_remediation: bool,
    pub routing_policy: RoutingPolicy,
    pub max_ai_repositories: usize,
    pub dry_run: bool,
}

impl Default for HygieneConfig {
    fn default() -> Self {
        Self {
            ai_remediation: true,
            routing_policy: RoutingPolicy::LocalOnly,
            max_ai_repositories: usize::MAX,
            dry_run: false,
        }
    }
}

impl HygieneConfig {
    pub fn local() -> Self {
        Self::default()
    }
}
