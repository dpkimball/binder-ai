use serde::{Deserialize, Serialize};

/// Whether a catalog entry can be selected right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable,
    #[default]
    Unknown,
}

impl Availability {
    /// Only [`Self::Available`] is selectable. [`Self::Unknown`] means not yet
    /// discovered; the effective catalog must resolve local models to Available
    /// or Unavailable before routing.
    pub fn is_selectable(self) -> bool {
        matches!(self, Self::Available)
    }
}
