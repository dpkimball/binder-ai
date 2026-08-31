//! Engineering intelligence HTTP adapter. Bindb planning + LLM explain.

use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use binder_ai::{
    plan, AiClient, AiClientBuilder, AiRequest, CodeintelKnowledge, IntelError, PlanRequest,
    ProcessCodeintelCli, TaskKind,
};
use serde::{Deserialize, Serialize};

struct AppState {
    knowledge: CodeintelKnowledge<ProcessCodeintelCli>,
    ai_client: AiClient,
}

#[derive(Debug, Deserialize)]
struct ExplainRequest {
    #[serde(default)]
    workspace: Option<PathBuf>,
    request: String,
    /// Number of files to read (default 8, max 20).
    #[serde(default)]
    max_files: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ExplainResponse {
    explanation: String,
    files_read: Vec<String>,
    inspection_set_size: usize,
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("binder-ai-serve --store DIR [--listen HOST:PORT] [--codeintel BIN]");
        eprintln!("  default listen: 0.0.0.0:7841 (or BINDER_AI_LISTEN)");
        eprintln!("  BINDER_AI_WORKSPACE: default PlanRequest.workspace when JSON omits it");
        return Ok(());
    }
    let store = flag(&args, "--store")
        .or_else(|| env::var("BINDER_AI_CODEINTEL_STORE").ok())
        .ok_or("missing --store (or BINDER_AI_CODEINTEL_STORE)")?;
    let listen = flag(&args, "--listen")
        .or_else(|| env::var("BINDER_AI_LISTEN").ok())
        .unwrap_or_else(|| "0.0.0.0:7841".into());
    let codeintel = flag(&args, "--codeintel")
        .unwrap_or_else(|| env::var("CODEINTEL_BIN").unwrap_or_else(|_| "codeintel".into()));
    let addr: SocketAddr = listen
        .parse()
        .map_err(|err| format!("listen {listen}: {err}"))?;

    // Binds are for `reindex()`, not `plan()`. QA indexes via the chart init
    // container into BINDER_AI_CODEINTEL_STORE. Default workspace (when the
    // JSON omits `workspace`) is BINDER_AI_WORKSPACE so `repo:path` resolves
    // against server checkouts and `resolved_execution_paths` are workspace-
    // relative (`binder-ai/Dockerfile`), which Cursor can open locally.
    let knowledge = CodeintelKnowledge::new(
        PathBuf::from(store),
        Vec::new(),
        ProcessCodeintelCli {
            bin: PathBuf::from(codeintel),
        },
    );
    let ai_client = AiClientBuilder::new().build();
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/plan", post(plan_handler))
        .route("/explain", post(explain_handler))
        .with_state(Arc::new(AppState {
            knowledge,
            ai_client,
        }));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| err.to_string())?;
    eprintln!("binder-ai-serve listening on {addr}");
    axum::serve(listener, app)
        .await
        .map_err(|err| err.to_string())
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "service": "binder-ai-serve" }))
}

async fn plan_handler(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<PlanRequest>,
) -> Result<Json<binder_ai::AgentPlan>, (StatusCode, String)> {
    if request.workspace.is_none() {
        if let Ok(root) = env::var("BINDER_AI_WORKSPACE") {
            if !root.is_empty() {
                request.workspace = Some(PathBuf::from(root));
            }
        }
    }
    match plan(&state.knowledge, &request) {
        Ok(response) => Ok(Json(response.for_agent())),
        Err(IntelError::EmptyRequest) => Err((StatusCode::BAD_REQUEST, "empty request".into())),
        Err(err) => Err((StatusCode::BAD_GATEWAY, err.to_string())),
    }
}

