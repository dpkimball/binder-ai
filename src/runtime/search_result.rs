use std::path::PathBuf;

/// One match from [`super::Runtime::search`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub path: PathBuf,
    pub line: Option<u32>,
    pub preview: Option<String>,
}

impl SearchResult {
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            line: None,
            preview: None,
        }
    }
}
