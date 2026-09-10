use serde::{Deserialize, Serialize};

use crate::error::{AiError, Result};

/// Reference to a named, optionally version-pinned prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptRef {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
}

impl PromptRef {
    pub fn latest(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
        }
    }

    pub fn versioned(name: impl Into<String>, version: u32) -> Self {
        Self {
            name: name.into(),
            version: Some(version),
        }
    }
}

/// A single immutable prompt revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptVersion {
    pub name: String,
    pub version: u32,
    pub template: String,
}

impl PromptVersion {
    pub fn id(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

/// `{variable}` substitution. Unknown keys are left in place.
pub fn render_template(template: &str, variables: &[(String, String)]) -> String {
    let mut out = template.to_string();
    for (key, value) in variables {
        let needle = format!("{{{key}}}");
        out = out.replace(&needle, value);
    }
    out
}

/// Versioned prompt catalog. Applications register names, not raw text, at call sites.
#[derive(Debug, Clone, Default)]
pub struct PromptRegistry {
    prompts: Vec<PromptVersion>,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builtin() -> Self {
        let mut registry = Self::new();
        for prompt in builtin_prompts() {
            registry.register(prompt);
        }
        registry
    }

    pub fn register(&mut self, prompt: PromptVersion) {
        self.prompts.push(prompt);
    }

    pub fn get(&self, name: &str, version: Option<u32>) -> Result<&PromptVersion> {
        let mut matches: Vec<&PromptVersion> =
            self.prompts.iter().filter(|p| p.name == name).collect();
        if matches.is_empty() {
            return Err(AiError::prompt(format!("unknown prompt '{name}'")));
        }
        matches.sort_by_key(|p| p.version);
        match version {
            Some(v) => matches
                .into_iter()
                .find(|p| p.version == v)
                .ok_or_else(|| AiError::prompt(format!("prompt '{name}' has no version {v}"))),
            None => Ok(*matches.last().expect("non-empty")),
        }
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.prompts.iter().map(|p| p.name.clone()).collect();
        names.sort();
        names.dedup();
        names
    }
}

fn builtin_prompts() -> Vec<PromptVersion> {
    vec![
        PromptVersion {
            name: "health-insight".into(),
            version: 1,
            template: "You are a personal running coach analysing one month of training data. \
                       Respond in 2–3 plain-English sentences. Surface the single most actionable \
                       insight — focus on the clearest pattern between sleep, load, and pace. \
                       Do not list raw numbers; interpret them. Be direct and encouraging.\n\n\
                       Training data:\n{input}".into(),
        },
        PromptVersion {
            name: "summarization".into(),
            version: 1,
            template: "Summarize the following for a Binder operator. Be concise.\n\n{input}".into(),
        },
        PromptVersion {
            name: "classification".into(),
            version: 1,
            template: "Classify the text. Reply with JSON {\"label\": string, \"confidence\": number}.\n\n{input}".into(),
        },
        PromptVersion {
            name: "extraction".into(),
            version: 1,
            template: "Extract structured fields. Reply with JSON only.\n\n{input}".into(),
        },
        PromptVersion {
            name: "planning".into(),
            version: 1,
            template: "Produce a numbered plan. Reply with JSON {\"steps\": [string]}.\n\nGoal:\n{input}".into(),
        },
        PromptVersion {
            name: "issue-planning".into(),
            version: 1,
            template: "Plan implementation of this GitHub issue. Reply with JSON {\"steps\": [string]}.\n\n{input}".into(),
        },
        PromptVersion {
            name: "code-generation".into(),
            version: 1,
            template: "Write the requested code. Prefer small, compilable changes.\n\n{input}".into(),
        },
        PromptVersion {
            name: "code-review".into(),
            version: 1,
            template: "Review the following diff. List defects, risks, and missing tests.\n\n{input}".into(),
        },
        PromptVersion {
            name: "architecture-review".into(),
            version: 1,
            template: "Review architecture. List risks and recommendations.\n\n{input}".into(),
        },
        PromptVersion {
            name: "architecture-review".into(),
            version: 2,
            template: "Review architecture against Binder patterns (typed contracts, no provider leakage, DPK make b). List blockers then suggestions.\n\n{input}".into(),
        },
        PromptVersion {
            name: "architecture-review".into(),
            version: 3,
            template: "You are Binder's architecture reviewer. Check pattern-fit, maintainability, and missing tests. Output markdown with Blockers / Risks / Follow-ups.\n\n{input}".into(),
        },
        PromptVersion {
            name: "refactoring".into(),
            version: 1,
            template: "Refactor as instructed. Preserve behavior. Show the resulting code.\n\n{input}".into(),
        },
        PromptVersion {
            name: "reasoning".into(),
            version: 1,
            template: "Reason step by step, then give the answer.\n\n{input}".into(),
        },
        PromptVersion {
            name: "conversation".into(),
            version: 1,
            template: "{input}".into(),
        },
        PromptVersion {
            name: "moderation".into(),
            version: 1,
            template: "Decide if the content is allowed. Reply with JSON {\"allowed\": bool, \"reason\": string}.\n\n{input}".into(),
        },
        PromptVersion {
            name: "issue-implementation-plan".into(),
            version: 1,
            template: "Reply with JSON only: {\"files\": [string], \"steps\": [string], \"summary\": string}. Do not include markdown.\n\n{input}".into(),
        },
        PromptVersion {
            name: "issue-implementation-edit".into(),
            version: 1,
            template: "Reply with JSON only: {\"edits\": [{\"path\": string, \"contents\": string}], \"summary\": string}. Each contents value is the full file. Do not include markdown.\n\n{input}".into(),
        },
        PromptVersion {
            name: "issue-implementation-edit".into(),
            version: 2,
            template: "Reply with JSON only for this one file: {\"path\": string, \"contents\": string, \"summary\": string}. contents is the full replacement. Never return {} or empty edits. Do not include markdown.\n\n{input}".into(),
        },
        PromptVersion {
            name: "complete".into(),
            version: 1,
            template: "{input}".into(),
        },
    ]
}

pub mod manager {
    use super::{render_template, PromptRef, PromptRegistry, PromptVersion};
    use crate::error::Result;

    /// Resolves and renders prompts for a request.
    #[derive(Debug, Clone)]
    pub struct PromptManager {
        registry: PromptRegistry,
    }

    impl PromptManager {
        pub fn new(registry: PromptRegistry) -> Self {
            Self { registry }
        }

        pub fn registry(&self) -> &PromptRegistry {
            &self.registry
        }

        pub fn resolve(
            &self,
            reference: &PromptRef,
            variables: &[(String, String)],
        ) -> Result<(PromptVersion, String)> {
            let version = self.registry.get(&reference.name, reference.version)?;
            let rendered = render_template(&version.template, variables);
            Ok((version.clone(), rendered))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn architecture_review_has_three_versions() {
        let registry = PromptRegistry::builtin();
        assert_eq!(
            registry.get("architecture-review", None).unwrap().version,
            3
        );
        assert_eq!(
            registry
                .get("architecture-review", Some(1))
                .unwrap()
                .version,
            1
        );
    }

    #[test]
    fn renders_variables() {
        let out = render_template("Hello {name}", &[("name".into(), "Binder".into())]);
        assert_eq!(out, "Hello Binder");
    }
}
