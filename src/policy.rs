use serde::{Deserialize, Serialize};

/// How the selector may pick among catalog entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPolicy {
    LocalOnly,
    PreferLocal,
    LowestCost,
    #[default]
    Balanced,
    BestQuality,
    CloudOnly,
    FreeFirst,
    /// Hard privacy: same selection as [`Self::LocalOnly`].
    Private,
    Fastest,
}

/// Backward-compatible name used by the first crate slice.
pub type RoutingPolicy = ExecutionPolicy;

impl ExecutionPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalOnly => "local_only",
            Self::PreferLocal => "prefer_local",
            Self::LowestCost => "lowest_cost",
            Self::Balanced => "balanced",
            Self::BestQuality => "best_quality",
            Self::CloudOnly => "cloud_only",
            Self::FreeFirst => "free_first",
            Self::Private => "private",
            Self::Fastest => "fastest",
        }
    }

    pub fn allows_local(self) -> bool {
        !matches!(self, Self::CloudOnly)
    }

    pub fn allows_cloud(self) -> bool {
        !matches!(self, Self::LocalOnly | Self::Private)
    }

    pub fn is_private(self) -> bool {
        matches!(self, Self::LocalOnly | Self::Private)
    }
}

impl std::fmt::Display for ExecutionPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_only_rejects_cloud() {
        assert!(ExecutionPolicy::LocalOnly.allows_local());
        assert!(!ExecutionPolicy::LocalOnly.allows_cloud());
        assert!(!ExecutionPolicy::Private.allows_cloud());
        assert!(!ExecutionPolicy::CloudOnly.allows_local());
        assert!(ExecutionPolicy::FreeFirst.allows_cloud());
    }
}
