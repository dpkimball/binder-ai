binder-ai
=========

Official compiled release of binder-ai.

Includes public executables for engineering intelligence:
  binder-ai-plan
  binder-ai-serve

Source code is privately maintained and is not included.

Installation
------------

1. Verify the archive against the matching `*.tar.xz.sha256` file from the
   GitHub Release (or use the release’s `sha256.sum`).
2. Extract this archive (`tar -xJf …` for `.tar.xz`).
3. Place the binaries on your PATH.

Verify
------

    binder-ai-plan --version
    binder-ai-serve --version
    binder-ai-plan --help
    binder-ai-serve --help

Configuration
-------------

No baked operator hosts. Set BINDER_AI_CODEINTEL_STORE, BINDER_AI_WORKSPACE,
and optional BINDER_WORKSPACE_PROFILE for bank prefixes / inventory.

License
-------

See LICENSE.txt in this archive. This is proprietary binary distribution,
not open-source software.

Support
-------

See https://github.com/dpkimball/binder-ai-releases