async fn explain_handler(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<ExplainRequest>,
) -> Result<Json<ExplainResponse>, (StatusCode, String)> {
    if request.workspace.is_none() {
        if let Ok(root) = env::var("BINDER_AI_WORKSPACE") {
            if !root.is_empty() {
                request.workspace = Some(PathBuf::from(root));
            }
        }
    }
    let text = request.request.trim().to_string();
    if text.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty request".into()));
    }

    let plan_request = PlanRequest {
        workspace: request.workspace.clone(),
        request: text.clone(),
        since_sha: None,
    };
    let plan_response = match plan(&state.knowledge, &plan_request) {
        Ok(r) => r,
        Err(IntelError::EmptyRequest) => {
            return Err((StatusCode::BAD_REQUEST, "empty request".into()))
        }
        Err(err) => return Err((StatusCode::BAD_GATEWAY, err.to_string())),
    };

    let workspace = request
        .workspace
        .as_deref()
        .unwrap_or_else(|| Path::new("."));
    let max_files = request.max_files.unwrap_or(8).min(20);

    // Canonicalize the workspace root once so we can containment-check each path.
    // If canonicalization fails (workspace doesn't exist), we skip file reads.
    let canonical_workspace = tokio::fs::canonicalize(workspace).await.ok();

    let mut file_sections: Vec<String> = Vec::new();
    let mut files_read: Vec<String> = Vec::new();
    for rel_path in plan_response
        .resolved_execution_paths
        .iter()
        .take(max_files)
    {
        let full_path = workspace.join(rel_path);
        // Reject path traversal: require the resolved path to stay within workspace.
        if let Some(ref cws) = canonical_workspace {
            match tokio::fs::canonicalize(&full_path).await {
                Ok(canonical_full) if canonical_full.starts_with(cws) => {}
                _ => continue,
            }
        }
        if let Ok(contents) = tokio::fs::read_to_string(&full_path).await {
            let lang = lang_hint(rel_path);
            let excerpt = safe_truncate(&contents, 4000);
            file_sections.push(format!("### {rel_path}\n```{lang}\n{excerpt}\n```"));
            files_read.push(rel_path.clone());
        }
    }

    if files_read.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "no files readable from workspace; provide workspace path".into(),
        ));
    }

    let mut prompt = format!(
        "You are an expert software engineer. Answer this question about the codebase:\n\n\
         > {text}\n\n\
         Cite specific function names and file paths. Be concise.\n\n\
         ## Source Files\n\n{}",
        file_sections.join("\n\n")
    );

    // Need 9B: append linked issue bodies and doc refs as design rationale context.
    // All data comes from the first plan() call — no second CLI spawn needed.
    let issues: Vec<String> = plan_response
        .linked_issues
        .iter()
        .filter(|i| !i.body_excerpt.is_empty())
        .take(3)
        .map(|i| format!("### Issue #{}: {}\n{}", i.number, i.title, i.body_excerpt))
        .collect();
    if !issues.is_empty() {
        prompt.push_str("\n\n## Related Issues\n\n");
        prompt.push_str(&issues.join("\n\n"));
    }
    let docs: Vec<String> = plan_response
        .linked_docs
        .iter()
        .take(3)
        .map(|d| format!("- `{}` ({}/{})", d.path, d.repo, d.kind))
        .collect();
    if !docs.is_empty() {
        prompt.push_str("\n\n## Related Documentation\n\n");
        prompt.push_str(&docs.join("\n"));
    }

    prompt.push_str("\n\nAnswer:");

    let ai_request = AiRequest::new(TaskKind::Reasoning, prompt);
    let ai_response = state
        .ai_client
        .execute(ai_request)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("LLM error: {e}")))?;

    Ok(Json(ExplainResponse {
        explanation: ai_response.content,
        files_read,
        inspection_set_size: plan_response.inspection_set.len(),
    }))
}

fn lang_hint(path: &str) -> &'static str {
    if path.ends_with(".rs") {
        "rust"
    } else if path.ends_with(".ts") || path.ends_with(".tsx") {
        "typescript"
    } else if path.ends_with(".py") {
        "python"
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        "yaml"
    } else if path.ends_with(".toml") {
        "toml"
    } else if path.ends_with(".json") {
        "json"
    } else {
        ""
    }
}

fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}
