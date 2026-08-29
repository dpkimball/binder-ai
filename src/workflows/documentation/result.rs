use serde::Serialize;

/// One documentation gap. Review result, not a workflow failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentationGap {
    pub kind: String,
    pub summary: String,
}

/// Outcome of [`super::DocumentationReviewWorkflow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentationReviewReport {
    pub success: bool,
    pub workflow: &'static str,
    pub llm_invoked: bool,
    pub validation_passed: bool,
    pub history_recorded: bool,
    pub search_used: bool,
    pub primary_source: &'static str,
    pub files_inspected: Vec<String>,
    pub gaps: Vec<DocumentationGap>,
    pub summary: String,
}
