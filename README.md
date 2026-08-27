# binder-ai

[![CI](https://github.com/dpkimball/binder-ai/actions/workflows/ci.yml/badge.svg)](https://github.com/dpkimball/binder-ai/actions/workflows/ci.yml)

Binder's AI orchestration layer.

`binder-llm` talks to providers. `binder-ai` decides **how, when, and why** an LLM is used. Applications (`binder-server`, `binder-cloud`, `binder-desktop`, `binder-cli`) must not call `binder-llm` directly. All AI requests flow through `AiClient`.

## Role

- Task-oriented API (`summarize`, `classify`, `extract`, `plan`, `execute`, …)
- Policy-based routing and provider fallback
- Versioned prompt registry
- Composable context construction
- Tool registry and execution (provider-neutral)
- Semantic retries, token budgets, cost estimates, tracing metadata

HTTP, provider SDKs, authentication, and request mapping stay in `binder-llm`.

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

Routing picks the model. Callers describe intent, not providers.

```rust
use binder_ai::{AiClient, RoutingPolicy};

# fn prefer_local() -> binder_ai::AiClient {
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
