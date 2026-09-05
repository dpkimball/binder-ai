---
id: scope-boundary-review
title: binder-ai — scope boundary review (dashboard automation vs. AI orchestration)
status: draft
created: 2026-09-05
updated: 2026-09-05
owner: binder-ai
github_issue: https://github.com/dpkimball/binder-ai/issues/73
related:
  - https://github.com/dpkimball/binder-ai/issues/72
  - https://github.com/dpkimball/binder-ai/issues/73
  - https://github.com/dpkimball/binder-ai/issues/74
  - https://github.com/dpkimball/binder-ai/issues/67
  - ./responsibilities.md
---

# binder-ai — scope boundary review

## Purpose

Follow-on architectural review after `#67` (public artifact readiness). That
issue scrubbed source/docs for DPK-specific strings and hosts, but did not
check what actually ships inside the release archive. This review checked
the real archive contents and the crate's internal module boundaries against
its own declared scope in `CLAUDE.md` and `docs/specs/responsibilities.md`.

## Method

Read every top-level module under `src/`, grepped actual `use` statements
across modules to check for inverted/circular dependencies (rather than
inferring from filenames), diffed declared scope (`CLAUDE.md`,
`responsibilities.md`) against actual behavior, and inspected a downloaded
public release archive (`tar -tJf`) rather than assuming `dist-workspace.toml`
config matched intent.

## Findings

### 1. Core orchestration is well-scoped and well-layered

`client.rs`/`builder.rs`/`task.rs`/`request.rs`/`response.rs`, `routing/`,
`models/`, `providers/`, `prompt/`, `context/`, `tools/`, `cache/`,
`telemetry/`, `planners/`, `intel/`, `knowledge/`, `workflows/engineering/`,
and `workflows/issue/` are all legitimately AI orchestration. No circular or
inverted dependencies found between layers (`intel/` does not reach into
`workflows/`; `knowledge/`/`tools/` do not reach into each other or into
`workflows/`; `routing/`/`models/`/`policy.rs` have zero references to
`workflows/`). `providers/mod.rs` self-documents the `binder-llm` boundary
("No HTTP of its own"), and no `reqwest`/`hyper` outbound client usage exists
anywhere outside `binder-ai-serve`'s inbound `axum` server. The "no HTTP
clients, no provider SDKs" rule in `CLAUDE.md` is genuinely respected, not
just documented.

### 2. `workflows/hygiene/` and `workflows/documentation/` are repo-dashboard-specific, not general orchestration

`workflows/hygiene/skill.rs` self-documents as "JSON contract for the Cursor
hygiene skill." Its types (`SkillApplyDocument`, `SkillFindingJson`,
`FindingKind` with variants like `feature_branch_merged`, `missing_badge`,
`cargo_fmt_required`) are shaped exactly to `repo-dashboard`'s output, not to
a general orchestration vocabulary. `repo_dashboard/hygiene.py` and
`repo_dashboard/work_queue.py` are confirmed as the actual production
consumers, invoking the `hygiene_apply` binary on `PATH`.

`docs/specs/responsibilities.md` already documents hygiene-style execution
(`HygieneWorkflow`) as one of binder-ai's bindb-first execution paths — that
framing is correct in the abstract; general "autonomous workflow that plans
via bindb, executes via `Runtime`, validates, re-indexes" is squarely
in-scope. What's misplaced is the *dashboard-specific JSON contract and CLI
adapter binaries* themselves, not the workflow pattern.

Dependency direction is one-way and clean: hygiene/documentation code
consumes `AiClient`/`Knowledge`/`Runtime` from the rest of the crate; nothing
in the core crate references hygiene/documentation types (verified by grep —
zero reverse references). This makes extraction a **medium**, surgical lift,
not a rewrite. See `#73`.

### 3. Undocumented binaries ship in the public release archive

`src/bin/*.rs` is Cargo's `autobins` default — every file there becomes a
binary target whether or not it has an explicit `[[bin]]` stanza. Only
`binder-ai-plan` and `binder-ai-serve` have `[[bin]]` entries and are
documented as the public artifacts (`binder-ai-releases` README/
GETTING_STARTED). `documentation_review` and `hygiene_apply` are picked up by
`autobins`, built by `cargo-dist`, and bundled into every release archive
undocumented. Confirmed by downloading and inspecting `v0.1.1`:

```text
binder-ai-aarch64-apple-darwin/documentation_review
binder-ai-aarch64-apple-darwin/hygiene_apply
binder-ai-aarch64-apple-darwin/binder-ai-plan
binder-ai-aarch64-apple-darwin/binder-ai-serve
binder-ai-aarch64-apple-darwin/README.md
binder-ai-aarch64-apple-darwin/README.txt
binder-ai-aarch64-apple-darwin/LICENSE.txt
```

This is the concrete gap `#67`'s audit missed — it checked source and docs
for DPK strings, not actual archive contents. See `#72` for the fast fix
(feature-gate the two binaries out of the default/dist build) and `#73` for
the structural fix (extract them entirely).

### 4. `Runtime`'s "wait for multiple consumers" deferral has already been met

`CLAUDE.md`: *"Do not create a `binder-runtime` crate. Keep `Runtime` here
until multiple applications need it independently."* `Runtime` is already
used by four workflow families today (`issue`, `engineering`, `hygiene`,
`documentation`) with two implementations (`LocalRuntime`, `MemoryRuntime`).
The literal criterion (a separate *application*, not just a separate
*workflow family*, needing it) hasn't technically fired, but the spirit has.
Not actionable on its own yet — revisit once `#73` lands and `Runtime`'s
remaining consumer count drops to the two that stay in `binder-ai` (`issue`,
`engineering`). If it's still needed by 3+ families after that extraction,
extracting `Runtime` itself becomes justified by the crate's own stated bar.

### 5. Small doc/code naming drift

`responsibilities.md`/`CLAUDE.md` describe repository-discovery fallback as
gated on `fallback_required`. The actual code
(`src/workflows/issue/workflow.rs:85-91`) gates on `Option<inspection_set>`
presence — functionally equivalent, but no `fallback_required` field exists
anywhere. See `#74`.

### 6. Not a problem, just a size note

`intel/repo_ask.rs` (812 lines) and `knowledge/codeintel.rs` (669 lines) are
the two largest files in the crate. Both are doing legitimately in-scope
work. Not filed as an issue — flagged here only as a future split candidate
if either grows further.

## Decisions

- Hygiene/documentation-style autonomous workflows (plan via bindb, execute
  via `Runtime`, validate, re-index) remain a legitimate binder-ai pattern.
  This review does not challenge that pattern — see `responsibilities.md`.
- The dashboard-specific JSON contract, vocabulary, and CLI adapters
  (`hygiene_apply`, `documentation_review`) do not belong in the
  general-purpose orchestration crate or its public release artifact.
- `Runtime` extraction is explicitly deferred, not decided against. Do not
  bundle it into `#73`.

## Out of scope

- Re-architecting `workflows/engineering/` or `workflows/issue/` — both are
  in-scope and well-layered, no changes proposed.
- Any change to `binder-llm` provider boundary — confirmed already correct.
- Deciding `Runtime`'s fate now — tracked as a future revisit, not a current
  issue.
