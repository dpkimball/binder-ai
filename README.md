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
- `EngineeringWorkflow` for bindb-first engineering: query `Knowledge`, open only the inspection set, invoke binder-llm, validate, re-index
- `binder-ai-plan` — one-shot inspect set for Cursor (stdout JSON, no HTTP, no model, no writes)
- `binder-ai-implement` — unattended issue implementation. The default is `--dry-run` (plan only, no model call, no writes). `--execute` may change files in the workspace through the implementation workflow. `--publish` is rejected.
- `binder-ai-serve` (`--features intel`) — HTTP adapter with no authentication. `POST /plan` returns an inspection set and does not call a model or write files. `POST /ask` and `POST /explain` can call a model. Question routes can change the question bank. `POST /federation/impact` is read-only. With no `--listen` and no `BINDER_AI_LISTEN`, the process binds to `127.0.0.1:7841`. The container image sets `0.0.0.0:7841`; do not publish that port without an access control in front of it.

HTTP for **providers** stays in `binder-llm`. Engineering intelligence is `binder-ai-plan` (CLI) and `binder-ai-serve` (HTTP). Workspace Cursor clients should call their `scripts/binder-plan` (or equivalent) before searching the tree.

**Operator QA / image:** set `BINDER_AI_PLAN_URL` to your serve endpoint (no baked LAN default in this crate). Image tags and in-cluster registry names live in your Helm/overlay values, not library defaults. Workstation example:

```sh
export BINDER_AI_PLAN_URL=http://<your-host>:<port>
scripts/binder-plan "what docker files have been used"
```

Omit `workspace` in the JSON when the pod sets `BINDER_AI_WORKSPACE` so inspect paths resolve against seeded checkouts.

**Public binaries:** tag `v*` → [`binder-ai-releases`](https://github.com/dpkimball/binder-ai-releases). See [`docs/specs/private-source-public-binary.md`](docs/specs/private-source-public-binary.md).

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

`MemoryRuntime` is for tests (in-memory files, scripted commands). `LocalRuntime::new(".")` is the development implementation. It runs commands on the host with the process environment. It is not a sandbox.

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

Clone `binder-llm` beside this repository. The crate depends on that sibling path. No private registry is required to build.

```text
workspace/
  binder-llm/
  binder-ai/
```

```sh
cd binder-ai
cargo fmt -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

`make b` publishes this crate to the private Kellnr registry (`publish = ["dpk"]`). That registry is an internal publishing path. crates.io publication is a separate choice.

Plan and ask also need a `codeintel` binary and a bindb store at runtime. Those are not Cargo dependencies.

## Out of scope (deferred)

Multi-agent graphs, MCP client, RAG pipelines, long-term memory, evaluation harness, human approval workflows.

## Stability

0.1.x. `POST /plan` is the supported IDE entry. The HTTP service has no authentication. The process default is `127.0.0.1:7841`. The container listens on `0.0.0.0:7841` so a ClusterIP Service can reach it, and that port must not be published without an access control in front of it.

A locked four-repository walkthrough is in [docs/examples/four-repo.md](docs/examples/four-repo.md).

## Contributing

Pull requests are not accepted. Fork the repository to keep your own changes. Open an issue for bugs or suggestions. Security reports go through [GitHub Security Advisories](https://github.com/dpkimball/binder-ai/security/advisories/new). See [SECURITY.md](SECURITY.md).

## License

MIT. See [LICENSE](LICENSE).
