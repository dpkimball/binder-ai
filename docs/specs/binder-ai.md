# binder-ai

github_issue: https://github.com/dpkimball/binder-ai/issues/1

Binder's AI orchestration layer. Applications never call `binder-llm` directly. All AI traffic flows through `AiClient`.

`binder-llm` communicates with LLM providers. `binder-ai` decides how, when, and why an LLM is used.

## In scope

- Task-oriented API: `chat`, `chat_json`, `execute`, `summarize`, `classify`, `extract`, `embeddings`, `complete`, `plan`
- Policy routing (`BestQuality`, `Fastest`, `LowestCost`, `LocalOnly`, `CloudOnly`, `Balanced`) and configurable fallback
- Versioned prompt registry and composable context builders
- Abstract conversation storage
- Tool registry + executor (provider-neutral loop)
- Semantic retries (malformed JSON, tool failure, context too large, provider unavailable)
- Token budgets, cost estimates (no billing), tracing metadata, optional caches

## Out of scope

HTTP / provider SDKs / auth / request mapping / REST APIs (those are `binder-llm`).

Deferred: multi-agent orchestration, planning graphs, workflow execution, MCP, RAG, long-term memory, knowledge graphs, evaluation, A/B prompts, human approval.

## Swap contract

Consumers express **intent** (`Task`, named prompts, routing policy). They do not select providers or models unless they explicitly override configuration.
