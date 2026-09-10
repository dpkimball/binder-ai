use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    #[serde(alias = "content", alias = "text")]
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct ImplementationOutput {
    #[serde(default, alias = "files", alias = "changes")]
    edits: Vec<FileEdit>,
    #[serde(default)]
    summary: String,
}

#[derive(Debug)]
pub struct GeneratedEdits {
    pub edits: Vec<FileEdit>,
    pub summary: String,
}

/// Local models (mistral) cannot emit six full-file replacements in one
/// JSON object. Ask for one inspect-set file at a time.
pub async fn implement(
    ai: &AiClient,
    issue: &Issue,
    plan: &ImplementationPlan,
    files: &[(PathBuf, String)],
) -> Result<GeneratedEdits, WorkflowError> {
    let targets = target_files(plan, files);
    if targets.is_empty() {
        return Err(WorkflowError::ImplementationFailed {
            message: "empty_edits".into(),
        });
    }
    let mut edits = Vec::new();
    let mut summaries = Vec::new();
    for (path, original) in targets {
        if let Some(round) = implement_one(ai, issue, plan, path, original).await? {
            if let Some(edit) = round.matching {
                if edit.contents != original {
                    if !edit.summary.is_empty() {
                        summaries.push(edit.summary.clone());
                    }
                    edits.push(FileEdit {
                        path: edit.path,
                        contents: edit.contents,
                    });
                }
            }
            // Keep paths the model named that we did not ask for so the
            // workflow can refuse them. Dropping them looked like empty_edits.
            edits.extend(round.extras);
        }
    }
    if edits.is_empty() {
        return Err(WorkflowError::ImplementationFailed {
            message: "empty_edits".into(),
        });
    }
    Ok(GeneratedEdits {
        edits,
        summary: if summaries.is_empty() {
            plan.summary.clone()
        } else {
            summaries.join(" ")
        },
    })
}

struct OneEdit {
    path: PathBuf,
    contents: String,
    summary: String,
}

struct OneRound {
    matching: Option<OneEdit>,
    extras: Vec<FileEdit>,
}

fn path_matches(edit: &Path, requested: &Path) -> bool {
    edit.as_os_str().is_empty()
        || edit == requested
        || edit.file_name() == requested.file_name()
        || edit.ends_with(requested)
        || requested.ends_with(edit)
}

async fn implement_one(
    ai: &AiClient,
    issue: &Issue,
    plan: &ImplementationPlan,
    path: &Path,
    original: &str,
) -> Result<Option<OneRound>, WorkflowError> {
    let path_display = path.display();
    for attempt in 0..2 {
        let retry_note = if attempt == 0 {
            String::new()
        } else {
            format!(
                "\n\nYour previous reply had no replacement for `{path_display}`. Return JSON with non-empty `contents` for this file only."
            )
        };
        let input = format!(
            "Issue:\n{}\n\nPlan summary:\n{}\nSteps:\n{}\n\nReplace this one file. JSON keys: path, contents.\n### {path_display}\n```\n{original}\n```{retry_note}",
            issue.render(),
            plan.summary,
            plan.steps.join("\n"),
        );
        let mut request = AiRequest::new(TaskKind::CodeGeneration, input);
        request.prompt = Some(PromptRef::latest("issue-implementation-edit"));
        request.json_mode = true;
        request.json_schema = Some(file_edit_schema());
        request.use_tools = false;
        let value = ai
            .chat_json::<Value>(request)
            .await
            .map_err(|err| WorkflowError::from_ai_implementation(&err))?;
        let output = implementation_output_from_value(value)?;
        let summary = output.summary;
        let mut matching = None;
        let mut extras = Vec::new();
        for edit in output.edits {
            if edit.contents.trim().is_empty() {
                continue;
            }
            if path_matches(&edit.path, path) {
                matching = Some(OneEdit {
                    path: path.to_path_buf(),
                    contents: edit.contents,
                    summary: summary.clone(),
                });
            } else {
                extras.push(edit);
            }
        }
        if matching.is_none() && extras.is_empty() {
            continue;
        }
        return Ok(Some(OneRound { matching, extras }));
    }
    Ok(None)
}

