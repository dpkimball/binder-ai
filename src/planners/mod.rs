use serde::{Deserialize, Serialize};

/// Single-shot plan. Multi-agent graphs are deferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Plan {
    #[serde(default)]
    pub steps: Vec<String>,
}

impl Plan {
    pub fn from_markdown(text: &str) -> Self {
        let steps = text
            .lines()
            .map(str::trim)
            .filter(|line| {
                line.starts_with('-')
                    || line.starts_with('*')
                    || line
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
            })
            .map(|line| {
                line.trim_start_matches(|c: char| {
                    c.is_ascii_digit() || c == '.' || c == '-' || c == '*' || c.is_whitespace()
                })
                .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();
        Self { steps }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numbered_list() {
        let plan = Plan::from_markdown("1. one\n2. two\n");
        assert_eq!(plan.steps, vec!["one", "two"]);
    }
}
