---
title: Agent Workflow POC #1 — Hygiene from bindb
status: spec
github_issue: https://github.com/dpkimball/binder-ai/issues/16
---

# Hygiene from bindb

Prove that binder-ai can execute the existing Hygiene workflow using bindb as the primary knowledge source. This POC is the execution model, not AI quality and not new Hygiene features.

## Flow

```text
Hygiene Request
        │
        ▼
binder-ai HygieneWorkflow
        │
        ▼
Knowledge::plan() / doctrine()
        │
        ▼
Inspection Set (semantic graph)
        │
        ▼
Reasoning Required?
      /         \
     No         Yes
     │           │
     │     binder-llm
     │     (inspection-set files only)
      \         /
       ▼
Existing Hygiene dispatch + Runtime
       │
       ▼
Validate
       │
       ▼
Knowledge::reindex()
       │
       ▼
Knowledge::record_execution()
```

## Responsibilities

- **bindb (`Knowledge`)** — repository metadata, doctrine, inspection set, prior execution history. Does not decide how Hygiene runs.
- **binder-ai (`HygieneWorkflow`)** — query bindb first, dispatch existing finding kinds, skip binder-llm when dispatch is deterministic, validate, re-index, record history.
- **binder-llm** — only when a finding requires judgment (Rust source remediation). Receives curated inspection-set files, never a repository walk.

`HygieneEngine::process_findings` remains the dashboard path and may still `Runtime::search` via `IssueImplementationWorkflow`. This POC does not change that caller.

## Rules

- Query bindb before any file inspection.
- Inspection set comes from the semantic graph (`KnowledgePlan.inspection_set`).
- Do not call `Runtime::search` on this path.
- Empty inspection set is allowed for fully deterministic command-only work (`cargo fmt`, clippy `--fix`, `git branch -d`).
- Empty inspection set plus an AI finding is `KnowledgeFailed` with **0** LLM calls.
- README badge insertion reads `README.md` only when that path is in the inspection set.
- Re-index and execution history are recorded only after validation succeeds.

## Non-goals

New Hygiene finding kinds, parser improvements, IDE/Cursor integration, hosted execution, generic workflow framework.