fn target_files<'a>(
    plan: &ImplementationPlan,
    files: &'a [(PathBuf, String)],
) -> Vec<(&'a PathBuf, &'a str)> {
    if !plan.files.is_empty() {
        let wanted: Vec<&PathBuf> = plan.files.iter().collect();
        let selected: Vec<_> = files
            .iter()
            .filter(|(path, _)| {
                wanted.iter().any(|want| {
                    path == *want
                        || path.file_name() == want.file_name()
                        || path.ends_with(want)
                        || want.ends_with(path)
                })
            })
            .map(|(p, c)| (p, c.as_str()))
            .collect();
        if !selected.is_empty() {
            return selected;
        }
    }
    files.iter().map(|(p, c)| (p, c.as_str())).collect()
}

fn file_edit_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string" },
            "contents": { "type": "string" },
            "content": { "type": "string" },
            "summary": { "type": "string" },
            "edits": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "contents": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path"]
                }
            }
        }
    })
}

fn implementation_output_from_value(value: Value) -> Result<ImplementationOutput, WorkflowError> {
    if value.get("edits").is_some()
        || value.get("files").is_some()
        || value.get("changes").is_some()
    {
        return serde_json::from_value(value).map_err(|err| WorkflowError::ImplementationFailed {
            message: format!("invalid implementation JSON: {err}"),
        });
    }
    if value.get("path").is_some()
        && (value.get("contents").is_some()
            || value.get("content").is_some()
            || value.get("text").is_some())
    {
        let summary = value
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let edit: FileEdit =
            serde_json::from_value(value).map_err(|err| WorkflowError::ImplementationFailed {
                message: format!("invalid implementation JSON: {err}"),
            })?;
        return Ok(ImplementationOutput {
            edits: vec![edit],
            summary,
        });
    }
    serde_json::from_value(value).map_err(|err| WorkflowError::ImplementationFailed {
        message: format!("invalid implementation JSON: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{implement, implementation_output_from_value, target_files, FileEdit};
    use crate::mock::ScriptedProvider;
    use crate::workflows::issue::planner::ImplementationPlan;
    use crate::workflows::issue::request::Issue;
    use crate::AiClient;
    use binder_llm::{ChatMessage, ChatResponse, LlmClient, Provider};
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn llm(
        provider: impl binder_llm::LlmProvider + 'static,
        kind: Provider,
        model: &str,
    ) -> LlmClient {
        let mut builder = LlmClient::builder().provider(kind).model(model);
        if kind.requires_api_key() {
            builder = builder.api_key("test-key");
        }
        let config = builder.build_config().expect("config");
        LlmClient::from_provider(Arc::new(provider), config)
    }

    fn assistant(text: &str) -> ChatResponse {
        ChatResponse::new(Provider::Ollama, "mock", ChatMessage::assistant(text))
    }

    #[test]
    fn accepts_single_file_object_and_aliases() {
        let value = json!({
            "path": "CLAUDE.md",
            "content": "hello",
        });
        let out = implementation_output_from_value(value).unwrap();
        assert_eq!(out.edits.len(), 1);
        assert_eq!(out.edits[0].path, PathBuf::from("CLAUDE.md"));
        assert_eq!(out.edits[0].contents, "hello");
    }

    #[test]
    fn accepts_files_alias() {
        let value = json!({
            "files": [{ "path": "a.rs", "text": "fn x() {}" }]
        });
        let out = implementation_output_from_value(value).unwrap();
        assert_eq!(
            out.edits,
            vec![FileEdit {
                path: PathBuf::from("a.rs"),
                contents: "fn x() {}".into(),
            }]
        );
    }

    #[test]
    fn prefers_plan_files_that_were_inspected() {
        let plan = ImplementationPlan {
            files: vec![PathBuf::from("CLAUDE.md")],
            steps: Vec::new(),
            summary: String::new(),
        };
        let files = vec![
            (PathBuf::from("src/lib.rs"), "fn a() {}".into()),
            (PathBuf::from("CLAUDE.md"), "# doc\n".into()),
        ];
        let targets = target_files(&plan, &files);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, &PathBuf::from("CLAUDE.md"));
    }

    #[tokio::test]
    async fn empty_model_json_is_empty_edits() {
        let scripted = ScriptedProvider::new()
            .push_chat(assistant("{}"))
            .push_chat(assistant(r#"{"edits":[]}"#));
        let ai = AiClient::builder()
            .llm(llm(scripted, Provider::Ollama, "mistral"))
            .build();
        let issue = Issue::new("docs: fix naming", "Align CLAUDE.md");
        let plan = ImplementationPlan {
            files: vec![PathBuf::from("CLAUDE.md")],
            steps: vec!["rename fallback_required".into()],
            summary: "align docs".into(),
        };
        let files = vec![(PathBuf::from("CLAUDE.md"), "# old\n".into())];
        let err = implement(&ai, &issue, &plan, &files).await.unwrap_err();
        assert_eq!(err.kind(), "implementation_failed");
        assert!(err.to_string().contains("empty_edits"));
    }

    #[tokio::test]
    async fn extra_json_paths_are_returned_for_the_workflow_to_refuse() {
        let scripted = ScriptedProvider::new().push_chat(assistant(
            "{\"edits\":[{\"path\":\"CLAUDE.md\",\"contents\":\"# old\\n\"},{\"path\":\"README.md\",\"contents\":\"leaked\\n\"}]}",
        ));
        let ai = AiClient::builder()
            .llm(llm(scripted, Provider::Ollama, "mistral"))
            .build();
        let issue = Issue::new("docs: fix naming", "Align CLAUDE.md");
        let plan = ImplementationPlan {
            files: vec![PathBuf::from("CLAUDE.md")],
            steps: Vec::new(),
            summary: String::new(),
        };
        let files = vec![(PathBuf::from("CLAUDE.md"), "# old\n".into())];
        let out = implement(&ai, &issue, &plan, &files).await.unwrap();
        assert!(out
            .edits
            .iter()
            .any(|e| e.path.as_os_str() == "README.md" && e.contents == "leaked\n"));
        assert!(!out.edits.iter().any(|e| e.path.as_os_str() == "CLAUDE.md"));
    }

    #[tokio::test]
    async fn per_file_json_writes_replacement() {
        let scripted = ScriptedProvider::new().push_chat(assistant(
            "{\"path\":\"CLAUDE.md\",\"contents\":\"# new\\n\"}",
        ));
        let ai = AiClient::builder()
            .llm(llm(scripted, Provider::Ollama, "mistral"))
            .build();
        let issue = Issue::new("docs: fix naming", "Align CLAUDE.md");
        let plan = ImplementationPlan {
            files: vec![PathBuf::from("CLAUDE.md")],
            steps: Vec::new(),
            summary: String::new(),
        };
        let files = vec![(PathBuf::from("CLAUDE.md"), "# old\n".into())];
        let out = implement(&ai, &issue, &plan, &files).await.unwrap();
        assert_eq!(out.edits.len(), 1);
        assert_eq!(out.edits[0].contents, "# new\n");
    }

    #[tokio::test]
    async fn fenced_json_from_model_is_accepted() {
        let scripted = ScriptedProvider::new().push_chat(assistant(
            "```json\n{\"path\":\"CLAUDE.md\",\"contents\":\"# new\\n\"}\n```",
        ));
        let ai = AiClient::builder()
            .llm(llm(scripted, Provider::Ollama, "mistral"))
            .build();
        let issue = Issue::new("docs: fix naming", "Align CLAUDE.md");
        let plan = ImplementationPlan::default();
        let files = vec![(PathBuf::from("CLAUDE.md"), "# old\n".into())];
        let out = implement(&ai, &issue, &plan, &files).await.unwrap();
        assert_eq!(out.edits[0].contents, "# new\n");
    }
}
