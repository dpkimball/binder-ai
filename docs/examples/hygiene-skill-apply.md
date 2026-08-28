# Cursor skill → HygieneWorkflow

Discovery stays on the skill (git, GitHub, CI, `cargo fmt --check`, README). This example only **applies**.

```bash
codeintel index "$REPO" --store "$STORE" --name "$NAME"

cargo run --all-features --example hygiene_apply -- \
  --repo "$REPO" --store "$STORE" --name "$NAME" \
  --codeintel "${CODEINTEL_BIN:-codeintel}" \
  --input apply.json
```

`apply.json` is `SkillApplyDocument`: `request`, `findings[]` (`kind`, `repository_root`, optional `target_ref` / `github_slug` / `project`), `operational` (default/current branch, dirty, open PR heads).

Deterministic kinds only unless `BINDER_AI_LIVE_LLM=1` (LocalOnly catalog discovery, currently Ollama). Hosted dashboard Python apply is unchanged.
