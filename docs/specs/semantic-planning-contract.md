---
title: Semantic planning before repository discovery
description: Contract requiring that engineering requests attempt bindb planning before repository-wide discovery, with search as an explicit fallback only when fallback_required is true.
category: specs
status: spec
github_issue: https://github.com/dpkimball/binder-ai/issues/27
---

# Semantic planning contract

Engineering requests always attempt bindb planning before repository-wide discovery. Search is a fallback, never the default, and never silent.

This crate owns the contract. IDE plugins that intercept editor search are out of scope.

## Flow

```text
Engineering Request
        │
        ▼
binder-ai plan (bindb)
        │
        ├── resolved → open only inspection set
        │
        ├── partial → open resolved paths; discovery recommended, not required
        │
        └── unresolved / fallback_required
                │
                ▼
        repository discovery (explicit, with reason)
```

## Planner response

`planning_source` is `bindb` when planning was attempted. `planning_result` is `resolved`, `partial`, or `unresolved`.

```json
{
  "planning_source": "bindb",
  "planning_result": "resolved",
  "fallback_required": false,
  "inspection_set": ["repo:path"]
}
```

When `partial` or `unresolved`:

- `fallback_reason` is set (`not_indexed`, `parser_coverage_insufficient`, `workspace_resolution_failed`, `semantic_unavailable`)
- `repository_discovery_recommended` says whether a repo walk is appropriate
- `fallback_required` is true only when the client must not proceed from the inspect set alone

## Fallback

Repository discovery is allowed only when `fallback_required` is true, which happens when:

- bindb returned no inspect set (`not_indexed`)
- parser coverage is insufficient
- workspace resolution produced no usable files
- semantic planning is unavailable

Clients must record that search ran. They must not search when the plan is `resolved`.

## Metrics

Every plan and every engineering acquire records:

- planning source
- planning result
- fallback required
- fallback reason
- inspection set size
- files opened from inspection set
- repository search performed (yes/no)

## Non-goals

IDE-specific enforcement, Cursor integration details, editor plugins, parser improvements.
