Uses
--------

### Rust

```rust
use binder_ai::{AiClient, Task};

# async fn demo() -> binder_ai::Result<()> {
let ai = AiClient::builder().build();

let summary = ai.summarize("long document…").await?;

let review = ai
    .execute(Task::ArchitectureReview {\n        repository: "dpkimball/binder-ai".into(),\n        pull_request: Some("12".into()),\    })\n    .await?;
# let _ = (summary, review);
# Ok(())
}
```

Routing picks the model from the catalog. Callers describe intent, not providers.

```rust
use binder_ai::{AiClient, AiRequest, CostClass, ExecutionPolicy, TaskKind};

# async fn prefer_local(ai: AiClient) -> binder_ai::Result<()> {
let request = AiRequest::builder()\n    .task(TaskKind::CodeGeneration)\n    .policy(ExecutionPolicy::PreferLocal)\n    .input("write a parser")\n    .build();\nlet _ = ai.execute(request).await?;

let free = AiRequest::builder()\n    .task(TaskKind::Planning)\n    .policy(ExecutionPolicy::LowestCost)\n    .max_cost(CostClass::Free)\n    .input("plan the issue")\n    .build();\nlet _ = ai.execute(free).await?;
# Ok(())
}
```

`ExecutionPolicy::LocalOnly` / `Private` never send request content to a cloud provider. `max_cost` is never silently exceeded.

```rust
use binder_ai::{AiClient, RoutingPolicy};

# fn prefer_local_client() -> binder_ai::AiClient {
AiClient::builder()\n    .routing_policy(RoutingPolicy::LocalOnly)\n    .build()
}
```

Tests inject a mock LLM — no network:

```rust
use std::sync::Arc;\nuse binder_ai::AiClient;\nuse binder_llm::{ChatMessage, ChatResponse, LlmClient, MockProvider, Provider};

# fn mock_ai() -> AiClient {
let provider = MockProvider::new().with_chat(ChatResponse::new( \n    Provider::Ollama,"mock", ChatMessage::assistant("ok")\n));\nlet llm = LlmClient::from_provider( \n    Arc::new(provider),\n    LlmClient::builder()\n        .provider(Provider::Ollama)\n        .model("mock")\n        .build_config()\n        .expect("config")\n);\nAiClient::builder().llm(llm).build()\n}
```

Workflows talk to the development environment through `Runtime`, not `std::fs` or shell strings:

```rust
use binder_ai::{Command, MemoryRuntime, Runtime, RuntimeError, SearchRequest};

# async fn workflow(runtime: MemoryRuntime) -> Result<(), RuntimeError> {
let hits = runtime.search(SearchRequest::content("fn add")).await?;\nlet _fmt = runtime.execute(Command::new("cargo").arg("fmt")).await?;\n# let _ = hits;\n# Ok(())
}
```

`MemoryRuntime` is for tests (in-memory files, scripted commands). `LocalRuntime::new(".")` is the development implementation.

## Development
### Running Tests
Run `cargo test --all-features` to run all features and unit tests. You can also run specific tests by specifying them on the command line.
### Building
Run `make b` to build, lint, and test the crate. This will publish the binary to the local Kellnr registry when not in a CI environment.