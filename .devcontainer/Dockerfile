FROM mcr.microsoft.com/devcontainers/rust:1-1-bookworm

# Install Node.js 20.x
RUN apt-get update && apt-get install -y libdbus-1-dev libudev-dev pkg-config && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Add wasm32 target for Rust
USER vscode
RUN rustup target add wasm32-unknown-unknown

# Install stellar-cli 23.x
RUN cargo install --locked stellar-cli --version "~23.0"
