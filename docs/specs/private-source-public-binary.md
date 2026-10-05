---
id: private-source-public-binary
title: binder-ai — private source / public binary distribution
status: superseded
created: 2026-09-02
updated: 2026-10-05
owner: binder-ai
github_issue: https://github.com/dpkimball/binder-ai/issues/67
related:
  - https://github.com/dpkimball/binder-ai/issues/67
  - https://github.com/dpkimball/bindb/issues/97
  - https://github.com/dpkimball/binder-llm/issues/6
  - https://github.com/dpkimball/binder-workspace/issues/194
---

# binder-ai — private source / public binary distribution

> **Superseded for source licensing.** The source repository is MIT (`LICENSE`). This document describes the earlier private-source / public-binary channel. The readiness checklist lives in the codeintel repo at `docs/public-release-readiness.md`.

## Purpose

Publish **public compiled binaries** (`binder-ai-plan`, `binder-ai-serve`) without exposing private source. Mirror bindb / binder-llm cargo-dist → `*-releases`.

## Public executable contract

```text
binder-ai-plan --version
binder-ai-serve --version
binder-ai-plan --help
binder-ai-serve --help
```

`--version` prints semver + short commit + build date. No private repo URLs. No baked LAN hosts — store/workspace/profile via flags and env.

## Release automation

cargo-dist (`dist-workspace.toml`) builds `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`, includes proprietary `packaging/*`, publishes to [`dpkimball/binder-ai-releases`](https://github.com/dpkimball/binder-ai-releases).

Release CI must resolve `binder-llm` from Kellnr: secrets `PRIVATE_CARGO_URL`, `CARGO_REGISTRIES_DPK_TOKEN`, plus `GH_RELEASES_TOKEN` for the public releases repo. Setup fragment: `.github/workflows/dist-cargo-registry-setup.yml` (included via `github-build-setup`).

Public install docs must match cargo-dist naming: `binder-ai-<rust-target-triple>.tar.xz`, per-archive `*.tar.xz.sha256`, and `sha256.sum`.

### cargo-dist announcement URL drift

`github-releases-repo` uploads to the public repo, but cargo-dist still embeds **private** `dpkimball/binder-ai` download URLs in the release body and `dist-manifest.json` hosting block (upstream: [axodotdev/cargo-dist#2252](https://github.com/axodotdev/cargo-dist/issues/2252)).

Mitigation: `.github/workflows/fix-binder-ai-releases-announcement.yml` rewrites those URLs after a successful `Release` (requires `GH_RELEASES_TOKEN`).

## Image (Phase 1b)

Container image publish (GHCR / operator registry) is **deferred**. Helm/QA overlays remain the DPK image path until documented.

## Out of scope

Open-sourcing private source; shipping DPK question-bank / seed inventory inside release archives.
