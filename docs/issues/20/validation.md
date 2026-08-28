# Validation — issue #20

Cursor skill apply path: live discovery JSON → `hygiene_apply` example → `HygieneWorkflow` on a **controlled** temp git checkout (not a workspace project). Dashboard Python `run_hygiene` unchanged.

## Skill discovery (agent, not bindb)

On `/tmp/hygiene-skill-C8k9WU/repo`:

| Check | Result |
|-------|--------|
| `git branch --list feature/old` | leftover `feature/old` |
| `cargo fmt --check` | exit 1 (unformatted `src/lib.rs`) |
| README `badge.svg` | missing |

Wrote `apply.json` (`SkillApplyDocument`). Then `codeintel index` (3 files, 59ms) and:

```
cargo run --all-features --example hygiene_apply -- \
  --repo $REPO --store $STORE --name hygiene-skill \
  --codeintel $CODEINTEL --input apply.json
```

## Apply report

| Field | Value |
|-------|--------|
| success | true |
| llm_invoked | false |
| search_used | false |
| primary_source | discovery |
| planning_ms | 0 |
| reindex_ms | 46 |
| history_recorded | true |

Outcomes: deleted `feature/old`; `cargo fmt`; inserted CI badge. After: only `master`; formatted `add`; README badge present; `bindb.hygiene-history.jsonl` written.

## Tests

- `workflows::hygiene::skill` JSON mapping (unknown kind rejected)
- `HYGIENE_SKILL=1 cargo test --all-features --test hygiene_skill_apply` (skill-shaped discovery in-process)

Default `make b` skips the live test unless `HYGIENE_SKILL=1`.

## Later live proofs (same branch)

**Real workspace (`codeintel`):** leftover `feat/autonomous-workflow` (PR #12 squash-merged, two-dot identical to `origin/master`). Skill JSON → `hygiene_apply`: checked out `master`, `git branch -d` succeeded, `llm_invoked: false`. Then `git pull --ff-only origin master`. Remote `origin/feat/autonomous-workflow` still exists.

**Hosted Python (dev board):** `POST /api/hygiene?dry_run=true&repo=workspace-host` → request 381, worker wrote one `report_only` outcome (`feature/rename-volume-ssd` has a real tree vs `master`). No mutations. Python apply path not replaced.

**Live binder-llm:** `BINDER_AI_LIVE_LLM=1` on `/tmp/hygiene-ai-live` (tiny Cargo git crate, not a workspace project). bindb plan inspection set `[src/lib.rs]`. Ollama `qwen3-coder:30b`: `llm_invoked: true`, `primary_source: bindb`, `add` → `saturating_add`, tests passed (~36s).
