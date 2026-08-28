//! Skill apply adapter: consume discovery JSON, run HygieneWorkflow.
//!
//! This is not a REST API and not hosted dashboard apply.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use binder_ai::{
    dispatch, CodeintelKnowledge, DispatchDecision, HygieneConfig, HygieneWorkflow, LocalRuntime,
    MockAiClient, ProcessCodeintelCli, SkillApplyDocument, SkillApplyReport,
};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "hygiene_apply --repo DIR --store DIR --name NAME --input FILE [--codeintel BIN]"
        );
        return Ok(());
    }
    let repo = required_flag(&args, "--repo")?;
    let store = required_flag(&args, "--store")?;
    let name = required_flag(&args, "--name")?;
    let input = required_flag(&args, "--input")?;
    let codeintel = flag(&args, "--codeintel")
        .unwrap_or_else(|| env::var("CODEINTEL_BIN").unwrap_or_else(|_| "codeintel".into()));

    let raw = std::fs::read_to_string(&input).map_err(|err| format!("read {input}: {err}"))?;
    let request = SkillApplyDocument::parse(&raw)?.into_request()?;
    let config = HygieneConfig::default();
    let needs_ai = request
        .findings
        .iter()
        .any(|finding| matches!(dispatch(finding, &config), DispatchDecision::Ai(_)));
    if needs_ai {
        return Err(
            "AI findings are not applied by this example; keep the skill JSON deterministic".into(),
        );
    }

    let runtime = LocalRuntime::new(Path::new(&repo)).map_err(|err| err.to_string())?;
    let knowledge = CodeintelKnowledge::new(
        PathBuf::from(&store),
        vec![(name, PathBuf::from(&repo))],
        ProcessCodeintelCli {
            bin: PathBuf::from(codeintel),
        },
    );
    let mock = MockAiClient::json_turns_local(["{}", "{}"]);
    let workflow = HygieneWorkflow::new(runtime, knowledge, config, mock.client.clone());
    let report = workflow
        .execute(request)
        .await
        .map_err(|err| err.to_string())?;
    if mock.chat_calls() != 0 {
        return Err(format!(
            "deterministic skill apply invoked binder-llm {} time(s)",
            mock.chat_calls()
        ));
    }
    let json = serde_json::to_string_pretty(&SkillApplyReport::from_workflow(&report))
        .map_err(|err| err.to_string())?;
    println!("{json}");
    if !report.success {
        return Err("hygiene apply failed".into());
    }
    Ok(())
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn required_flag(args: &[String], name: &str) -> Result<String, String> {
    flag(args, name).ok_or_else(|| format!("missing {name}"))
}
