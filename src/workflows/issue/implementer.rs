use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::client::AiClient;
use crate::prompt::PromptRef;
use crate::request::AiRequest;
use crate::task::TaskKind;
use crate::workflows::issue::planner::ImplementationPlan;
use crate::workflows::issue::request::Issue;
use crate::workflows::issue::result::WorkflowError;

/// Full-file replacement. Not a git patch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEdit {
    pub path: PathBuf,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct ImplementationOutput {
    #[serde(default)]
    edits: Vec<FileEdit>,
    #[serde(default)]
    summary: String,
}

pub struct GeneratedEdits {
    pub edits: Vec<FileEdit>,
    pub summary: String,
}

pub async fn implement(
    ai: &AiClient,
    issue: &Issue,
    plan: &ImplementationPlan,
    files: &[(PathBuf, String)],
) -> Result<GeneratedEdits, WorkflowError> {
    let mut context = String::new();
    for (path, contents) in files {
        context.push_str(&format!(
            "### {}\n```\n{}\n```\n\n",
            path.display(),
            contents
        ));
    }
    let input = format!(
        "Issue:\n{}\n\nPlan summary:\n{}\nSteps:\n{}\nFiles:\n{}\n\nCurrent files:\n{context}",
        issue.render(),
        plan.summary,
        plan.steps.join("\n"),
        plan.files
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut request = AiRequest::new(TaskKind::CodeGeneration, input);
    request.prompt = Some(PromptRef::latest("issue-implementation-edit"));
    request.json_mode = true;
    request.use_tools = false;
    let output: ImplementationOutput = ai
        .chat_json(request)
        .await
        .map_err(|err| WorkflowError::from_ai_implementation(&err))?;
    Ok(GeneratedEdits {
        edits: output.edits,
        summary: if output.summary.is_empty() {
            plan.summary.clone()
        } else {
            output.summary
        },
    })
}
