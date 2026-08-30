# binder-ai

[![CI](https://github.com/dpkimball/binder-ai/actions/workflows/ci.yml/badge.svg)](https://github.com/dpkimball/binder-ai/actions/workflows/ci.yml)
[![Image](https://github.com/dpkimball/binder-ai/actions/workflows/image.yml/badge.svg)](https://github.com/dpkimball/binder-ai/actions/workflows/image.yml)

binder-ai is Binder's **autonomous engineering orchestrator**.

`bindb` answers facts. `binder-ai` decides **how, when, and why** an LLM is used and drives the workflow. `binder-llm` is the replaceable reasoning engine. Applications (`binder-server`, `binder-cloud`, `binder-desktop`, `binder-cli`) must not call `binder-llm` directly. All AI requests flow through `AiClient`.

Architectural context comes from bindb (`Knowledge`) before any LLM call. See [docs/specs/responsibilities.md](docs/specs/responsibilities.md).

## Role

- Task-oriented API (`summarize`, `classify`, `extract`, `plan`, `execute`, …)
- Policy-based routing and provider fallback
- Versioned prompt registry
- Composable context construction
- Tool registry and execution (provider-neutral)
- Semantic retries, token budgets, cost estimates, tracing metadata
- `Runtime` for files, search, structured commands, and git (not LLM providers)
- `IssueImplementationWorkflow` for a single concrete issue → plan → edit → fmt → test → diff path
- `HygieneEngine` for optional Rust source remediation from deterministic repository findings (inspection stays outside this crate). Dashboard AI still uses `IssueImplementationWorkflow` search.
- `HygieneWorkflow` for bindb-first Hygiene: query `Knowledge`, skip binder-llm when dispatch is deterministic, validate, re-index, record execution history
- `EngineeringWorkflow` for bindb-first engineering: query `Knowledge`, open only the inspection set, invoke binder-llm, validate, re-index
- `binder-ai-plan` — one-shot inspect set for Cursor (stdout JSON, no HTTP, no model, no writes)
- `binder-ai-serve` (`--features intel`) — `POST /plan` engineering context for IDEs. No model, no writes.

HTTP for **providers** stays in `binder-llm`. Engineering intelligence is `binder-ai-plan` (CLI) and `binder-ai-serve` (HTTP). Cursor in `dpk-workspace` must call `scripts/binder-plan` before searching the tree.

QA (`binder-ai-serve-qa`, Argo `services-parent`): `GET /health` and `POST /plan` at `http://192.168.86.47:31784`. Image is built by `.github/workflows/image.yml` onto `localhost:30500`. Workstation:

```sh
export BINDER_AI_PLAN_URL=http://192.168.86.47:31784
scripts/binder-plan "what docker files have been used"
```

Omit `workspace` in the JSON (the script does). The pod uses `BINDER_AI_WORKSPACE=/data/workspace` so inspect paths resolve against the seeded checkouts.

## Usage

```rust
use binder_ai::{AiClient, Task};

# async fn demo() -> binder_ai::Result<()> {
let ai = AiClient::builder().build();

let summary = ai.summarize("long document…").await?;

let review = ai
    .execute(Task::ArchitectureReview {
        repository: "dpkimball/binder-ai".into(),
        pull_request: Some("12".into()),
    })
    .await?;
# let _ = (summary, review);
# Ok(())
# }
```

Routing picks the model from the catalog. Callers describe intent, not providers.

```rust
use binder_ai::{AiClient, AiRequest, CostClass, ExecutionPolicy, TaskKind};

# async fn prefer_local(ai: AiClient) -> binder_ai::Result<()> {
let request = AiRequest::builder()
    .task(TaskKind::CodeGeneration)
    .policy(ExecutionPolicy::PreferLocal)
    .input("write a parser")
    .build();
let _ = ai.execute(request).await?;

let free = AiRequest::builder()
    .task(TaskKind::Planning)
    .policy(ExecutionPolicy::LowestCost)
    .max_cost(CostClass::Free)
    .input("plan the issue")
    .build();
let _ = ai.execute(free).await?;
# Ok(())
# }
```

`ExecutionPolicy::LocalOnly` / `Private` never send request content to a cloud provider. `max_cost` is never silently exceeded.

```rust
use binder_ai::{AiClient, RoutingPolicy};

# fn prefer_local_client() -> binder_ai::AiClient {
AiClient::builder()
    .routing_policy(RoutingPolicy::LocalOnly)
    .build()
# }
```

Tests inject a mock LLM — no network:

```rust
use std::sync::Arc;
use binder_ai::AiClient;
use binder_llm::{ChatMessage, ChatResponse, LlmClient, MockProvider, Provider};

# fn mock_ai() -> AiClient {
let provider = MockProvider::new().with_chat(ChatResponse::new(
    Provider::Ollama,
    "mock",
    ChatMessage::assistant("ok"),
));
let llm = LlmClient::from_provider(
    Arc::new(provider),
    LlmClient::builder()
        .provider(Provider::Ollama)
        .model("mock")
        .build_config()
        .expect("config"),
);
AiClient::builder().llm(llm).build()
# }
```

Workflows talk to the development environment through `Runtime`, not `std::fs` or shell strings:

```rust
use binder_ai::{Command, MemoryRuntime, Runtime, RuntimeError, SearchRequest};

# async fn workflow(runtime: MemoryRuntime) -> Result<(), RuntimeError> {
let hits = runtime.search(SearchRequest::content("fn add")).await?;
let _fmt = runtime.execute(Command::new("cargo").arg("fmt")).await?;
# let _ = hits;
# Ok(())
# }
```

`MemoryRuntime` is for tests (in-memory files, scripted commands). `LocalRuntime::new(".")` is the development implementation.

Repository Hygiene inspects repos deterministically. When a finding needs Rust source changes, it dispatches to `IssueImplementationWorkflow` through `HygieneEngine`. Inspection itself is not AI-driven. Production AI remediation uses default catalog discovery and `LocalOnly` routing:

```rust
use binder_ai::{AiClient, Finding, FindingKind, HygieneConfig, HygieneEngine, LocalRuntime, RoutingPolicy};

# async fn hygiene(findings: Vec<Finding>) {
let ai = AiClient::builder()
    .routing_policy(RoutingPolicy::LocalOnly)
    .build();
let runtime = LocalRuntime::new(".").expect("workspace");
let engine = HygieneEngine::new(HygieneConfig::default(), ai);
let _report = engine.process_findings(runtime, &findings).await;
# }
```

The workflow git diff is the output. Hygiene never commits, merges, pushes, or opens a PR.

Bindb-first Hygiene (`HygieneWorkflow`): consume findings from live discovery, query `Knowledge` only when judgment needs an inspection set, skip binder-llm for deterministic apply, re-index and record history after validation. The Cursor skill writes `SkillApplyDocument` JSON and runs `cargo run --bin hygiene_apply`. Set `OLLAMA_HOST` for cluster/QA Ollama (Argo `ollama-qa` / `dev-proxies-ollama`). See [docs/specs/hygiene-workflow.md](docs/specs/hygiene-workflow.md). Dashboard Python apply calls that binary when it is on PATH.

Bindb-first engineering (facts from `Knowledge`, no `Runtime::search`):

```rust
use std::path::PathBuf;
use binder_ai::{
    AiClient, EngineeringRequest, EngineeringWorkflow, LocalRuntime, MemoryKnowledge,
};

# async fn engineering(ai: AiClient) -> Result<(), binder_ai::WorkflowError> {
let knowledge = MemoryKnowledge::for_files(
    "prevent overflow in add",
    [PathBuf::from("src/lib.rs")],
);
let runtime = LocalRuntime::new(".").expect("workspace");
let workflow = EngineeringWorkflow::new(runtime, ai, knowledge);
let _report = workflow
    .execute(EngineeringRequest {
        request: "prevent overflow in add".into(),
        repository_root: PathBuf::from("."),
    })
    .await?;
# Ok(())
# }
```

Production `Knowledge` is a bindb/codeintel adapter. `MemoryKnowledge` is the test double.

## Develop

```text
dpk-workspace/
  dpk-build/
  binder-llm/
  binder-ai/
```

```sh
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
make b   # lint + test; publishes to DEV Kellnr when not CI
```

Private Kellnr registry only (`publish = ["dpk"]`). Never crates.io. Depends on `binder-llm` from registry `dpk`.

## Out of scope (deferred)

Multi-agent graphs, MCP client, RAG pipelines, long-term memory, evaluation harness, human approval workflows.

## License

See repository license / parent workspace policy.
