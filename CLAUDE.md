# CLAUDE.md

Guidance for agents working in `binder-ai`.

## What this crate is

Opinionated AI orchestration for Binder. Applications talk only to [`AiClient`]. This crate owns routing, prompts, context, conversations, tool orchestration, fallback, semantic retries, budgets, and telemetry.

Do **not** add HTTP clients, provider SDKs, auth, or provider JSON mapping. Those belong in `binder-llm`.

Do **not** add REST APIs. This is a library.

## Layout

```
src/
  client.rs, builder.rs, config.rs   # AiClient
  task.rs, request.rs, response.rs
  routing/, policy.rs, router.rs
  prompt/, context/, conversation.rs, session.rs
  tools/, telemetry/, cache/, providers/, planners/
```

Built-in prompts live in `prompt/registry.rs`. Routing tables live in `routing/selector.rs`.

## Build

```sh
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt -- --check
make b
```

Default features enable all `binder-llm` providers so the router can pick among them. Tests inject `LlmClient` / `MockProvider` and must not require network.

## Logging

`tracing` metadata only: task, policy, model, token counts, latency, fallbacks, tool-call counts, estimated cost, error kind. Never log prompts, completions, tool arguments that may contain secrets, or API keys.

## Publish

Private Kellnr (`CARGO_REGISTRY=dpk`). `env.sh` sets `PUBLISH_CRATE=true` for local `make b`.
