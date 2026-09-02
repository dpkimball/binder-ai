binder-ai
=========

Official compiled release of binder-ai.

Includes:
  binder-ai-plan   — local plan against YOUR codeintel/bindb store
  binder-ai-serve  — HTTP service (operator deploy)

Source is privately maintained and is not included.

Prerequisites
-------------

1. Public bindb (v0.1.2+) — https://github.com/dpkimball/bindb-releases
2. Public codeintel — https://github.com/dpkimball/codeintel-releases
3. A store you indexed with: codeintel index <repo> --store <dir> --name <label>

Installation
------------

1. Verify the archive against the matching `*.tar.xz.sha256` file.
2. Extract (`tar -xJf …`).
3. Place binder-ai-plan (and optionally binder-ai-serve) on your PATH.

Quick start
-----------

    export BINDER_AI_CODEINTEL_STORE=/path/to/your/store
    binder-ai-plan --request "Where is auth handled?" \
      --store "$BINDER_AI_CODEINTEL_STORE"

Or use --store / --workspace / --codeintel flags. No baked DPK hosts.

Full guide: https://github.com/dpkimball/binder-ai-releases/blob/master/GETTING_STARTED.md

Verify
------

    binder-ai-plan --version
    binder-ai-serve --version
    binder-ai-plan --help

Configuration
-------------

BINDER_AI_CODEINTEL_STORE / --store
BINDER_AI_WORKSPACE / --workspace
BINDER_WORKSPACE_PROFILE (optional)
CODEINTEL_BIN / --codeintel (optional)

License
-------

See LICENSE.txt. Proprietary binary distribution — not open-source software.

Support
-------

https://github.com/dpkimball/binder-ai-releases
