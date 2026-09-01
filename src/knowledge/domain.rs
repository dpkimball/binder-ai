//! Knowledge domain + scope validation for `/knowledge/{domain}/…` (spec 015 S2).

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeDomain {
    Engineering,
    BinderReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeScope {
    Engineering { repo: String },
    BinderReport { user_id: String, report_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("unknown knowledge domain: {0}")]
    UnknownDomain(String),
    #[error("domain {domain} is not live for /ask yet")]
    DomainNotLive { domain: &'static str },
    #[error("missing or invalid scope for domain {domain}")]
    InvalidScope { domain: &'static str },
    #[error("empty repo in engineering scope")]
    EmptyRepo,
    #[error("empty question")]
    EmptyQuestion,
}

impl KnowledgeDomain {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw.trim() {
            "engineering" => Ok(Self::Engineering),
            "binder_report" => Ok(Self::BinderReport),
            other => Err(DomainError::UnknownDomain(other.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Engineering => "engineering",
            Self::BinderReport => "binder_report",
        }
    }

    /// Phase 1: only engineering is wired; binder_report returns `DomainNotLive`.
    pub fn assert_live_for_ask(self) -> Result<(), DomainError> {
        match self {
            Self::Engineering => Ok(()),
            Self::BinderReport => Err(DomainError::DomainNotLive {
                domain: "binder_report",
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ScopedAskBody {
    pub scope: serde_json::Value,
    pub question: String,
    #[serde(default)]
    pub workspace: Option<std::path::PathBuf>,
    #[serde(default)]
    pub max_files: Option<usize>,
}

impl KnowledgeScope {
    pub fn from_domain_value(
        domain: KnowledgeDomain,
        value: &serde_json::Value,
    ) -> Result<Self, DomainError> {
        match domain {
            KnowledgeDomain::Engineering => {
                let repo = value
                    .get("repo")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .ok_or(DomainError::InvalidScope {
                        domain: "engineering",
                    })?;
                Ok(Self::Engineering {
                    repo: repo.to_string(),
                })
            }
            KnowledgeDomain::BinderReport => {
                let user_id = value
                    .get("user_id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let report_id = value
                    .get("report_id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                match (user_id, report_id) {
                    (Some(user_id), Some(report_id)) => Ok(Self::BinderReport {
                        user_id: user_id.to_string(),
                        report_id: report_id.to_string(),
                    }),
                    _ => Err(DomainError::InvalidScope {
                        domain: "binder_report",
                    }),
                }
            }
        }
    }

    pub fn engineering_repo(&self) -> Result<&str, DomainError> {
        match self {
            Self::Engineering { repo } if repo.is_empty() => Err(DomainError::EmptyRepo),
            Self::Engineering { repo } => Ok(repo.as_str()),
            Self::BinderReport { .. } => Err(DomainError::InvalidScope {
                domain: "engineering",
            }),
        }
    }
}

pub fn parse_scoped_ask(
    domain_raw: &str,
    body: &ScopedAskBody,
) -> Result<
    (
        KnowledgeDomain,
        String,
        Option<std::path::PathBuf>,
        Option<usize>,
    ),
    DomainError,
> {
    let domain = KnowledgeDomain::parse(domain_raw)?;
    domain.assert_live_for_ask()?;
    let scope = KnowledgeScope::from_domain_value(domain, &body.scope)?;
    let repo = scope.engineering_repo()?.to_string();
    let question = body.question.trim().to_string();
    if question.is_empty() {
        return Err(DomainError::EmptyQuestion);
    }
    Ok((domain, repo, body.workspace.clone(), body.max_files))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn engineering_scope_parses_repo() {
        let scope = KnowledgeScope::from_domain_value(
            KnowledgeDomain::Engineering,
            &json!({ "repo": "codeintel" }),
        )
        .unwrap();
        assert_eq!(
            scope,
            KnowledgeScope::Engineering {
                repo: "codeintel".into()
            }
        );
    }

    #[test]
    fn binder_report_not_live_for_ask() {
        let err = KnowledgeDomain::BinderReport
            .assert_live_for_ask()
            .unwrap_err();
        assert!(matches!(err, DomainError::DomainNotLive { .. }));
    }

    #[test]
    fn parse_scoped_ask_rejects_unknown_domain() {
        let body = ScopedAskBody {
            scope: json!({ "repo": "codeintel" }),
            question: "Q?".into(),
            workspace: None,
            max_files: None,
        };
        assert!(matches!(
            parse_scoped_ask("foreman", &body).unwrap_err(),
            DomainError::UnknownDomain(_)
        ));
    }
}
