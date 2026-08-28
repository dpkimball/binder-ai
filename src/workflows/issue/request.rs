use std::path::PathBuf;

/// Issue as supplied by a caller. GitHub fetch is out of scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub title: String,
    pub description: String,
}

impl Issue {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
        }
    }

    pub fn render(&self) -> String {
        format!("{}\n\n{}", self.title, self.description)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueImplementationRequest {
    pub issue: Issue,
    pub repository_root: PathBuf,
    /// When set, read these graph paths only. Do not search the repository.
    pub inspection_set: Option<Vec<PathBuf>>,
}

impl IssueImplementationRequest {
    pub fn new(issue: Issue, repository_root: impl Into<PathBuf>) -> Self {
        Self {
            issue,
            repository_root: repository_root.into(),
            inspection_set: None,
        }
    }

    pub fn with_inspection_set(mut self, files: impl Into<Vec<PathBuf>>) -> Self {
        self.inspection_set = Some(files.into());
        self
    }
}
