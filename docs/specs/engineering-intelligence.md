---
title: Engineering intelligence service — POST /plan
status: spec
github_issue: https://github.com/dpkimball/binder-ai/issues/26
---

# Engineering intelligence — plan

binder-ai is the engineering intelligence service for development tools. Git is the source of truth. bindb is application knowledge. binder-llm is optional reasoning and is **not** used here.

The IDE edits code. binder-ai answers what to inspect.

## First milestone

`POST /plan` returns structured engineering context for a natural-language request.

The client must not walk the repository. The client must not know bindb’s store layout. The store path is a server flag / `BINDER_AI_CODEINTEL_STORE` (or `--store` on the one-shot CLI).

Cursor in this workspace **must** call the planner before Grep/Glob. That is the product: binder-ai chooses the inspect set; Cursor edits those files. See `binder-ai-plan` and workspace `scripts/binder-plan`.

## Contract

```text
IDE  --POST /plan-->  binder-ai-serve
                         │
                         └── Knowledge::plan (bindb)
                         └── resolve semantic repo:path into workspace
                         └── JSON context (no writes, no model)
```

Request:

```json
{
  "workspace": "/path/to/workspace",
  "request": "Where is OLLAMA_HOST configured?"
}
```

`workspace` is optional. When set, semantic targets are resolved into that tree. Missing checkouts appear in `unresolved`.

Response fields: `primary_source`, `inspection_set`, `resolved_execution_paths`, `reasoning`, `doctrine`, `impact`. `unresolved` is omitted when empty.

## Binaries

One-shot (Cursor / scripts; no HTTP, no axum):

```sh
binder-ai-plan --request "Where is OLLAMA_HOST configured?" \
  --workspace /Volumes/SSD/dpk-workspace \
  --store "$BINDER_AI_CODEINTEL_STORE"
```

Stdout is `AgentPlan` JSON: inspect set and resolved paths, **without** the store-wide `doctrine` dump (that list is not request-local and is expensive in an IDE prompt).

Workspace wrapper (defaults store, workspace, codeintel binary):

```sh
scripts/binder-plan "Where is OLLAMA_HOST configured?"
```

Long-running HTTP (full `PlanResponse`, including `doctrine`):

```sh
binder-ai-serve --store "$STORE" --listen 127.0.0.1:7841
```

## Cursor agent contract

Always-apply rule: `.cursor/rules/binder-ai-plan-first.mdc`. Skill: `.cursor/skills/binder-ai-plan/SKILL.md`.

The agent runs `scripts/binder-plan`, then Read/edits **only** `resolved_execution_paths` when `fallback_required` is false. If `fallback_required` is true, repository discovery is allowed and must be reported (never a silent Grep). Exceptions: user-named paths, attached diffs, operational git/CI, work on the planner itself.

A Cursor/VS Code **extension** is still a later milestone.

## Semantic-first contract

See [semantic-planning-contract.md](semantic-planning-contract.md) ([#27](https://github.com/dpkimball/binder-ai/issues/27)). `EngineeringWorkflow` calls `plan` first and searches only when `fallback_required` is true.

## Non-goals

Cursor/VS Code extension, LLM apply, hygiene workflow changes, additional endpoints (`doctrine`, `impact`, `dependencies`, architecture, implementation-patterns, validation). Parser improvements.
