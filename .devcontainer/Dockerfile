# =============================================================================
# Tikka Contracts — Dev Container
#
# Installs the exact pinned Rust toolchain from rust-toolchain.toml so that
# devcontainer users get the same compiler as CI.
#
# Pinned versions are recorded here for auditability. Update them together
# with rust-toolchain.toml and docs/DEPLOYMENT.md.
# =============================================================================

FROM mcr.microsoft.com/devcontainers/base:ubuntu-22.04

# ---------------------------------------------------------------------------
# System dependencies
# ---------------------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    git \
    pkg-config \
    libssl-dev \
    shellcheck \
    && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# Node.js (for oracle TypeScript tooling and Prettier)
# ---------------------------------------------------------------------------
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# Rust — install rustup as the vscode user then pin the toolchain
#
# The toolchain version MUST match rust-toolchain.toml in the repo root.
# Change both places together.
# ---------------------------------------------------------------------------
ARG RUST_VERSION=1.85.0

USER vscode
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain none \
    && . "$HOME/.cargo/env" \
    && rustup toolchain install ${RUST_VERSION} \
         --component clippy rustfmt \
         --target wasm32v1-none \
    && rustup default ${RUST_VERSION}

ENV PATH="/home/vscode/.cargo/bin:${PATH}"

# ---------------------------------------------------------------------------
# Stellar CLI (latest release at image build time)
# Keep this pinned to a specific version in production images.
# ---------------------------------------------------------------------------
RUN curl -sSL \
    https://github.com/stellar/stellar-cli/releases/latest/download/stellar-cli-linux-amd64.tar.gz \
    | tar -xz -C /tmp \
    && sudo mv /tmp/stellar /usr/local/bin/stellar \
    && stellar --version

# ---------------------------------------------------------------------------
# pre-commit
# ---------------------------------------------------------------------------
RUN pip3 install --user pre-commit

WORKDIR /workspace
