---
title: Agent Workflow POC #1 — Hygiene from bindb
status: spec
github_issue: https://github.com/dpkimball/binder-ai/issues/16
github_issue_e2e: https://github.com/dpkimball/binder-ai/issues/18
github_issue_skill: https://github.com/dpkimball/binder-ai/issues/20
---

# Hygiene apply from bindb

HygieneWorkflow is the **apply** path. It does not replace Hygiene discovery.

Live operational facts (dirty trees, leftover branches, CI, PR heads, nested clones, secrets) stay on the scanner / dashboard / Cursor skill. Forcing those into bindb is the wrong abstraction.

## Principle

- **bindb-first** when the question is application knowledge (which files are implicated, doctrine, re-index after a patch).
- **Live inspection** when the question is current external/runtime state (git, GitHub, CI).

## Split

```text
Scanner / dashboard / skill
        │  findings + OperationalContext
        ▼
HygieneWorkflow
        │
        ├── bindb Knowledge::plan / doctrine
        │     only if a finding needs judgment
        │
        ├── binder-llm (inspection-set files only)
        │     skipped for deterministic apply
        │
        ├── Runtime execute / validate
        │
        ├── Knowledge::reindex
        │
        └── Knowledge::record_execution
```

`HygieneEngine::process_findings` remains for callers that still search. New apply work uses `HygieneWorkflow`.

The Cursor skill discovers operational findings, writes the JSON document in `SkillApplyDocument`, then runs `cargo run --example hygiene_apply` (not `HygieneEngine::process_findings`). Dashboard Python `run_hygiene` is unchanged.

## Rules

- Findings are an input. HygieneWorkflow does not walk the board or GitHub.
- Deterministic apply (fmt, clippy `--fix`, badge, branch `-d`) does not call `Knowledge::plan` and does not require an inspection set.
- README badge insertion reads the live file; it is not gated on the semantic graph.
- Branch delete uses `OperationalContext` (default branch, current branch, dirty, open PR heads) with the same guards as dashboard/skill.
- AI findings query bindb for the inspection set; empty set → `KnowledgeFailed`, 0 LLM calls.
- After validation succeeds: re-index and record execution history.
- Production `Knowledge` is `CodeintelKnowledge` (codeintel CLI → Architecture JSON). `MemoryKnowledge` is the test double.

## Non-goals

New Hygiene finding kinds, parser improvements, IDE/Cursor product integration beyond the skill JSON command, hosted dashboard rewrite, generic workflow framework. Dashboard Python apply remains until the worker image ships a binder-ai binary.
