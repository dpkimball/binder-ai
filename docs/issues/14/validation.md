# Validation — issue #14

`EngineeringWorkflow` on `MemoryKnowledge` + `MemoryRuntime` + `MockAiClient`. No network, no repository walk.

## Demonstration

Request: `prevent overflow in add`.

1. **bindb facts** — `MemoryKnowledge` returns inspection set `[src/lib.rs]` (`primary_source: "bindb"`). `plan_calls == 1`.
2. **Inspection set** — workflow reads only `src/lib.rs`. `README.md` is in the runtime and is never read. `search_calls == 0`.
3. **Curated reasoning** — `MockAiClient` receives two turns (plan + implement) whose file context is the inspection set only.
4. **Execute** — `src/lib.rs` is written; fmt and test succeed.
5. **bindb update** — `reindex_calls == 1` after validation. Doctrine before/after: 0 findings.

| Check | Result |
|-------|--------|
| Search used | **0** |
| Files opened | `src/lib.rs` only |
| LLM calls | 2 (binder-llm `LlmClient` / mock provider) |
| Edits outside inspection set | not applied; run marked unsuccessful; **no reindex** |
| Empty inspection set | error `knowledge_failed`; **0** LLM calls |

## Tests

`tests/engineering_workflow.rs` (4). Full `cargo test --all-features` and `clippy -D warnings` clean.

## Not in this milestone

codeintel path-dep adapter (production `Knowledge` over a live store). rustc, IDE, hosted service. Changing `IssueImplementationWorkflow` search path (hygiene still uses it).
