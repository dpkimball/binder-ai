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

## Live proof (`HYGIENE_E2E=1`)

Real git checkout (not `MemoryRuntime`) + `codeintel index` + `CodeintelKnowledge` + `LocalRuntime` + skill-shaped findings (`feature/old`, `cargo fmt`, missing badge) + `OperationalContext`.

Command: `HYGIENE_E2E=1 cargo test --all-features --test hygiene_codeintel_e2e`

Result: **pass**. LLM not invoked. Leftover branch deleted, `src/lib.rs` formatted, README badge inserted, store re-indexed, history JSONL written.

First run failed with `path_escape` on `git`/`cargo` because `LocalRuntime` compared a non-canonical `/var/folders/...` cwd to a canonical `/private/var/folders/...` root. `resolve` now canonicalizes before the containment check. That is the adapter/path issue this proof was meant to catch.

## Tests

`tests/hygiene_workflow.rs`, `tests/hygiene_codeintel_e2e.rs` (opt-in). `CodeintelKnowledge` JSON mapping in `src/knowledge/codeintel.rs`. Full `make b`.

## Not in this milestone

New Hygiene features, dashboard Python rewrite, codeintel WAL object type for history (JSONL beside the store until then).
