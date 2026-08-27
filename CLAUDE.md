# CLAUDE.md

Guidance for agents working in `binder-ai`.

## What this crate is

Opinionated AI orchestration for Binder. Applications talk to [`AiClient`] for models and [`Runtime`] for the development environment. This crate owns routing, prompts, context, conversations, tool orchestration, fallback, semantic retries, budgets, telemetry, and workspace operations.

Do **not** add HTTP clients, provider SDKs, auth, or provider JSON mapping. Those belong in `binder-llm`.

Do **not** add REST APIs. This is a library.

Do **not** create a `binder-runtime` crate. Keep `Runtime` here until multiple applications need it independently.

## Layout

```
src/
  client.rs, builder.rs, config.rs   # AiClient
  task.rs, request.rs, response.rs
  routing/, policy.rs, router.rs
  prompt/, context/, conversation.rs, session.rs
  tools/, telemetry/, cache/, providers/, planners/
  runtime/                           # LocalRuntime + MemoryRuntime
  workflows/issue/                   # IssueImplementationWorkflow
```

Built-in prompts live in `prompt/mod.rs`. Catalog routing lives in `routing/selector.rs` and `models/`. Workflows must use `Runtime` instead of `std::fs` / `Command` / `git2`.

## Build

```sh
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt -- --check
make b
```

Default features enable all `binder-llm` providers so the router can pick among them. Tests inject `LlmClient` / `MockProvider` and must not require network.

## Logging

`tracing` metadata only: task, policy, model, token counts, latency, fallbacks, tool-call counts, estimated cost, error kind, command executable/exit/duration. Never log prompts, completions, tool arguments that may contain secrets, API keys, or command stdout/stderr.

## Publish

Private Kellnr (`CARGO_REGISTRY=dpk`). `env.sh` sets `PUBLISH_CRATE=true` for local `make b`.
