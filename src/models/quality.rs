use serde::{Deserialize, Serialize};

use crate::task::TaskKind;

/// Routing hints on a 0–5 scale. Not benchmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct QualityProfile {
    #[serde(default)]
    pub general: u8,
    #[serde(default)]
    pub coding: u8,
    #[serde(default)]
    pub reasoning: u8,
    #[serde(default)]
    pub tool_use: u8,
}

impl QualityProfile {
    pub fn clamp(self) -> Self {
        Self {
            general: self.general.min(5),
            coding: self.coding.min(5),
            reasoning: self.reasoning.min(5),
            tool_use: self.tool_use.min(5),
        }
    }

    pub fn for_task(self, task: TaskKind) -> u8 {
        match task {
            TaskKind::CodeGeneration | TaskKind::Refactoring | TaskKind::CodeReview => self.coding,
            TaskKind::Planning
            | TaskKind::IssuePlanning
            | TaskKind::Reasoning
            | TaskKind::ArchitectureReview => self.reasoning,
            TaskKind::Embedding => self.general,
            _ => self.general,
        }
    }
}
