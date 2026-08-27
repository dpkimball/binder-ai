use crate::error::{AiError, Result};
use crate::routing::selector::ModelTarget;

/// Ordered provider attempts after the primary choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackChain {
    targets: Vec<ModelTarget>,
}

impl FallbackChain {
    pub fn single(target: ModelTarget) -> Self {
        Self {
            targets: vec![target],
        }
    }

    pub fn try_from_targets(targets: Vec<ModelTarget>) -> Result<Self> {
        if targets.is_empty() {
            return Err(AiError::NoEligibleModel);
        }
        Ok(Self { targets })
    }

    pub fn primary(&self) -> &ModelTarget {
        &self.targets[0]
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModelTarget> {
        self.targets.iter()
    }

    pub fn len(&self) -> usize {
        self.targets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}
