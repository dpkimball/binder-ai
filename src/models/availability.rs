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
    pub fn is_selectable(self) -> bool {
        matches!(self, Self::Available | Self::Unknown)
    }
}
