//! Unattended issue implementation. Bindb-first. Dry-run by default. No HTTP.

use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use binder_ai::{
    execute_implement, plan_for_implement, report_from_plan, report_from_workflow_error,
    AiClientBuilder, CodeintelKnowledge, Issue, LocalRuntime, ProcessCodeintelCli,
};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<ExitCode, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args
        .iter()
        .any(|a| a == "-V" || a == "--version" || a == "version")
    {
        println!("binder-ai-implement {}", env!("CARGO_PKG_VERSION"));
        println!("commit: {}", env!("BINDER_AI_GIT_SHA"));
        println!("built: {}", env!("BINDER_AI_BUILD_DATE"));
        return Ok(ExitCode::SUCCESS);
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "binder-ai-implement --repo DIR --store DIR --name LABEL \\\n  [--request TEXT | --issue URL|N | --title T --body B] \\\n  [--workspace DIR] [--codeintel BIN] [--github-repo owner/name] \\\n  [--dry-run | --execute] [--publish]"
        );
        eprintln!("  Default is --dry-run (plan only, no LLM, no writes).");
        eprintln!("  --execute writes via Engineering inspection set. Foreman omits --publish.");
        eprintln!("  --publish is not supported in this slice (Foreman opens the PR).");
        return Ok(ExitCode::SUCCESS);
    }

    if flag_bool(&args, "--publish") {
        return Err(
            "--publish is not supported in this slice; Foreman creates the branch and PR".into(),
        );
    }
    let execute = flag_bool(&args, "--execute");
    let dry_run_flag = flag_bool(&args, "--dry-run");
    if execute && dry_run_flag {
        return Err("pass only one of --dry-run or --execute".into());
    }
    let dry_run = !execute;

    let repo = required_flag(&args, "--repo")?;
    let store = flag(&args, "--store")
        .or_else(|| env::var("BINDER_AI_CODEINTEL_STORE").ok())
        .ok_or("missing --store (or BINDER_AI_CODEINTEL_STORE)")?;
    let name = required_flag(&args, "--name")?;
    let workspace = flag(&args, "--workspace").unwrap_or_else(|| repo.clone());
    let codeintel = flag(&args, "--codeintel")
        .unwrap_or_else(|| env::var("CODEINTEL_BIN").unwrap_or_else(|_| "codeintel".into()));
    let github_repo = flag(&args, "--github-repo");

    let issue = load_issue(&args, github_repo.as_deref())?;
    let request_text = issue.render();

    binder_ai::install_bank_prefixes_from_env()?;

    let workspace_path = PathBuf::from(&workspace);
    let binds = vec![(name.clone(), workspace_path.clone())];
    let knowledge = CodeintelKnowledge::new(
        PathBuf::from(store),
        binds.clone(),
        ProcessCodeintelCli {
            bin: PathBuf::from(codeintel),
        },
    );

    let plan = plan_for_implement(&knowledge, &request_text, workspace_path.clone(), binds)
        .map_err(|err| err.to_string())?;
    let declined = binder_ai::autonomous_decline_reason(&plan).is_some();

    let report = if dry_run || declined {
        report_from_plan(plan, &name, dry_run)
    } else {
        let runtime =
            LocalRuntime::new(&workspace_path).map_err(|err| format!("runtime: {err}"))?;
        let ai = AiClientBuilder::new().build();
        match execute_implement(runtime, ai, issue, workspace_path, &name, &plan).await {
            Ok(report) => report,
            Err(err) => report_from_workflow_error(&plan, &err),
        }
    };

    let json = serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?;
    println!("{json}");
    if report.success && !report.declined {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
}

fn load_issue(args: &[String], github_repo: Option<&str>) -> Result<Issue, String> {
    if let Some(request) = flag(args, "--request") {
        return Ok(split_request(&request));
    }
    let title = flag(args, "--title");
    let body = flag(args, "--body").unwrap_or_default();
    if let Some(title) = title {
        return Ok(Issue::new(title, body));
    }
    let issue_ref = flag(args, "--issue")
        .ok_or("missing --request, --title, or --issue (GitHub URL or number)")?;
    fetch_github_issue(&issue_ref, github_repo)
}

fn split_request(request: &str) -> Issue {
    let trimmed = request.trim();
    match trimmed.split_once("\n\n") {
        Some((title, description)) => Issue::new(title.trim(), description.trim()),
        None => Issue::new(trimmed, ""),
    }
}

fn fetch_github_issue(issue_ref: &str, github_repo: Option<&str>) -> Result<Issue, String> {
    let (repo, number) = parse_issue_ref(issue_ref, github_repo)?;
    let mut cmd = Command::new("gh");
    cmd.args(["issue", "view", &number.to_string(), "--json", "title,body"]);
    if let Some(repo) = repo.as_deref() {
        cmd.args(["-R", repo]);
    }
    let output = cmd
        .output()
        .map_err(|err| format!("gh spawn failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "gh issue view failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|err| format!("gh json: {err}"))?;
    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description = value
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Issue::new(title, description))
}

fn parse_issue_ref(
    issue_ref: &str,
    github_repo: Option<&str>,
) -> Result<(Option<String>, u64), String> {
    let trimmed = issue_ref.trim();
    if let Some(rest) = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    {
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 4 && parts[2] == "issues" {
            let number = parts[3]
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or("invalid GitHub issue URL")?;
            return Ok((Some(format!("{}/{}", parts[0], parts[1])), number));
        }
    }
    if let Some((repo, num)) = trimmed.split_once('#') {
        let number = num
            .parse()
            .map_err(|_| "invalid owner/repo#number".to_string())?;
        return Ok((Some(repo.to_string()), number));
    }
    let number = trimmed
        .parse::<u64>()
        .map_err(|_| "issue must be a URL, owner/repo#N, or number".to_string())?;
    Ok((github_repo.map(str::to_string), number))
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn required_flag(args: &[String], name: &str) -> Result<String, String> {
    flag(args, name).ok_or_else(|| format!("missing {name}"))
}

fn flag_bool(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}
