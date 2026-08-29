//! Engineering intelligence HTTP adapter. Bindb planning only. No LLM, no writes.

use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use binder_ai::{
    plan, CodeintelKnowledge, IntelError, PlanRequest, PlanResponse, ProcessCodeintelCli,
};

struct AppState {
    knowledge: CodeintelKnowledge<ProcessCodeintelCli>,
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
        return Ok(());
    }
    let store = flag(&args, "--store")
        .or_else(|| env::var("BINDER_AI_CODEINTEL_STORE").ok())
        .ok_or("missing --store (or BINDER_AI_CODEINTEL_STORE)")?;
    let listen = flag(&args, "--listen").unwrap_or_else(|| "127.0.0.1:7841".into());
    let codeintel = flag(&args, "--codeintel")
        .unwrap_or_else(|| env::var("CODEINTEL_BIN").unwrap_or_else(|_| "codeintel".into()));
    let addr: SocketAddr = listen
        .parse()
        .map_err(|err| format!("listen {listen}: {err}"))?;

    let knowledge = CodeintelKnowledge::new(
        PathBuf::from(store),
        Vec::new(),
        ProcessCodeintelCli {
            bin: PathBuf::from(codeintel),
        },
    );
    let app = Router::new()
        .route("/plan", post(plan_handler))
        .with_state(Arc::new(AppState { knowledge }));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| err.to_string())?;
    eprintln!("binder-ai-serve listening on {addr}");
    axum::serve(listener, app)
        .await
        .map_err(|err| err.to_string())
}

async fn plan_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<PlanRequest>,
) -> Result<Json<PlanResponse>, (StatusCode, String)> {
    match plan(&state.knowledge, &request) {
        Ok(response) => Ok(Json(response)),
        Err(IntelError::EmptyRequest) => Err((StatusCode::BAD_REQUEST, "empty request".into())),
        Err(err) => Err((StatusCode::BAD_GATEWAY, err.to_string())),
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}
