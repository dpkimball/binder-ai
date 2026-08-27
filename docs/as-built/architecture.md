# binder-ai as-built

`binder-ai` is a Rust library. `AiClient` is the only type applications should construct.

```mermaid
flowchart TD
  apps["binder-server / binder-cloud / binder-desktop / binder-cli"]
  ai["binder-ai AiClient"]
  catalog["Model Catalog"]
  selector["ModelSelector"]
  llm["binder-llm LlmClient"]
  providers["Ollama / OpenAI / Anthropic / xAI / Gemini"]
  apps --> ai
  ai --> catalog
  ai --> selector
  selector --> llm
  llm --> providers
```

## Request path

1. Map the call to a `Task` / `AiRequest`.
2. Resolve a named prompt (versioned) and compose context (conversation, repo, diff, issue, …).
3. Build `ModelRequirements` from task defaults plus request policy / cost / preference.
4. `CatalogSelector` filters by availability, execution policy, capabilities, context window, cost, and quality, then scores the rest.
5. `providers` builds an `LlmClient` (or uses an injected test client).
6. Tool loop: model → registry → executor → tool result → model.
7. Semantic retries, then fallback to the next catalog target. `LocalOnly` / `Private` never include cloud targets.
8. Record `AiUsage`, estimated cost, and tracing metadata (never prompt text).

Infrastructure HTTP retries stay in `binder-llm`.

## Catalog merge

Effective catalog = builtin TOML metadata + Ollama discovery (`list_models`) + user TOML overlays + which providers have credentials (or were injected in tests).
