use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use super::{
    DoctrineSnapshot, ExecutionRecord, Knowledge, KnowledgeError, KnowledgePlan, ReindexStats,
};

/// Invoke the `codeintel` CLI (JSON on stdout).
pub trait CodeintelCli: Send + Sync {
    fn run(&self, args: &[String]) -> std::result::Result<String, KnowledgeError>;
}

/// Production CLI: `codeintel` on PATH (or an explicit binary).
#[derive(Debug, Clone)]
pub struct ProcessCodeintelCli {
    pub bin: PathBuf,
}

impl Default for ProcessCodeintelCli {
    fn default() -> Self {
        Self {
            bin: PathBuf::from("codeintel"),
        }
    }
}

impl CodeintelCli for ProcessCodeintelCli {
    fn run(&self, args: &[String]) -> std::result::Result<String, KnowledgeError> {
        let output = Command::new(&self.bin)
            .args(args)
            .output()
            .map_err(|err| KnowledgeError::new(format!("codeintel spawn failed: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KnowledgeError::new(format!(
                "codeintel exited {}: {stderr}",
                output.status
            )));
        }
        String::from_utf8(output.stdout)
            .map_err(|err| KnowledgeError::new(format!("codeintel stdout utf8: {err}")))
    }
}

/// bindb facts via the codeintel Architecture / index CLI.
///
/// binder-ai does not depend on the unpublished `codeintel` crate. This adapter
/// is the production `Knowledge` impl for Hygiene apply: plan/doctrine/reindex
/// go through JSON; execution history is kept here and mirrored as JSONL next
/// to the store.
pub struct CodeintelKnowledge<C> {
    store: PathBuf,
    binds: Vec<(String, PathBuf)>,
    cli: C,
    history: Mutex<Vec<ExecutionRecord>>,
}

impl CodeintelKnowledge<ProcessCodeintelCli> {
    pub fn open(store: impl Into<PathBuf>, binds: Vec<(String, PathBuf)>) -> Self {
        Self::new(store, binds, ProcessCodeintelCli::default())
    }
}

impl<C: CodeintelCli> CodeintelKnowledge<C> {
    pub fn new(store: impl Into<PathBuf>, binds: Vec<(String, PathBuf)>, cli: C) -> Self {
        Self {
            store: store.into(),
            binds,
            cli,
            history: Mutex::new(Vec::new()),
        }
    }

    pub fn execution_history_local(&self) -> Vec<ExecutionRecord> {
        self.history
            .lock()
            .expect("codeintel knowledge mutex")
            .clone()
    }
}

impl<C: CodeintelCli> Knowledge for CodeintelKnowledge<C> {
    fn plan(&self, request: &str) -> std::result::Result<KnowledgePlan, KnowledgeError> {
        let store = self.store.display().to_string();
        let stdout = self.cli.run(&[
            "architecture".into(),
            "--store".into(),
            store,
            "plan".into(),
            "--request".into(),
            request.into(),
        ])?;
        plan_from_architecture_json(&stdout)
    }

    fn doctrine(&self) -> std::result::Result<DoctrineSnapshot, KnowledgeError> {
        let store = self.store.display().to_string();
        let stdout = self.cli.run(&[
            "architecture".into(),
            "--store".into(),
            store,
            "doctrine".into(),
        ])?;
        doctrine_from_architecture_json(&stdout)
    }

    fn reindex(&self) -> std::result::Result<ReindexStats, KnowledgeError> {
        let store = self.store.display().to_string();
        let mut repositories = Vec::new();
        for (name, path) in &self.binds {
            self.cli.run(&[
                "index".into(),
                path.display().to_string(),
                "--store".into(),
                store.clone(),
                "--name".into(),
                name.clone(),
            ])?;
            repositories.push(name.clone());
        }
        Ok(ReindexStats {
            repositories,
            skipped: self.binds.is_empty(),
        })
    }

