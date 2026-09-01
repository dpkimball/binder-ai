//! Engineering intelligence HTTP adapter. Bindb planning + LLM explain/ask.

use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use binder_ai::{
    assess_ask_context, build_ask_prompt, build_explain_prompt, filter_guidance_for_repo,
    filter_linked_docs_for_repo, filter_paths_for_repo, lang_hint, plan, safe_truncate, AiClient,
    AiClientBuilder, AiRequest, CodeintelKnowledge, IntelError, PlanRequest, PlanResponse,
    ProcessCodeintelCli, TaskKind,
};
use serde::{Deserialize, Serialize};

struct AppState {
    store: PathBuf,
    codeintel_bin: PathBuf,
    knowledge: CodeintelKnowledge<ProcessCodeintelCli>,
    ai_client: AiClient,
}

#[derive(Debug, Deserialize)]
struct ExplainRequest {
    #[serde(default)]
    workspace: Option<PathBuf>,
    request: String,
    #[serde(default)]
    max_files: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AskRequest {
    repo: String,
    question: String,
    #[serde(default)]
    workspace: Option<PathBuf>,
    #[serde(default)]
    max_files: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct StoreQuestionRequest {
    repo: String,
    id: String,
    question: String,
    answer: String,
}

#[derive(Debug, Serialize)]
struct ExplainResponse {
    explanation: String,
    files_read: Vec<String>,
    inspection_set_size: usize,
}

#[derive(Debug, Serialize)]
struct AskResponse {
    answer: String,
    repo: String,
    files_read: Vec<String>,
    guidance: Vec<GuidanceOut>,
    linked_docs: Vec<DocRefOut>,
    linked_issues: Vec<IssueRefOut>,
    inspection_set_size: usize,
    planning_result: String,
}

#[derive(Debug, Serialize)]
struct GuidanceOut {
    id: String,
    question: String,
    answer: String,
}

#[derive(Debug, Serialize)]
struct DocRefOut {
    repo: String,
    path: String,
    kind: String,
    title: String,
}

#[derive(Debug, Serialize)]
struct IssueRefOut {
    number: u64,
    title: String,
}

#[derive(Debug, Serialize)]
struct StoreQuestionResponse {
    stored: bool,
    repo: String,
    id: String,
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
        eprintln!("  POST /plan  POST /explain  POST /ask  POST /questions");
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

    let store_path = PathBuf::from(&store);
    let codeintel_bin = PathBuf::from(&codeintel);
    let knowledge = CodeintelKnowledge::new(
        store_path.clone(),
        Vec::new(),
        ProcessCodeintelCli {
            bin: codeintel_bin.clone(),
        },
    );
    let ai_client = AiClientBuilder::new().build();
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/plan", post(plan_handler))
        .route("/explain", post(explain_handler))
        .route("/ask", post(ask_handler))
        .route("/questions", post(questions_handler))
        .with_state(Arc::new(AppState {
            store: store_path,
            codeintel_bin,
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
    apply_default_workspace(&mut request.workspace);
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
    apply_default_workspace(&mut request.workspace);
    let text = request.request.trim().to_string();
    if text.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty request".into()));
    }

    let plan_request = PlanRequest {
        workspace: request.workspace.clone(),
        request: text.clone(),
        since_sha: None,
    };
    let plan_response = plan_or_bad_gateway(&state.knowledge, &plan_request)?;

    let workspace = request
        .workspace
        .as_deref()
        .unwrap_or_else(|| Path::new("."));
    let max_files = request.max_files.unwrap_or(8).min(20);
    let (file_sections, files_read) = read_plan_files(
        workspace,
        &plan_response.resolved_execution_paths,
        max_files,
        None,
    )
    .await;

    if files_read.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "no files readable from workspace; provide workspace path".into(),
        ));
    }

    let prompt = build_explain_prompt(&text, &plan_response, &file_sections);
    let explanation = run_llm(&state.ai_client, prompt).await?;

    Ok(Json(ExplainResponse {
        explanation,
        files_read,
        inspection_set_size: plan_response.inspection_set.len(),
    }))
}

async fn ask_handler(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<AskRequest>,
) -> Result<Json<AskResponse>, (StatusCode, String)> {
    apply_default_workspace(&mut request.workspace);
    let repo = request.repo.trim().to_string();
    let question = request.question.trim().to_string();
    if repo.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty repo".into()));
    }
    if question.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty question".into()));
    }

    let plan_request = PlanRequest {
        workspace: request.workspace.clone(),
        request: format!("{repo}: {question}"),
        since_sha: None,
    };
    let plan_response = plan_or_bad_gateway(&state.knowledge, &plan_request)?;

    let repo_guidance = filter_guidance_for_repo(&plan_response.guidance, &repo);
    let repo_docs = filter_linked_docs_for_repo(&plan_response.linked_docs, &repo);

    let workspace = request
        .workspace
        .as_deref()
        .unwrap_or_else(|| Path::new("."));
    let max_files = request.max_files.unwrap_or(8).min(20);
    let scoped_paths = filter_paths_for_repo(&plan_response.resolved_execution_paths, &repo);
    let (mut file_sections, files_read) =
        read_plan_files(workspace, &scoped_paths, max_files, None).await;

    let doc_sections = read_linked_doc_sections(workspace, &repo_docs, 3).await;
    let repo_doc_sections = doc_sections.len();
    file_sections.extend(doc_sections);

    if let Err(reason) = assess_ask_context(&repo, &plan_response, &files_read, repo_doc_sections, &repo_guidance) {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, reason.message().into()));
    }

    let prompt = build_ask_prompt(
        &repo,
        &question,
        &plan_response,
        &file_sections,
        &repo_guidance,
    );
    let answer = run_llm(&state.ai_client, prompt).await?;

    Ok(Json(AskResponse {
        answer,
        repo: repo.clone(),
        files_read,
        guidance: repo_guidance
            .iter()
            .map(|g| GuidanceOut {
                id: g.id.clone(),
                question: g.question.clone(),
                answer: g.answer.clone(),
            })
            .collect(),
        linked_docs: repo_docs
            .iter()
            .take(5)
            .map(|d| DocRefOut {
                repo: d.repo.clone(),
                path: d.path.clone(),
                kind: d.kind.clone(),
                title: d.title.clone(),
            })
            .collect(),
        linked_issues: plan_response
            .linked_issues
            .iter()
            .take(5)
            .map(|i| IssueRefOut {
                number: i.number,
                title: i.title.clone(),
            })
            .collect(),
        inspection_set_size: plan_response.inspection_set.len(),
        planning_result: format!("{:?}", plan_response.planning_result)
            .to_lowercase()
            .replace(' ', "_"),
    }))
}

