# Tikka Development Guide

Welcome to the `tikka-contracts` development guide! This document covers setting up your local environment, building, testing, deploying, and verifying the Soroban smart contracts.

## 🛠 Prerequisites

1.  **Rust Toolchain**: Install via [rustup](https://rustup.rs/).
2.  **WebAssembly Target**: Add the `wasm32-unknown-unknown` target:
    ```bash
    rustup target add wasm32-unknown-unknown
    ```
3.  **Stellar CLI**: Install the official Stellar CLI:
    ```bash
    cargo install --locked stellar-cli --features opt
    ```

## 🏗 Build & Test

### Build the Contract
To compile the Soroban contract into WebAssembly (`.wasm`):
```bash
cargo build --target wasm32-unknown-unknown --release
```
The compiled WASM binary will be located at `target/wasm32-unknown-unknown/release/hello_world.wasm`.

### Run Unit Tests
To execute the contract's standard Rust unit tests:
```bash
cargo test
```

## 🚀 Deployment

The project provides automated shell scripts in the `scripts/` directory to facilitate deployment and interactions.

### 1. Environment Configuration
Copy the `.env.example` file to create your own local `.env`:
```bash
cp .env.example .env
```
Fill out the required variables including `DEPLOYER_SECRET_KEY` and `RAFFLE_CONTRACT_ADDRESS`.

### 2. Fund Your Testnet Account
Ensure your deployer account has Testnet Lumens (XLM):
```bash
./scripts/fund-testnet.sh <YOUR_PUBLIC_KEY>
```

### 3. Deploy to Testnet
Deploy the compiled WASM binary directly to the Stellar testnet:
```bash
./scripts/deploy-testnet.sh
```
This script automatically compiles the contract (if needed), deploys it using the `DEPLOYER_SECRET_KEY` from your `.env`, and outputs the resulting `C...` contract address.

## 🔎 Verifying Deployed Contracts

It is essential to verify that the deployed contract logic matches your local build to ensure trust.

```bash
./scripts/verify.sh
```
This script downloads the remote WASM bytecode associated with `RAFFLE_CONTRACT_ADDRESS` on the active `STELLAR_NETWORK` and compares its SHA256 checksum to your locally compiled binary. It outputs `Match: YES` to guarantee parity.

## 🕹 Invoking Contract Functions

Use the `invoke.sh` script to interact with your deployed contract smoothly:
```bash
./scripts/invoke.sh <function_name> [args...]
# Example:
./scripts/invoke.sh buy_ticket --amount 10
```
