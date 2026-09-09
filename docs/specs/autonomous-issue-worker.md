---
id: autonomous-issue-worker
title: binder-ai — entry point for autonomous issue implementation
status: draft
created: 2026-09-05
updated: 2026-09-09
owner: binder-ai
github_issue: https://github.com/dpkimball/binder-ai/issues/81
related:
  - https://github.com/dpkimball/binder-ai/issues/81
  - https://github.com/dpkimball/binder-ai/issues/79
  - https://github.com/dpkimball/foreman/issues/338
  - ./responsibilities.md
  - https://github.com/dpkimball/binder-workspace/blob/master/docs/development/01-issue-agent-pipeline.md
  - https://github.com/dpkimball/binder-workspace/blob/master/docs/development/02-issue-doctrine.md
  - https://github.com/dpkimball/binder-workspace/blob/master/docs/development/03-autonomous-issue-delivery.md
---

# binder-ai — entry point for autonomous issue implementation

## Purpose

`binder-ai` already has two real, working halves and one missing link between
them:

1. **Facts** (`bindb`/`codeintel`) — a queryable semantic graph of a repo.
2. **Planning** (`binder-ai-plan`/`binder-ai-serve`) — resolves an inspection
   set from those facts. No model call, no writes. This is the part that
   already replaces "grep the whole repo for context."
3. **Implementation** (`IssueImplementationWorkflow`, `EngineeringWorkflow`)
   — plan → invoke `binder-llm` → edit via `Runtime` → fmt → test → diff.
   Real, tested library code. **Nothing invokes it.** Grepped for consumers:
   no binary in `src/bin/`, nothing in `binder-server`, nothing in
   `repo-dashboard`.

The practical effect: every issue fix in this workspace today is driven by
an external agent (Cursor, Claude Code) doing the editing by hand, at
external-agent token cost, even for issues mechanical enough that a local
model (Ollama, already `binder-llm`'s default feature) would plausibly
suffice. This spec proposes the missing entry point.

## Non-goals

- **Not** a replacement for the Cursor `issue-agent-pipeline` doctrine
  (`binder-workspace` `docs/development/01-issue-agent-pipeline.md`). That
  doctrine's Worker → Review queues, labels, and "agent never merges" rule
  stay exactly as they are. This spec adds a second kind of Worker —
  `binder-ai` itself — for the slice of issues simple enough for it.
- **Not** removing the human-merge-only rule. The entry point produces a
  branch + PR, same as any other worker. A human merges.
- **Not** removing the four-gate review ledger (Developer/Architect/QE/
  Security) from `02-issue-doctrine.md`. The resulting PR is reviewed like
  any other.
- **Not** claiming this replaces an external agent for hard problems.
  Judgment-requiring work (design tradeoffs, ambiguous requirements,
  cross-repo changes) should still route to a human or an external agent —
  see Escalation below.

## Design

### Entry point

A new binary, `binder-ai-implement` (working name), mirroring the existing
`binder-ai-plan` shape:

```text
binder-ai-implement --repo <dir> --store <bindb-store> --name <label> \
  [--request TEXT | --issue <github-issue-url-or-number>] \
  [--workspace <dir>] [--dry-run | --execute]
```

Default is `--dry-run`. Foreman calls `--execute` and omits `--publish` (Foreman
already clones, branches, and opens the PR). `--publish` is rejected in this
slice.

Steps:

1. Load the issue body from `--request` / `--title`+`--body`, or `gh issue view`
   when `--issue` is set.
2. Resolve an inspection set the same way `binder-ai-plan` does
   (`Knowledge::plan` over bindb/codeintel), binding `--name` to `--workspace`
   so `repo:path` inspect targets resolve on a single-repo checkout.
3. If `fallback_required` or planning is not `resolved`, **decline** (JSON
   `declined: true`). Do not search the repository.
4. `--dry-run`: print the inspect set. No model, no writes.
5. `--execute`: feed the issue body + inspection set into
   `IssueImplementationWorkflow` (no search). `Runtime` edits, `cargo fmt`,
   `cargo test`, and a diff. Foreman then publishes the branch.

An HTTP twin (`binder-ai-serve` `POST /implement`) can follow the same
plan/serve split as the existing endpoints, once the CLI is proven —
out of scope for this slice.

### Escalation

Not every issue is a good fit for an unattended local model. Reuse the
existing judgment-vs-mechanical split already documented in
[`responsibilities.md`](./responsibilities.md) ("routine hygiene must not
invoke binder-llm; judgment-requiring work does") and the existing
`RoutingPolicy`/`ExecutionPolicy` model-selection logic:

- Mechanical, narrow-scope issues (the shape of `#72`–`#78` in this
  session: a wrong string, a dangling manifest entry, a doc/code drift) are
  the target case.
- Issues requiring architectural judgment, cross-repo coordination, or
  ambiguous product intent should be **detected and declined**, not
  attempted — report back "needs a human or external agent" rather than
  shipping a shallow fix. Getting this classification right matters more
  than coverage; a wrong autonomous PR costs more reviewer time than no PR.

### Review ledger stays external

This entry point does not set its own review ledger status — it only gets
the PR to `agent:needs-review`, identical to a Cursor worker's output. The
Developer/Architect reviewers and QE/Security pipeline gates from
`02-issue-doctrine.md` still apply unchanged. Given an autonomous process is
now the one editing source and opening PRs unattended, the **Security**
gate should default to human/reviewer judgment rather than pipeline-only for
at least the first several runs, until there's evidence its outputs are
trustworthy.

## Acceptance criteria

See [`#81`](https://github.com/dpkimball/binder-ai/issues/81) (CLI). Spec text
landed under [`#79`](https://github.com/dpkimball/binder-ai/issues/79).

## Phasing

1. **This spec.**
2. `binder-ai-implement` CLI, local-only, `--dry-run` default (prints the
   plan and diff without pushing) until proven on a handful of real issues.
3. Live mode: branch + PR, human review as normal.
4. Only after phase 3 is trusted: `POST /implement` on `binder-ai-serve` for
   automation (e.g. a scheduled sweep of labeled "good first issue"-style
   items), still gated by the same review ledger.
