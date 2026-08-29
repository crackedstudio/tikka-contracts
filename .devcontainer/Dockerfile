# Tikka Contracts Development Container
#
# Toolchain strategy: only `rustup` is installed at the image level; the
# pinned Rust version, components, and targets are read **transparently**
# from the repo's `rust-toolchain.toml` the first time cargo/rustc is invoked
# (rustup auto-provisions the matching toolchain on first use).
#
# This means a Rust version upgrade requires a ONE-FILE change to
# rust-toolchain.toml (plus the docs/DEPLOYMENT.md checklist) — this
# Dockerfile does NOT need to be re-pinned.  Do NOT add `rustup install
# 1.XX.0` or any version-specific install step to this file.

FROM mcr.microsoft.com/devcontainers/base:ubuntu-22.04

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        git \
        libssl-dev \
        pkg-config \
        shellcheck \
        nodejs \
        npm \
        make \
    && rm -rf /var/lib/apt/lists/*

# Install rustup only.  The exact Rust toolchain (channel, components clippy
# & rustfmt, target wasm32v1-none) comes from rust-toolchain.toml at repo
# root and is provisioned on first `cargo` invocation via rustup's
# toolchain-file auto-detection.
RUN set -eux; \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
        sh -s -- -y --default-toolchain none --profile minimal; \
    echo 'source $HOME/.cargo/env' >> /root/.bashrc; \
    echo 'source $HOME/.cargo/env' >> /etc/bash.bashrc;

ENV PATH="/root/.cargo/bin:${PATH}"
ENV CARGO_TERM_COLOR=always

# Warm up: explicitly trigger the rustup toolchain provisioning against the
# checked-in rust-toolchain.toml so the first `cargo build` inside the
# running container is fast.  We intentionally do NOT bake a specific
# version into the image — rustup reads ./rust-toolchain.toml here.
WORKDIR /workspace
COPY rust-toolchain.toml ./
RUN rustup show active-toolchain 2>&1 | head -5 || true

# Node/npm deps for the oracle service (best-effort).
COPY oracle/package.json ./oracle/
RUN if [ -f oracle/package.json ]; then cd oracle && npm install || true; fi
