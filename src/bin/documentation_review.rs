//! Dashboard apply adapter: documentation review → DocumentationReviewWorkflow.
//!
//! Not a REST API. Bindb plans; Runtime reads. No LLM.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use binder_ai::{
    CodeintelKnowledge, DocumentationReviewRequest, DocumentationReviewWorkflow, LocalRuntime,
    ProcessCodeintelCli, WorkspaceLayout,
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
            "documentation_review --repo DIR --store DIR --name NAME [--finding KIND] [--workspace DIR] [--bind name=PATH] [--codeintel BIN]"
        );
        return Ok(());
    }
    let repo = required_flag(&args, "--repo")?;
    let store = required_flag(&args, "--store")?;
    let name = required_flag(&args, "--name")?;
    let finding = flag(&args, "--finding");
    let codeintel = flag(&args, "--codeintel")
        .unwrap_or_else(|| env::var("CODEINTEL_BIN").unwrap_or_else(|_| "codeintel".into()));
    let workspace = flag(&args, "--workspace").unwrap_or_else(|| default_workspace(&repo));
    let layout = execution_layout(&workspace, &name, &repo, &args)?;

    let runtime = LocalRuntime::new(Path::new(&repo)).map_err(|err| err.to_string())?;
    let knowledge = CodeintelKnowledge::new(
        PathBuf::from(&store),
        vec![(name.clone(), PathBuf::from(&repo))],
        ProcessCodeintelCli {
            bin: PathBuf::from(codeintel),
        },
    );
    let request = DocumentationReviewRequest {
        request: match finding.as_deref() {
            Some(kind) => format!("documentation review {kind} in {name}"),
            None => format!("documentation review README docs as-built in {name}"),
        },
        repository: name,
        repository_root: PathBuf::from(&repo),
        finding_kind: finding,
    };
    let report = DocumentationReviewWorkflow::new(runtime, knowledge)
        .with_workspace(layout)
        .execute(request)
        .await
        .map_err(|err| err.to_string())?;
    let json = serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?;
    println!("{json}");
    if !report.success {
        return Err("documentation review failed".into());
    }
    Ok(())
}

fn execution_layout(
    workspace: &str,
    name: &str,
    repo: &str,
    args: &[String],
) -> Result<WorkspaceLayout, String> {
    let mut layout = WorkspaceLayout::new(workspace).bind(name, PathBuf::from(repo));
    for spec in flags(args, "--bind") {
        let (bind_name, path) = parse_bind(&spec)?;
        layout = layout.bind(bind_name, path);
    }
    Ok(layout)
}

fn default_workspace(repo: &str) -> String {
    Path::new(repo)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new(repo))
        .display()
        .to_string()
}

fn parse_bind(spec: &str) -> Result<(String, PathBuf), String> {
    let (name, path) = spec
        .split_once('=')
        .ok_or_else(|| format!("--bind expects name=PATH, got `{spec}`"))?;
    if name.is_empty() || path.is_empty() {
        return Err(format!("--bind expects name=PATH, got `{spec}`"));
    }
    Ok((name.to_string(), PathBuf::from(path)))
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn flags(args: &[String], name: &str) -> Vec<String> {
    args.windows(2)
        .filter(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .collect()
}

fn required_flag(args: &[String], name: &str) -> Result<String, String> {
    flag(args, name).ok_or_else(|| format!("missing {name}"))
}
