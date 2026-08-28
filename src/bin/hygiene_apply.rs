//! Skill / dashboard apply adapter: discovery JSON → HygieneWorkflow.
//!
//! Not a REST API. Live LLM uses `OLLAMA_HOST` (QA Argo `ollama-service` or
//! dev `dev-proxies-ollama`) via `ProviderSettings::defaults_for`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use binder_ai::{
    dispatch, AiClient, CodeintelKnowledge, DispatchDecision, HygieneConfig, HygieneWorkflow,
    LocalRuntime, MockAiClient, ProcessCodeintelCli, RoutingPolicy, SkillApplyDocument,
    SkillApplyReport,
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
    let live_llm = env::var("BINDER_AI_LIVE_LLM").ok().as_deref() == Some("1");
    if needs_ai && !live_llm {
        return Err(
            "AI findings require BINDER_AI_LIVE_LLM=1 (LocalOnly catalog discovery)".into(),
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
    let report = if needs_ai {
        let ai = AiClient::builder()
            .routing_policy(RoutingPolicy::LocalOnly)
            .build();
        HygieneWorkflow::new(runtime, knowledge, config, ai)
            .execute(request)
            .await
            .map_err(|err| err.to_string())?
    } else {
        let mock = MockAiClient::json_turns_local(["{}", "{}"]);
        let report = HygieneWorkflow::new(runtime, knowledge, config, mock.client.clone())
            .execute(request)
            .await
            .map_err(|err| err.to_string())?;
        if mock.chat_calls() != 0 {
            return Err(format!(
                "deterministic skill apply invoked binder-llm {} time(s)",
                mock.chat_calls()
            ));
        }
        report
    };
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
