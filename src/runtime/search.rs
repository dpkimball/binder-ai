use serde::{Deserialize, Serialize};

/// What to match when searching a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    #[default]
    Content,
    Filename,
}

/// Search constraints. Filters are additive (all must match).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default)]
    pub kind: SearchKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclusions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
}

impl SearchRequest {
    pub fn content(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            kind: SearchKind::Content,
            glob: None,
            extensions: Vec::new(),
            exclusions: Vec::new(),
            max_results: None,
        }
    }

    pub fn filename(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            kind: SearchKind::Filename,
            glob: None,
            extensions: Vec::new(),
            exclusions: Vec::new(),
            max_results: None,
        }
    }

    pub fn glob(mut self, glob: impl Into<String>) -> Self {
        self.glob = Some(glob.into());
        self
    }

    pub fn extension(mut self, ext: impl Into<String>) -> Self {
        self.extensions.push(ext.into());
        self
    }

    pub fn exclude(mut self, pattern: impl Into<String>) -> Self {
        self.exclusions.push(pattern.into());
        self
    }

    pub fn max_results(mut self, max: usize) -> Self {
        self.max_results = Some(max);
        self
    }

    pub(crate) fn matches_path(&self, relative: &str) -> bool {
        if !self.exclusions.is_empty() {
            let lower = relative.replace('\\', "/");
            if self.exclusions.iter().any(|ex| {
                let needle = ex.trim_start_matches("./").replace('\\', "/");
                lower.contains(&needle) || file_name(relative).contains(ex.as_str())
            }) {
                return false;
            }
        }
        if !self.extensions.is_empty() {
            let ext = extension_of(relative);
            let ok = self.extensions.iter().any(|wanted| {
                let w = wanted.trim_start_matches('.');
                ext == w
            });
            if !ok {
                return false;
            }
        }
        if let Some(glob) = &self.glob {
            if !glob_matches(glob, relative) {
                return false;
            }
        }
        true
    }
}

fn file_name(relative: &str) -> &str {
    relative.rsplit(['/', '\\']).next().unwrap_or(relative)
}

fn extension_of(relative: &str) -> &str {
    file_name(relative)
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("")
}

fn glob_matches(pattern: &str, relative: &str) -> bool {
    let normalized = relative.replace('\\', "/");
    match globset::Glob::new(pattern) {
        Ok(glob) => glob.compile_matcher().is_match(&normalized),
        Err(_) => normalized.contains(pattern),
    }
}
