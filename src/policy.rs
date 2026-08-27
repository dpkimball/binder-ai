use serde::{Deserialize, Serialize};

/// Application preference for how the router should pick a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutingPolicy {
    BestQuality,
    Fastest,
    LowestCost,
    LocalOnly,
    CloudOnly,
    #[default]
    Balanced,
}

impl RoutingPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BestQuality => "best_quality",
            Self::Fastest => "fastest",
            Self::LowestCost => "lowest_cost",
            Self::LocalOnly => "local_only",
            Self::CloudOnly => "cloud_only",
            Self::Balanced => "balanced",
        }
    }

    pub fn allows_local(self) -> bool {
        !matches!(self, Self::CloudOnly)
    }

    pub fn allows_cloud(self) -> bool {
        !matches!(self, Self::LocalOnly)
    }
}

impl std::fmt::Display for RoutingPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_only_rejects_cloud() {
        assert!(RoutingPolicy::LocalOnly.allows_local());
        assert!(!RoutingPolicy::LocalOnly.allows_cloud());
        assert!(!RoutingPolicy::CloudOnly.allows_local());
    }
}
