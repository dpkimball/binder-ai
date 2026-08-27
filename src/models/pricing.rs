use serde::{Deserialize, Serialize};

/// Coarse cost class. Local inference is [`CostClass::Free`] (zero marginal API cost).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    #[default]
    Free,
    VeryLow,
    Low,
    Medium,
    High,
    Premium,
}

impl CostClass {
    pub fn rank(self) -> u8 {
        match self {
            Self::Free => 0,
            Self::VeryLow => 1,
            Self::Low => 2,
            Self::Medium => 3,
            Self::High => 4,
            Self::Premium => 5,
        }
    }

    pub fn within(self, max: CostClass) -> bool {
        self.rank() <= max.rank()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::VeryLow => "very_low",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Premium => "premium",
        }
    }
}

/// Optional USD-per-million-token hints. Unknown pricing must not break routing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Pricing {
    #[serde(default)]
    pub cost_class: CostClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_per_million: Option<f64>,
}

impl Pricing {
    pub fn free() -> Self {
        Self {
            cost_class: CostClass::Free,
            input_per_million: Some(0.0),
            output_per_million: Some(0.0),
        }
    }

    pub fn estimate_usd(self, prompt_tokens: u32, completion_tokens: u32) -> Option<f64> {
        let input = self.input_per_million?;
        let output = self.output_per_million.unwrap_or(input);
        Some(
            (prompt_tokens as f64 / 1_000_000.0) * input
                + (completion_tokens as f64 / 1_000_000.0) * output,
        )
    }
}
