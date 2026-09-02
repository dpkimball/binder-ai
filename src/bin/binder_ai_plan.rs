//! One-shot engineering plan for Cursor. Bindb only. No HTTP, no model, no writes.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use binder_ai::{plan, CodeintelKnowledge, PlanRequest, ProcessCodeintelCli};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "-V" || a == "--version" || a == "version") {
        println!("binder-ai-plan {}", env!("CARGO_PKG_VERSION"));
        println!("commit: {}", env!("BINDER_AI_GIT_SHA"));
        println!("built: {}", env!("BINDER_AI_BUILD_DATE"));
        return Ok(());
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "binder-ai-plan --request TEXT [--workspace DIR] [--store DIR] [--codeintel BIN]"
        );
        eprintln!("  -V, --version    Print version and build provenance");
        return Ok(());
    }
    let request = required_flag(&args, "--request")?;
    let workspace = flag(&args, "--workspace").or_else(|| env::var("BINDER_AI_WORKSPACE").ok());
    let store = flag(&args, "--store")
        .or_else(|| env::var("BINDER_AI_CODEINTEL_STORE").ok())
        .ok_or("missing --store (or BINDER_AI_CODEINTEL_STORE)")?;
    let codeintel = flag(&args, "--codeintel")
        .unwrap_or_else(|| env::var("CODEINTEL_BIN").unwrap_or_else(|_| "codeintel".into()));

    binder_ai::install_bank_prefixes_from_env()?;

    let knowledge = CodeintelKnowledge::new(
        PathBuf::from(store),
        Vec::new(),
        ProcessCodeintelCli {
            bin: PathBuf::from(codeintel),
        },
    );
    let response = plan(
        &knowledge,
        &PlanRequest {
            workspace: workspace.map(PathBuf::from),
            request,
            since_sha: None,
        },
    )
    .map_err(|err| err.to_string())?;
    let json =
        serde_json::to_string_pretty(&response.for_agent()).map_err(|err| err.to_string())?;
    println!("{json}");
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
