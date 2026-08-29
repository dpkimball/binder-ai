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

binder-ai owns autonomous execution **and** engineering intelligence for tools:

1. Receive the engineering request
2. Query bindb (via a `Knowledge` surface)
3. Collect semantic context and the inspection set
4. Resolve those targets into the caller’s workspace when asked
5. Return that context to an IDE (`binder-ai-plan` / `POST /plan`) **or** continue into apply workflows. Repository discovery is allowed only when `fallback_required` is true.
6. Determine the execution plan (apply path only)
7. Decide whether judgment is required
8. Invoke binder-llm **only** with that curated context (never on `binder-ai-plan` / `POST /plan`)
9. Execute implementation through `Runtime` (files, fmt, test, git)
10. Validate
11. Re-index bindb after a successful validation
12. Produce the execution report

binder-ai does **not** implement LLM reasoning (no provider SDKs, no prompt-as-transport). It does **not** discover application structure by walking the repository when bindb can answer.

`IssueImplementationWorkflow` still uses `Runtime::search` when no inspection set is supplied (`HygieneEngine::process_findings`). Bindb-first *engineering* uses `EngineeringWorkflow`. Hygiene **apply** uses `HygieneWorkflow`: findings come from live discovery; bindb is queried only when judgment needs an inspection set.

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

Use **bindb** when the answer is a fact about the application: dependency lookup, impact, doctrine, historical comparison, inspection set for a source change.

Use **live inspection** when the answer is current operational state: dirty trees, leftover branches, CI, PR heads, nested clones, secrets.

Use **binder-llm** when judgment is required: implement this, explain this design, compare alternatives, review, generate tests, improve readability, write docs, evaluate tradeoffs. Routine Hygiene (fmt, clippy `--fix`, badge, delete merged branch) must not invoke binder-llm.

## Design constraints

- binder-ai queries bindb before invoking binder-llm whenever **architectural** context is required. Operational hygiene discovers live git/GitHub/CI state first, then queries bindb only for source-change inspection sets.
- binder-llm receives curated context, never a repository-wide dump from `find` / `rg`.
- bindb remains the authoritative semantic model.
- binder-llm implementations are replaceable without changing binder-ai orchestration.

## Non-goals

New parsers, rustc / rust-analyzer, Cursor extension, LLM planning improvements inside bindb. Additional intelligence endpoints beyond `POST /plan` are later milestones.
