use crate::context::sources::ContextSource;

/// Joins context sources into a single block for the user turn.
#[derive(Debug, Clone, Default)]
pub struct ContextBuilder {
    sources: Vec<ContextSource>,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(mut self, source: ContextSource) -> Self {
        self.sources.push(source);
        self
    }

    pub fn extend(mut self, sources: impl IntoIterator<Item = ContextSource>) -> Self {
        self.sources.extend(sources);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn build(&self) -> String {
        self.sources
            .iter()
            .map(|source| format!("## {}\n{}", source.heading(), source.body()))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ContextSource;

    #[test]
    fn composes_sections() {
        let text = ContextBuilder::new()
            .push(ContextSource::Repository("binder-ai".into()))
            .push(ContextSource::GitDiff("diff --git a/x".into()))
            .build();
        assert!(text.contains("## Repository"));
        assert!(text.contains("## Git Diff"));
        assert!(text.contains("binder-ai"));
    }
}
