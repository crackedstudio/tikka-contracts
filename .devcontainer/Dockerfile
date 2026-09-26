# The Rust compiler version MUST match rust-toolchain.toml.  If you bump one,
# bump both — the devcontainer needs to produce byte-identical WASM artifacts
# to CI and to deploy scripts (issue #843).
FROM mcr.microsoft.com/devcontainers/rust:1-1-bullseye

# Install Node.js 20.x
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs shellcheck \
    && rm -rf /var/lib/apt/lists/*

# Pin Rust to the exact version declared in rust-toolchain.toml and install
# the pinned components (rustfmt, clippy) plus the Soroban WASM target.
USER vscode
ARG PINNED_RUST_VERSION=1.94.0
RUN rustup toolchain install "${PINNED_RUST_VERSION}" --profile minimal \
        --component rustfmt \
        --component clippy \
        --target wasm32v1-none \
    && rustup default "${PINNED_RUST_VERSION}" \
    && rustup show

# Install stellar-cli 23.4.1 — pinned so the devcontainer produces the same
# wasm32v1-none artifacts CI and the deploy scripts upload.
RUN cargo install --locked stellar-cli --version "23.4.1" --features opt
