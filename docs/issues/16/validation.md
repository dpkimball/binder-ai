# Validation — issue #16

`HygieneWorkflow` on `MemoryKnowledge` + `MemoryRuntime` + `MockAiClient`. No network, no repository-wide discovery.

## Demonstration

Request: `hygiene fmt` with `FindingKind::CargoFmtRequired`.

1. **bindb first** — `Knowledge::doctrine` then `Knowledge::plan`. `plan_calls == 1` before any `Runtime` read.
2. **Inspection set** — `[src/lib.rs]` from the graph. `cargo fmt` does not open those files.
3. **LLM skipped** — dispatch is deterministic. `chat_calls == 0`, `llm_invoked == false`.
4. **Execute** — scripted `cargo fmt` via `Runtime::execute`. `search_calls == 0`.
5. **Validate** — finding success; doctrine 0 findings before and after.
6. **Re-index** — `reindex_calls == 1`.
7. **History** — `ExecutionRecord { workflow: "hygiene", llm_invoked: false }` stored in bindb.

| Metric | Deterministic fmt | AI rust bug (curated) |
|--------|-------------------|------------------------|
| Planning time | `planning_ms` on report (sub-ms in tests) | same |
| Inspection set size | 1 (`src/lib.rs`) | 1 (`src/lib.rs`) |
| Files opened | 0 | 1 (`src/lib.rs`; README not read) |
| Files modified | 0 | 1 (`src/lib.rs`) |
| binder-llm invoked | **No** | **Yes** (2 turns, inspection-set context only) |
| Validation | pass | pass |
| Re-index duration | `reindex_ms` on report | same |
| Doctrine before / after | 0 / 0 | 0 / 0 |
| Search used | **0** | **0** |

Empty inspection set + deterministic fmt: success, re-index, history. Empty inspection set + AI finding: `knowledge_failed`, **0** LLM calls, no re-index.

Dashboard `HygieneEngine::process_findings` is unchanged and still searches for AI remediation.

## Tests

`tests/hygiene_workflow.rs`. Full `cargo test --all-features` and `clippy -D warnings` via `make b`.

## Not in this milestone

New Hygiene features, parser work, IDE/Cursor, hosted execution, generic workflow framework, production codeintel `Knowledge` adapter.
