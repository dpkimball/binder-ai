---
title: bindb, binder-ai, and binder-llm responsibilities
status: spec
github_issue: https://github.com/dpkimball/binder-ai/issues/14
---

# Architecture — bindb, binder-ai, binder-llm

Git is the source of truth. **bindb** is the persistent semantic knowledge layer. **binder-ai** is the autonomous engineering orchestrator. **binder-llm** is a replaceable reasoning engine.

No component assumes responsibilities that belong to another.

## bindb — facts

bindb owns persistent knowledge. codeintel is a bindb *consumer* that writes and queries the code graph; it is not a second knowledge store.

Examples of what bindb holds:

- Semantic graph, source index, repository metadata
- Architectural doctrine, dependency and call graphs
- Historical snapshots, impact and cross-repository relationships
- Agent execution history (when binder-ai records it)

bindb answers questions of **fact**: what depends on this, which doctrine applies, which files are implicated, what changed.

bindb does **not** perform reasoning, generate code, or run workflows.

## binder-ai — orchestration

binder-ai owns autonomous execution:

1. Receive the engineering request
2. Query bindb (via a `Knowledge` surface)
3. Collect semantic context and the inspection set
4. Determine the execution plan
5. Decide whether judgment is required
6. Invoke binder-llm **only** with that curated context
7. Execute implementation through `Runtime` (files, fmt, test, git)
8. Validate
9. Re-index bindb after a successful validation
10. Produce the execution report

binder-ai does **not** implement LLM reasoning (no provider SDKs, no prompt-as-transport). It does **not** discover application structure by walking the repository when bindb can answer.

`IssueImplementationWorkflow` still uses `Runtime::search` when no inspection set is supplied (`HygieneEngine::process_findings`). Bindb-first Hygiene uses `HygieneWorkflow` (no search). Bindb-first engineering uses `EngineeringWorkflow`.

## binder-llm — reasoning

binder-llm executes model inference on **supplied** messages. It is the replaceable reasoning engine.

What callers get from it (orchestrated as binder-ai tasks): code generation, review, architecture review, tradeoff analysis, test and docs generation, refactoring suggestions, pattern recognition, natural language understanding.

binder-llm:

- Operates on curated context
- Does **not** walk repositories or query bindb
- Does **not** own prompts, tools, or workflows (those stay in binder-ai)
- Is swapped by configuration (`Provider`, model, credentials) without changing binder-ai orchestration

## Execution model

```text
Engineering Request
        │
        ▼
binder-ai
        │
        ├── Query bindb (Knowledge::plan / doctrine)
        │
        ├── Collect semantic context (inspection set)
        │
        ├── Determine execution plan
        │
        ├── Invoke binder-llm when judgment is required
        │     (only inspection-set files)
        │
        ├── Execute via Runtime
        │
        ├── Validate (fmt / test)
        │
        ├── Re-index bindb
        │
        └── Produce execution report
```

## Decision rules

Use **bindb** when the answer is a fact: dependency lookup, impact, doctrine, historical comparison, repository selection, inspection set.

Use **binder-llm** when judgment is required: implement this, explain this design, compare alternatives, review, generate tests, improve readability, write docs, evaluate tradeoffs. Routine Hygiene (fmt, clippy `--fix`, badge, delete merged branch) must not invoke binder-llm.

## Design constraints

- binder-ai queries bindb before invoking binder-llm whenever architectural context is required.
- binder-llm receives curated context, never a repository-wide dump from `find` / `rg`.
- bindb remains the authoritative semantic model.
- binder-llm implementations are replaceable without changing binder-ai orchestration.

## Non-goals

New parsers, rustc / rust-analyzer, IDE or Cursor product integration, hosted services, LLM planning improvements inside bindb.
