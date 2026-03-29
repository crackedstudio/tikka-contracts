# Design Document: Randomness Oracle Abstraction

## Overview

The randomness oracle abstraction introduces a thin trait layer between the raffle contract and any external randomness provider. Two Soroban `contractclient` traits are defined:

- `RandomnessOracleTrait` — the interface the raffle calls *out* to when requesting randomness
- `RandomnessReceiverTrait` — the interface an oracle calls *back* into the raffle to deliver a result

A `RandomnessRequest` struct carries all context needed for a round-trip request/response without relying on implicit ledger state.

The existing `provide_randomness` entry-point is preserved for backward compatibility; it shares the same internal winner-selection logic as the new `receive_randomness` callback path.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  Raffle Instance                     │
│                                                      │
│  finalize_raffle()                                   │
│    └─► RandomnessOracleClient::request_randomness()  │──► Oracle Contract
│                                                      │         │
│  receive_randomness() / provide_randomness()  ◄──────│─────────┘
│    └─► internal: do_finalize_with_seed()             │
└─────────────────────────────────────────────────────┘
```

## New File: `contracts/raffle/src/oracle.rs`

```rust
use soroban_sdk::{contractclient, contracttype, Address, Env};

/// Typed request passed to any oracle provider.
#[derive(Clone)]
#[contracttype]
pub struct RandomnessRequest {
    /// The raffle contract address that is requesting randomness.
    pub raffle_id: Address,
    /// Unique identifier for this request (ledger sequence at request time).
    pub request_id: u64,
    /// The address the oracle must call back with the result.
    pub callback_address: Address,
}

/// Interface every oracle provider must implement.
/// Use `RandomnessOracleClient` to call any conforming oracle contract.
#[contractclient(name = "RandomnessOracleClient")]
pub trait RandomnessOracleTrait {
    fn request_randomness(env: Env, request: RandomnessRequest);
}

/// Callback interface the raffle contract exposes to oracle providers.
/// Use `RandomnessReceiverClient` to call back into the raffle.
#[contractclient(name = "RandomnessReceiverClient")]
pub trait RandomnessReceiverTrait {
    fn receive_randomness(env: Env, request_id: u64, random_seed: u64);
}
```

## Changes to `contracts/raffle/src/instance/mod.rs`

### 1. Import oracle types

```rust
use crate::oracle::{RandomnessOracleClient, RandomnessRequest};
```

### 2. `finalize_raffle` — dispatch via trait instead of raw event

Replace the bare `RandomnessRequested` event + early return with an actual typed call:

```rust
if raffle.randomness_source == RandomnessSource::External {
    let oracle = raffle.oracle_address.clone().ok_or(Error::InvalidParameters)?;
    let request = RandomnessRequest {
        raffle_id: env.current_contract_address(),
        request_id: env.ledger().sequence() as u64,
        callback_address: env.current_contract_address(),
    };
    // Store request_id so receive_randomness can validate it
    env.storage().instance().set(&DataKey::PendingRequestId, &request.request_id);

    let oracle_client = RandomnessOracleClient::new(&env, &oracle);
    oracle_client.request_randomness(&request);

    publish_event(&env, "randomness_requested", RandomnessRequested {
        oracle,
        timestamp: env.ledger().timestamp(),
    });
    return Ok(());
}
```

### 3. New `receive_randomness` public function

```rust
pub fn receive_randomness(env: Env, request_id: u64, random_seed: u64) -> Result<(), Error> {
    let raffle = read_raffle(&env)?;
    // Only the registered oracle may call this
    match &raffle.oracle_address {
        Some(oracle) => oracle.require_auth(),
        None => return Err(Error::NotAuthorized),
    }
    // Validate the request_id matches what we stored
    let pending: u64 = env.storage().instance()
        .get(&DataKey::PendingRequestId)
        .ok_or(Error::InvalidStateTransition)?;
    if pending != request_id {
        return Err(Error::InvalidParameters);
    }
    env.storage().instance().remove(&DataKey::PendingRequestId);
    do_finalize_with_seed(&env, random_seed)
}
```

### 4. `provide_randomness` delegates to shared logic

```rust
pub fn provide_randomness(env: Env, random_seed: u64) -> Result<Address, Error> {
    let raffle = read_raffle(&env)?;
    match &raffle.oracle_address {
        Some(oracle) => oracle.require_auth(),
        None => return Err(Error::NotAuthorized),
    }
    if raffle.status != RaffleStatus::Drawing
        || raffle.randomness_source != RandomnessSource::External
    {
        return Err(Error::InvalidStateTransition);
    }
    do_finalize_with_seed(&env, random_seed)?;
    Ok(read_raffle(&env)?.winners.get(0).unwrap())
}
```

### 5. New `DataKey` variant

```rust
PendingRequestId,  // u64 — stored during External randomness request
```

## Data Models

### `RandomnessRequest`

| Field | Type | Description |
|---|---|---|
| `raffle_id` | `Address` | Requesting contract (for oracle routing) |
| `request_id` | `u64` | Ledger sequence at request time |
| `callback_address` | `Address` | Where oracle delivers the result |

### Storage

- `DataKey::PendingRequestId` — instance storage, set when request is dispatched, cleared on receipt. Prevents replayed or mismatched callbacks.

## Backward Compatibility

- `provide_randomness(env, seed)` stays on the public ABI unchanged
- `oracle_address: Option<Address>` on `Raffle` / `RaffleConfig` unchanged
- `RandomnessSource::External` enum variant unchanged
- All existing tests pass — they call `provide_randomness` directly, bypassing the new dispatch path
