use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::client::AiClient;
use crate::prompt::PromptRef;
use crate::request::AiRequest;
use crate::task::TaskKind;
use crate::workflows::issue::request::Issue;
use crate::workflows::issue::result::WorkflowError;

/// Structured plan from the planner. JSON on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImplementationPlan {
    #[serde(default)]
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub summary: String,
}

pub async fn plan(
    ai: &AiClient,
    issue: &Issue,
    files: &[(PathBuf, String)],
) -> Result<ImplementationPlan, WorkflowError> {
    let mut context = String::new();
    for (path, contents) in files {
        context.push_str(&format!(
            "### {}\n```\n{}\n```\n\n",
            path.display(),
            contents
        ));
    }
    let input = format!(
        "Repository context:\n{context}\nIssue:\n{}\n\nDescribe files to modify, implementation approach, and risks.",
        issue.render()
    );
    let mut request = AiRequest::new(TaskKind::Planning, input);
    request.prompt = Some(PromptRef::latest("issue-implementation-plan"));
    request.json_mode = true;
    request.use_tools = false;
    ai.chat_json(request)
        .await
        .map_err(|err| WorkflowError::from_ai_planning(&err))
}

/// Deterministic tokens from title + description. No embeddings.
pub fn search_terms(issue: &Issue) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = BTreeSet::new();
    if !issue.title.trim().is_empty() {
        let title = issue.title.trim().to_string();
        seen.insert(normalize(&title));
        terms.push(title);
    }
    for raw in issue
        .title
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .chain(
            issue
                .description
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-'),
        )
    {
        let token = raw.trim();
        if token.len() < 3 || is_stopword(token) {
            continue;
        }
        let key = normalize(token);
        if seen.insert(key) {
            terms.push(token.to_string());
        }
        if terms.len() >= 8 {
            break;
        }
    }
    terms
}

fn normalize(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "the"
            | "and"
            | "for"
            | "that"
            | "this"
            | "with"
            | "from"
            | "into"
            | "should"
            | "must"
            | "have"
            | "issue"
            | "fix"
            | "add"
            | "implement"
            | "please"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nouns_from_title() {
        let issue = Issue::new("Overflow in add helper", "src/lib.rs should saturating_add");
        let terms = search_terms(&issue);
        assert!(terms.iter().any(|t| t.contains("Overflow")));
        assert!(terms
            .iter()
            .any(|t| t == "helper" || t == "lib" || t.contains("saturating")));
    }
}
