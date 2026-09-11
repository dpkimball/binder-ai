# syntax=docker/dockerfile:1.6
#
# Plan service: binder-ai-serve + codeintel CLI. Store is mounted at /data.
# codeintel/bindb come from named BuildKit contexts (siblings in CI / make b).
# Rebuild this image whenever codeintel gains CLI surface the chart relies on
# (e.g. `architecture doc-compliance` for the store-refresh sidecar).
# binder-llm comes from Kellnr (registry dpk), same as binder-server.
# Index: CARGO_REGISTRIES_DPK_INDEX (env.sh locally; PRIVATE_CARGO_URL in CI).
# Local docker: dpk.toml extra_args uses host.docker.internal (DEV Kellnr).

ARG CARGO_REGISTRIES_DPK_INDEX

FROM rust:1-slim-bookworm AS codeintel-build
WORKDIR /src
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    pkg-config \
  && rm -rf /var/lib/apt/lists/*
COPY --from=bindb Cargo.toml Cargo.lock README.md ./bindb/
COPY --from=bindb src ./bindb/src
COPY --from=bindb benches ./bindb/benches
COPY --from=codeintel Cargo.toml Cargo.lock README.md build.rs ./codeintel/
COPY --from=codeintel src ./codeintel/src
COPY --from=codeintel profiles ./codeintel/profiles
WORKDIR /src/codeintel
# build.rs injects CODEINTEL_* via rustc-env; no .git in build context.
ENV GITHUB_SHA=docker
RUN cargo install --locked --path . --root /out
RUN strip /out/bin/codeintel || true

FROM rust:1-slim-bookworm AS serve-build
ARG CARGO_REGISTRIES_DPK_INDEX
ENV CARGO_REGISTRIES_DPK_INDEX=$CARGO_REGISTRIES_DPK_INDEX
WORKDIR /src/binder-ai
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    cmake \
    git \
    libssl-dev \
    zlib1g-dev \
    pkg-config \
  && rm -rf /var/lib/apt/lists/*
COPY .cargo/config.toml /usr/local/cargo/config.toml
COPY Cargo.toml Cargo.lock README.md build.rs ./
COPY src ./src
RUN cargo install --locked --path . --bin binder-ai-serve --root /out --features intel
RUN strip /out/bin/binder-ai-serve || true

FROM rust:1-slim-bookworm AS implement-build
ARG CARGO_REGISTRIES_DPK_INDEX
ENV CARGO_REGISTRIES_DPK_INDEX=$CARGO_REGISTRIES_DPK_INDEX
WORKDIR /src/binder-ai
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    cmake \
    git \
    libssl-dev \
    pkg-config \
  && rm -rf /var/lib/apt/lists/*
COPY .cargo/config.toml /usr/local/cargo/config.toml
COPY Cargo.toml Cargo.lock README.md build.rs ./
RUN sed -i 's|http://127\.0\.0\.1:31127|http://host.docker.internal:31127|g' Cargo.lock
COPY src ./src
RUN cargo install --locked --path . --bin binder-ai-implement --root /out \
  && strip /out/bin/binder-ai-implement || true

# implement-artifacts: minimal image for GHCR (foreman copies the binary from here)
FROM debian:bookworm-slim AS implement-artifacts
COPY --from=implement-build /out/bin/binder-ai-implement /out/bin/binder-ai-implement

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl git \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 65532 --create-home --home-dir /nonexistent --shell /usr/sbin/nologin serve \
  && mkdir -p /data \
  && chown 65532:65532 /data
COPY --from=codeintel-build /out/bin/codeintel /usr/local/bin/codeintel
COPY --from=serve-build /out/bin/binder-ai-serve /usr/local/bin/binder-ai-serve
ENV BINDER_AI_CODEINTEL_STORE=/data
ENV CODEINTEL_BIN=/usr/local/bin/codeintel
ENV BINDER_AI_LISTEN=0.0.0.0:7841
EXPOSE 7841
USER 65532:65532
WORKDIR /data
ENTRYPOINT ["binder-ai-serve"]
CMD ["--store", "/data", "--listen", "0.0.0.0:7841", "--codeintel", "/usr/local/bin/codeintel"]