async fn questions_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<StoreQuestionRequest>,
) -> Result<Json<StoreQuestionResponse>, (StatusCode, String)> {
    let repo = body.repo.trim().to_string();
    let id = body.id.trim().to_string();
    let question = body.question.trim().to_string();
    let answer = body.answer.trim().to_string();
    if repo.is_empty() || id.is_empty() || question.is_empty() || answer.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "repo, id, question, and answer are required".into(),
        ));
    }
    let repo_arg = if repo == "cross" {
        "cross".to_string()
    } else {
        repo.clone()
    };
    let output = Command::new(&state.codeintel_bin)
        .args([
            "put-question",
            "--store",
            &state.store.display().to_string(),
            "--repo",
            &repo_arg,
            "--id",
            &id,
            "--question",
            &question,
            "--answer",
            &answer,
        ])
        .output()
        .map_err(|err| (StatusCode::BAD_GATEWAY, format!("codeintel spawn failed: {err}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("codeintel put-question failed: {stderr}"),
        ));
    }
    Ok(Json(StoreQuestionResponse {
        stored: true,
        repo,
        id,
    }))
}

fn apply_default_workspace(workspace: &mut Option<PathBuf>) {
    if workspace.is_none() {
        if let Ok(root) = env::var("BINDER_AI_WORKSPACE") {
            if !root.is_empty() {
                *workspace = Some(PathBuf::from(root));
            }
        }
    }
}

fn plan_or_bad_gateway(
    knowledge: &CodeintelKnowledge<ProcessCodeintelCli>,
    request: &PlanRequest,
) -> Result<PlanResponse, (StatusCode, String)> {
    match plan(knowledge, request) {
        Ok(r) => Ok(r),
        Err(IntelError::EmptyRequest) => Err((StatusCode::BAD_REQUEST, "empty request".into())),
        Err(err) => Err((StatusCode::BAD_GATEWAY, err.to_string())),
    }
}

async fn read_plan_files(
    workspace: &Path,
    paths: &[String],
    max_files: usize,
    repo_filter: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    let paths: Vec<String> = if let Some(repo) = repo_filter {
        filter_paths_for_repo(paths, repo)
    } else {
        paths.to_vec()
    };

    let canonical_workspace = tokio::fs::canonicalize(workspace).await.ok();
    let mut file_sections = Vec::new();
    let mut files_read = Vec::new();
    for rel_path in paths.iter().take(max_files) {
        let full_path = workspace.join(rel_path);
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
    (file_sections, files_read)
}

async fn read_linked_doc_sections(
    workspace: &Path,
    docs: &[binder_ai::LinkedDocSummary],
    max_docs: usize,
) -> Vec<String> {
    let canonical_workspace = tokio::fs::canonicalize(workspace).await.ok();
    let mut sections = Vec::new();
    for doc in docs.iter().take(max_docs) {
        let rel_path = format!("{}/{}", doc.repo, doc.path);
        let full_path = workspace.join(&rel_path);
        if let Some(ref cws) = canonical_workspace {
            match tokio::fs::canonicalize(&full_path).await {
                Ok(canonical_full) if canonical_full.starts_with(cws) => {}
                _ => continue,
            }
        }
        if let Ok(contents) = tokio::fs::read_to_string(&full_path).await {
            let excerpt = safe_truncate(&contents, 3000);
            sections.push(format!(
                "### {rel_path} ({}/{})\n```markdown\n{excerpt}\n```",
                doc.repo, doc.kind
            ));
        }
    }
    sections
}

async fn run_llm(ai_client: &AiClient, prompt: String) -> Result<String, (StatusCode, String)> {
    let ai_request = AiRequest::new(TaskKind::Reasoning, prompt);
    let ai_response = ai_client
        .execute(ai_request)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("LLM error: {e}")))?;
    Ok(ai_response.content)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}
