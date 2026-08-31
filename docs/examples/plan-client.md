---
title: Plan client (engineering intelligence)
description: Example invocations of binder-ai-plan and binder-ai-serve for IDE engineering intelligence planning requests.
category: examples
---

# Plan client (engineering intelligence)

The IDE sends a request. binder-ai returns the inspect set. The client opens only those files. Cursor in this workspace uses the one-shot CLI (compact JSON, no daemon).

```bash
# Preferred: workspace wrapper (store / codeintel defaults)
scripts/binder-plan "Where is OLLAMA_HOST configured?"

# Equivalent one-shot
cargo run --bin binder-ai-plan -- \
  --request "Where is OLLAMA_HOST configured?" \
  --workspace /Volumes/SSD/dpk-workspace \
  --store "$BINDER_AI_CODEINTEL_STORE"
```

Open `resolved_execution_paths` when `fallback_required` is false. Search the tree only when `fallback_required` is true, and report `fallback_reason`.

Optional HTTP (full payload including `doctrine`; not what Cursor should ingest):

```bash
cargo run --all-features --bin binder-ai-serve -- \
  --store "$BINDER_AI_CODEINTEL_STORE" \
  --listen 127.0.0.1:7841

curl -sS http://127.0.0.1:7841/plan \
  -H 'content-type: application/json' \
  -d '{
    "workspace": "/Volumes/SSD/dpk-workspace",
    "request": "Where is OLLAMA_HOST configured?"
  }'
```