    fn record_execution(&self, record: ExecutionRecord) -> std::result::Result<(), KnowledgeError> {
        persist_execution_jsonl(&self.store, &record)?;
        self.history
            .lock()
            .expect("codeintel knowledge mutex")
            .push(record);
        Ok(())
    }

    fn execution_history(&self) -> std::result::Result<Vec<ExecutionRecord>, KnowledgeError> {
        Ok(self.execution_history_local())
    }
}

pub fn plan_from_architecture_json(
    stdout: &str,
) -> std::result::Result<KnowledgePlan, KnowledgeError> {
    let value: serde_json::Value = serde_json::from_str(stdout)
        .map_err(|err| KnowledgeError::new(format!("plan json: {err}")))?;
    let files = value
        .get("smallest_file_set")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default();
    let repositories = string_list(&value, "repositories");
    Ok(KnowledgePlan {
        request: value
            .get("request")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        interpretation: value
            .get("interpretation")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        repositories,
        inspection_set: files,
        rules_must_remain_satisfied: string_list(&value, "rules_must_remain_satisfied"),
        likely_affected: string_list(&value, "likely_affected"),
        primary_source: "bindb",
    })
}

pub fn doctrine_from_architecture_json(
    stdout: &str,
) -> std::result::Result<DoctrineSnapshot, KnowledgeError> {
    let value: serde_json::Value = serde_json::from_str(stdout)
        .map_err(|err| KnowledgeError::new(format!("doctrine json: {err}")))?;
    let repos = value
        .get("repositories")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let first = repos.first();
    let report = first.and_then(|r| r.get("report"));
    let findings = report
        .and_then(|r| r.get("findings"))
        .and_then(|v| v.as_array())
        .map(Vec::len)
        .unwrap_or(0);
    Ok(DoctrineSnapshot {
        repository: first
            .and_then(|r| r.get("repository"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        doctrine: report
            .and_then(|r| r.get("doctrine"))
            .and_then(|v| v.as_str())
            .unwrap_or("workspace")
            .to_string(),
        findings,
        rules_evaluated: report
            .and_then(|r| r.get("rules_evaluated"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
        primary_source: "bindb",
    })
}

fn string_list(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn persist_execution_jsonl(
    store: &std::path::Path,
    record: &ExecutionRecord,
) -> std::result::Result<(), KnowledgeError> {
    let path = store.with_extension("hygiene-history.jsonl");
    let line = serde_json::to_string(record)
        .map_err(|err| KnowledgeError::new(format!("history json: {err}")))?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| KnowledgeError::new(format!("history open: {err}")))?;
    writeln!(file, "{line}").map_err(|err| KnowledgeError::new(format!("history write: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_json_maps_smallest_file_set() {
        let raw = r#"{
            "request": "fix add",
            "interpretation": "file match",
            "repositories": ["demo"],
            "smallest_file_set": ["src/lib.rs"],
            "rules_must_remain_satisfied": ["workspace.acyclic-crates"],
            "likely_affected": ["demo/demo"],
            "primary_source": "bindb"
        }"#;
        let plan = plan_from_architecture_json(raw).unwrap();
        assert_eq!(plan.inspection_set, vec![PathBuf::from("src/lib.rs")]);
        assert_eq!(plan.primary_source, "bindb");
        assert_eq!(plan.repositories, vec!["demo"]);
    }

    #[test]
    fn doctrine_json_counts_findings() {
        let raw = r#"{
            "repositories": [{
                "repository": "demo",
                "report": {
                    "doctrine": "workspace",
                    "findings": [],
                    "rules_evaluated": 1
                }
            }],
            "primary_source": "bindb"
        }"#;
        let snap = doctrine_from_architecture_json(raw).unwrap();
        assert_eq!(snap.findings, 0);
        assert_eq!(snap.rules_evaluated, 1);
        assert_eq!(snap.repository, "demo");
    }
}
