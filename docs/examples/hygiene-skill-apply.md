# Cursor skill → HygieneWorkflow

Discovery stays on the skill (git, GitHub, CI, `cargo fmt --check`, README). This example only **applies**.

```bash
codeintel index "$REPO" --store "$STORE" --name "$NAME"

# Cluster: OLLAMA_HOST=http://dev-proxies-ollama.dev.svc.cluster.local:11434
# QA in-cluster: OLLAMA_HOST=http://ollama-service.services.svc.cluster.local:11434
cargo run --all-features --bin hygiene_apply -- \
  --repo "$REPO" --store "$STORE" --name "$NAME" \
  --codeintel "${CODEINTEL_BIN:-codeintel}" \
  --input apply.json
```

`apply.json` is `SkillApplyDocument`: `request`, `findings[]` (`kind`, `repository_root`, optional `target_ref` / `github_slug` / `project`), `operational` (default/current branch, dirty, open PR heads).

Deterministic kinds only unless `BINDER_AI_LIVE_LLM=1` (LocalOnly catalog discovery). Set `OLLAMA_HOST` to the Argo QA Ollama hop; unset still means laptop `localhost:11434`. Hosted dashboard Python apply is unchanged until the worker image ships this binary.
