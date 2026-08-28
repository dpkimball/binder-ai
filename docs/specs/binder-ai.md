# binder-ai

github_issue: https://github.com/dpkimball/binder-ai/issues/1
github_issue_catalog: https://github.com/dpkimball/binder-ai/issues/3
github_issue_runtime: https://github.com/dpkimball/binder-ai/issues/4
github_issue_engineering: https://github.com/dpkimball/binder-ai/issues/14
github_issue_hygiene_workflow: https://github.com/dpkimball/binder-ai/issues/16

Binder's AI orchestration layer. Applications never call `binder-llm` directly. All AI traffic flows through `AiClient`.

`bindb` answers facts. `binder-llm` executes reasoning on supplied context. `binder-ai` owns the workflow: query bindb, curate context, invoke binder-llm, execute, validate, re-index. See [responsibilities.md](responsibilities.md).

## In scope

- Task-oriented API: `chat`, `chat_json`, `execute`, `summarize`, `classify`, `extract`, `embeddings`, `complete`, `plan`
- Data-driven model catalog (`ModelDescriptor`, capabilities, cost class, quality hints, availability)
- Capability-aware routing (`ExecutionPolicy`: `LocalOnly`, `PreferLocal`, `LowestCost`, `Balanced`, `BestQuality`, `CloudOnly`, `FreeFirst`, `Private`)
- Deterministic `ModelSelector` with policy-driven fallback (no per-model name branches)
- Ollama discovery via `binder-llm` `list_models` (`AiClient::refresh_catalog`)
- User catalog overrides (TOML)
- Versioned prompt registry and composable context builders
- Abstract conversation storage
- Tool registry + executor (provider-neutral loop)
- Semantic retries (malformed JSON, tool failure, context too large, provider unavailable)
- Token budgets, cost estimates (no billing), tracing metadata, optional caches
- `Runtime` trait (`LocalRuntime` / `MemoryRuntime`) for files, search, structured commands, and git — independent of LLM providers
- `IssueImplementationWorkflow`: search → plan → edit → `cargo fmt` → `cargo test` → git diff (not a generic engine)
- `EngineeringWorkflow`: `Knowledge` (bindb) → inspection set → curated `AiClient` context → `Runtime` edits → fmt/test → re-index
- `HygieneWorkflow`: `Knowledge` (bindb) → existing Hygiene dispatch → skip binder-llm when deterministic → validate → re-index → `record_execution`

## Out of scope

HTTP / provider SDKs / auth / request mapping / REST APIs (those are `binder-llm`).

Deferred: automatic Ollama pulls, live quality benchmarks, autonomous paid-model escalation, electricity cost, GPU scheduling, multi-agent graphs, MCP, RAG, long-term memory, evaluation, A/B prompts, human approval. Docker/K8s/SSH/sandbox runtimes and a `binder-runtime` crate wait until multiple apps need the abstraction.

## Swap contract

Consumers express **intent** (`Task` / `AiTask`, named prompts, `ExecutionPolicy`, optional `max_cost`). They do not select providers or models unless they explicitly override configuration.

`LocalOnly` and `Private` never select a cloud model. An explicit `max_cost` is never silently exceeded (`AiError::NoEligibleModel`).
