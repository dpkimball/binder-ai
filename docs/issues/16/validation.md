# Validation — issue #16

Hygiene apply: findings from discovery, bindb only for application knowledge, re-index + history after success.

## Principle

Discovery (scanner / dashboard / skill) owns live git/GitHub/CI state. `HygieneWorkflow` consumes those findings. bindb is queried when a finding needs an inspection set. Routine fmt/badge/branch-delete does not call `Knowledge::plan` or binder-llm.

## Demonstration

**Deterministic fmt** (`FindingKind::CargoFmtRequired`):

| Metric | Result |
|--------|--------|
| `plan_calls` | **0** |
| binder-llm | **No** |
| Search | **0** |
| primary_source | `discovery` |
| Re-index + history | yes |
| Doctrine before / after | 0 / 0 |

**Badge** with empty inspection set: README is read from the live tree; not gated on bindb.

**Branch delete guards:** open PR head refused (no `-d`, no re-index). Current leftover + clean: `git checkout <default>` then `git branch -d`.

**AI rust bug:** `plan_calls == 1`, inspection set `[src/lib.rs]`, README not read, 2 LLM turns, re-index + history, `primary_source: bindb`.

Empty inspection set + AI: `knowledge_failed`, 0 LLM calls.

Dashboard `HygieneEngine::process_findings` is unchanged (still searches for AI). Hosted Python apply is unchanged until a binder-ai binary is in the worker image.

## Tests

`tests/hygiene_workflow.rs`. `CodeintelKnowledge` JSON mapping tests in `src/knowledge/codeintel.rs`. Full `make b`.

## Not in this milestone

New Hygiene features, dashboard Python rewrite, codeintel WAL object type for history (JSONL beside the store until then).
