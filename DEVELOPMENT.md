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
cargo build --target wasm32-unknown-unknown --release -p raffle-instance
```
The compiled WASM binary will be located at `target/wasm32-unknown-unknown/release/raffle-instance.wasm`.
The compiled WASM binary will be located at `target/wasm32-unknown-unknown/release/raffle_factory.wasm`.

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

## 📦 Storage Architecture & TTL Management

Soroban uses a **rent-based ledger model** where every piece of on-chain data has a **Time-To-Live (TTL)** measured in ledger sequence numbers. When a key's TTL expires the data is permanently deleted from the ledger — there is no recovery. Operators **must** proactively extend TTLs to prevent data loss.

### Storage Types Used

Tikka contracts use two of Soroban's three storage tiers:

| Storage Type | TTL Behaviour | Used For |
|---|---|---|
| **Instance** | All instance-storage keys share a single TTL tied to the contract's ledger entry. Extending the instance TTL extends every key inside it. | Fast-access operational state that lives and dies with the contract instance. |
| **Persistent** | Each key has its own independent TTL. Keys must be extended individually or in batches. | Long-lived data that must survive independently (tickets, checkpoints, per-user records). |

> [!IMPORTANT]
> Tikka contracts currently contain **no automatic `extend_ttl` calls** in their on-chain logic. TTL management is the **operator's responsibility** and must be performed via external tooling (Stellar CLI, SDK scripts, or a cron job).

### Key-to-Storage Mapping

#### RaffleFactory (`contracts/raffle/src/lib.rs`)

| DataKey | Storage Type | Description |
|---|---|---|
| `Admin` | Persistent | Factory admin address |
| `RaffleInstances` | Persistent | Vec of all deployed raffle instance addresses |
| `InstanceWasmHash` | Persistent | WASM hash used to deploy new raffle instances |
| `ProtocolFeeBP` | Persistent | Protocol fee in basis points |
| `Treasury` | Persistent | Treasury address for fee collection |
| `Paused` | **Instance** | Factory-level pause flag (absent = `false`) |
| `PendingAdmin` | Persistent | Two-step admin transfer target |
| `PendingOp(u32)` | Persistent | Timelocked pending config changes |
| `OpCounter` | Persistent | Monotonic counter for pending ops |
| `Checkpoint(u32)` | Persistent | Periodic state checkpoints |
| `LatestCheckpointIndex` | Persistent | Index of most recent checkpoint |
| `TotalRafflesCreated` | Persistent | Cumulative raffle count |
| `UniqueParticipant(Address)` | Persistent | Per-user participation tracking |
| `TotalUniqueParticipants` | Persistent | Aggregate unique participant count |
| `MinCreationDelay` | Persistent | Rate-limit delay for raffle creation |
| `LastCreationTime(Address)` | Persistent | Per-creator rate-limit timestamp |
| `WhitelistedPartner(Address)` | Persistent | Partner whitelist flags |
| `TotalVolumePerAsset(Address)` | Persistent | Cumulative volume per payment asset |
| `RaffleInstancesCount` | Persistent | Instance counter (test-only) |

#### RaffleInstance (`contracts/raffle-instance/src/lib.rs`)

| DataKey | Storage Type | Description |
|---|---|---|
| `Raffle` | **Instance** | Full raffle state struct (creator, status, winners, etc.) |
| `Factory` | **Instance** | Factory contract address |
| `Admin` | **Instance** | Admin address synced from factory |
| `Paused` | **Instance** | Instance-level pause flag |
| `ReentrancyGuard` | **Instance** | Transient reentrancy lock |
| `RandomnessRequested` | **Instance** | Whether oracle randomness has been requested |
| `RandomnessRequestLedger` | **Instance** | Ledger sequence when randomness was requested |
| `RandomnessSeed` | **Instance** | Fairness metadata after draw |
| `Ticket(u32)` | **Persistent** | Individual ticket data (owner, purchase time) |
| `TicketCount(Address)` | **Persistent** | Per-buyer ticket count |
| `FinishTime` | **Instance** | Raffle finish timestamp |

### Data Expiry Risks

> [!CAUTION]
> If TTLs are not extended, the following **catastrophic failures** can occur:

| Risk | Impact | Affected Keys |
|---|---|---|
| **Factory becomes inoperable** | `init_factory` data (Admin, InstanceWasmHash, Treasury) expires → no new raffles can be created and admin control is lost | All Factory persistent keys |
| **Active raffle data lost** | Instance storage expiry wipes the entire raffle state, including winners, tickets sold, and escrowed prize references | All RaffleInstance instance-storage keys (shared TTL) |
| **Ticket ownership lost** | Individual `Ticket(id)` entries expire → winners cannot be verified, refunds become impossible | `Ticket(u32)` persistent keys |
| **Prize claims blocked** | If raffle instance TTL expires after finalization but before claim, the winner address and claim status are lost | `Raffle` (instance), `Ticket(u32)` (persistent) |
| **Checkpoint history lost** | Historical `Checkpoint(u32)` keys expire → auditability degraded | `Checkpoint(u32)` persistent keys |
| **Participant tracking gaps** | `UniqueParticipant(Address)` expiry causes already-counted users to be counted again | `UniqueParticipant(Address)` persistent keys |

### Recommended TTL Bump Strategy

> [!TIP]
> On the Stellar mainnet, one ledger ≈ 5 seconds. The default minimum TTL for persistent entries is approximately **120 days (~2,073,600 ledgers)**. Plan your bump intervals accordingly.

#### Factory Contract (single instance, long-lived)

```bash
# Extend the factory instance TTL (covers ALL instance-storage keys)
stellar contract extend \
  --id <FACTORY_CONTRACT_ADDRESS> \
  --ledgers-to-extend 6220800 \
  --network <NETWORK> \
  --source-account <OPERATOR_KEY>

