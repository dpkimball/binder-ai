# Validation — issue #18

Follow-up after #17 merged. Live `CodeintelKnowledge` + `LocalRuntime` path containment.

## Acceptance

- [x] `LocalRuntime` canonicalizes before the workspace check (macOS `/var` vs `/private/var`).
- [x] `HYGIENE_E2E=1 cargo test --all-features --test hygiene_codeintel_e2e` passes: leftover branch deleted, fmt, badge, re-index, history JSONL, 0 LLM calls.
- [x] `make b` with `HYGIENE_E2E` unset still skips the live test and succeeds.

Evidence: `tests/hygiene_codeintel_e2e.rs`, `src/runtime/local_runtime.rs`. Details also in `docs/issues/16/validation.md` (live proof section).
