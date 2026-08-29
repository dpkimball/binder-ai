use std::path::PathBuf;

/// Documentation review request. Findings come from the dashboard work item.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentationReviewRequest {
    pub request: String,
    pub repository: String,
    pub repository_root: PathBuf,
    pub finding_kind: Option<String>,
}
