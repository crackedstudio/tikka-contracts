# Deployment Guide

This document covers deploying Tikka contracts to Stellar networks and records
the toolchain and WASM sizes for every deployment.

---

## Prerequisites

- Rust toolchain pinned in `rust-toolchain.toml` (installed automatically by `rustup`)
- Stellar CLI — install:
  ```bash
  cargo install --locked stellar-cli --features opt
  ```
- A funded Stellar account (testnet faucet: `stellar keys generate --network testnet`)

---

## Build

Always build with the pinned toolchain to get reproducible WASM output:

```bash
make build
```

Record the WASM sizes printed by `make build` in the [deployment log](#deployment-log) below.

---

## Environment configuration

Copy `.env.example` to `.env` and fill in:

```
DEPLOYER_SECRET_KEY=S...
RAFFLE_CONTRACT_ADDRESS=C...   # set after first deploy
STELLAR_NETWORK=testnet        # or mainnet
```

---

## Deploy to testnet

```bash
./scripts/deploy-testnet.sh
```

## Deploy to mainnet

```bash
./scripts/deploy-mainnet.sh
```

> **Important:** Run `./scripts/verify.sh` after every deployment to confirm
> the on-chain WASM hash matches your local build.

---

## Upgrading a deployed contract

1. Build the new WASM: `make build`
2. Record the new WASM size below.
3. Upload the new WASM hash via `stellar contract install`.
4. Invoke the factory's `set_wasm_hash` to point instances at the new code.
5. Update the deployment log.

---

## Toolchain history

> Every time `rust-toolchain.toml` is updated, add a row here.
> WASM size changes must be noted — unexpected size increases (> 5 kB) warrant investigation.

| Date | Rust version | `raffle-instance.wasm` | `raffle_factory.wasm` | Notes |
|---|---|---|---|---|
| 2026-09-02 | 1.85.0 (initial pin) | — | — | Toolchain pinned; sizes to be recorded on next build |

---

## Deployment log

> Record every mainnet and testnet deployment here.

| Date | Network | Contract | WASM hash (first 16 hex chars) | Deployer account | Notes |
|---|---|---|---|---|---|
| — | testnet | raffle-factory | — | — | Initial testnet deploy |

---

## WASM size policy

- Binaries **must** stay under 200 kB (`make size-check` enforces this).
- Any increase > 5 kB must be explained in the PR description.
- Record pre- and post-upgrade sizes in the deployment log above.

---

## Verifying a deployment

```bash
./scripts/verify.sh
```

This script downloads the on-chain WASM for `RAFFLE_CONTRACT_ADDRESS`, computes its SHA-256,
and compares it to your local build. Output `Match: YES` confirms parity.