# Extend critical persistent keys individually
stellar contract extend \
  --id <FACTORY_CONTRACT_ADDRESS> \
  --key Admin \
  --ledgers-to-extend 6220800 \
  --durability persistent \
  --network <NETWORK> \
  --source-account <OPERATOR_KEY>
```

**Recommended interval**: Extend by **~1 year (6,220,800 ledgers)** and re-run every **6 months** to maintain a comfortable buffer.

**Priority keys to extend** (Factory persistent):
1. `Admin` — loss = permanent lockout
2. `InstanceWasmHash` — loss = cannot deploy new raffles
3. `Treasury` — loss = fee collection breaks
4. `RaffleInstances` — loss = registry of all raffles gone
5. `ProtocolFeeBP` — loss = fee defaults to 0

#### Raffle Instance Contracts (many instances, variable lifetimes)

```bash
# Extend a raffle instance TTL (covers Raffle struct, Admin, Factory, etc.)
stellar contract extend \
  --id <RAFFLE_INSTANCE_ADDRESS> \
  --ledgers-to-extend 3110400 \
  --network <NETWORK> \
  --source-account <OPERATOR_KEY>
```

**Recommended interval**: Extend by **~6 months (3,110,400 ledgers)**. Re-run monthly for any raffle that is still `Active`, `Drawing`, or `Finalized` (i.e., prizes not yet claimed).

> [!WARNING]
> For raffle instances, **persistent ticket data** (`Ticket(u32)`, `TicketCount(Address)`) has **independent TTLs** from the instance storage. You must extend these separately if the raffle is long-running. Failure to do so can make ticket ownership unverifiable even if the raffle instance itself is still alive.

#### Automation Recommendation

Operators should set up a **cron job or scheduled script** that:

1. Queries all active raffle instances from the factory's `RaffleInstances` list
2. Checks the current TTL of each instance and its persistent keys
3. Extends any TTL that is below a threshold (e.g., < 30 days remaining)
4. Logs all extensions for auditability

Example pseudocode:
```bash
#!/bin/bash
# Run weekly via cron: 0 0 * * 0 /path/to/extend_ttls.sh

FACTORY="<FACTORY_CONTRACT_ADDRESS>"
NETWORK="testnet"  # or "mainnet"
MIN_LEDGERS=518400  # ~30 days
EXTEND_BY=3110400   # ~6 months

# 1. Extend factory instance + persistent keys
stellar contract extend --id "$FACTORY" \
  --ledgers-to-extend $EXTEND_BY --network $NETWORK

# 2. For each raffle instance, extend instance storage
for INSTANCE in $(stellar contract invoke --id "$FACTORY" \
  --network $NETWORK -- get_raffles '{"limit":200,"offset":0}' \
  | jq -r '.items[]'); do
    stellar contract extend --id "$INSTANCE" \
      --ledgers-to-extend $EXTEND_BY --network $NETWORK
done
```

### Design Rationale

**Why `Paused` uses instance storage**: The pause flag is accessed on every guarded function call and must share the contract instance's lifecycle. Instance storage avoids per-key TTL management overhead — extending the instance TTL automatically keeps the pause flag alive.

**Why tickets use persistent storage**: Each `Ticket(u32)` is an independent record that may need to outlive the raffle's active phase (e.g., for refunds after cancellation). Persistent storage allows per-ticket TTL management and avoids bloating the instance-storage entry, which would increase costs for every contract invocation.

**Why the factory keeps most keys in persistent storage**: Factory keys like `Admin`, `Treasury`, and `RaffleInstances` are rarely accessed together in a single invocation. Persistent storage is more cost-effective for infrequently co-accessed keys, even though it requires per-key TTL management.
