# Pull Request Checklist

Library crate (`binder-ai`). No Kubernetes deploy.

## Hard gate (before `gh pr create`)

- [ ] `cargo fmt -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all-features`
- [ ] `make b` (or report why it was skipped)

## Issue link (required)

- [ ] Target GitHub issue exists (Issue Doctrine body)
- [ ] PR body names `#N` and chooses **Closes** vs **Updates** (`Implements`)
