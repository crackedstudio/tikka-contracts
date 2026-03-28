# Design Document: Periodic State Snapshots (Sync Checkpoints)

## Overview

Every 1,000 raffles created by the `RaffleFactory`, the contract automatically computes and persists a `StateCheckpoint`. Each checkpoint captures the raffle count, ledger sequence, ledger timestamp, and a SHA-256 aggregate hash of those fields. Off-chain indexers can use these checkpoints to bootstrap from a recent known-good state rather than replaying the full chain history.

The feature is entirely additive: it hooks into the existing `create_raffle` path with a single post-creation check, adds new `DataKey` variants and a `StateCheckpoint` struct, and exposes two read-only query functions. No existing behaviour changes.

---

## Architecture

```mermaid
graph TD
    Caller -->|create_raffle| RaffleFactory
    RaffleFactory -->|raffle count % 1000 == 0?| CheckpointLogic
    CheckpointLogic -->|compute SHA-256| env.crypto.sha256
    CheckpointLogic -->|persist| PersistentStorage
    CheckpointLogic -->|emit event| EventStream

    PersistentStorage --> CP["DataKey::Checkpoint(index) → StateCheckpoint"]
    PersistentStorage --> LI["DataKey::LatestCheckpointIndex → u32"]

    Indexer -->|get_checkpoint(index)| RaffleFactory
    Indexer -->|get_latest_checkpoint_index()| RaffleFactory
    Indexer -->|watch events| EventStream
```

The checkpoint logic lives entirely inside `lib.rs` as a private helper function `maybe_create_checkpoint`, called at the end of `create_raffle` after the raffle instance has been appended to `RaffleInstances`. This keeps the existing function signature and return value unchanged.

---

## Components and Interfaces

### New `DataKey` variants (in `lib.rs`)

```rust
pub enum DataKey {
    // ... existing variants ...
    Checkpoint(u32),          // keyed by checkpoint index (1-based)
    LatestCheckpointIndex,    // u32, defaults to 0 when absent
}
```

### New `StateCheckpoint` struct (in `lib.rs`)

```rust
#[derive(Clone)]
#[contracttype]
pub struct StateCheckpoint {
    pub index: u32,
    pub raffle_count: u32,
    pub ledger_timestamp: u64,
    pub aggregate_hash: soroban_sdk::BytesN<32>,
}
```

`BytesN<32>` is the fixed-length type returned by `env.crypto().sha256()` in the Soroban SDK, so no conversion is needed.

### New event struct (in `events.rs`)

```rust
#[derive(Clone)]
#[contracttype]
pub struct CheckpointCreated {
    pub index: u32,
    pub raffle_count: u32,
    pub ledger_timestamp: u64,
    pub aggregate_hash: soroban_sdk::BytesN<32>,
}
```

### Private helper: `maybe_create_checkpoint` (in `lib.rs`)

```rust
const CHECKPOINT_INTERVAL: u32 = 1_000;

fn maybe_create_checkpoint(env: &Env, raffle_count: u32) {
    if raffle_count == 0 || raffle_count % CHECKPOINT_INTERVAL != 0 {
        return;
    }

    let index = raffle_count / CHECKPOINT_INTERVAL;
    let ledger_timestamp = env.ledger().timestamp();
    let ledger_sequence = env.ledger().sequence();

    // Serialise inputs: raffle_count (u32 BE) || ledger_sequence (u32 BE) || ledger_timestamp (u64 BE)
    let mut input = soroban_sdk::Bytes::new(env);
    input.extend_from_array(&raffle_count.to_be_bytes());
    input.extend_from_array(&ledger_sequence.to_be_bytes());
    input.extend_from_array(&ledger_timestamp.to_be_bytes());

    let aggregate_hash = env.crypto().sha256(&input);

    let checkpoint = StateCheckpoint {
        index,
        raffle_count,
        ledger_timestamp,
        aggregate_hash: aggregate_hash.clone(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::Checkpoint(index), &checkpoint);
    env.storage()
        .persistent()
        .set(&DataKey::LatestCheckpointIndex, &index);

    publish_factory_event(
        env,
        "checkpoint_created",
        crate::events::CheckpointCreated {
            index,
            raffle_count,
            ledger_timestamp,
            aggregate_hash,
        },
    );
}
```

### Modified: `create_raffle` (in `lib.rs`)

After `instances.push_back(creator.clone())` and the storage write, add:

```rust
let raffle_count = instances.len();
maybe_create_checkpoint(&env, raffle_count);
```

No changes to the function signature or return type.

### New public query functions (in `lib.rs`)

```rust
pub fn get_checkpoint(env: Env, index: u32) -> Option<StateCheckpoint> {
    env.storage()
        .persistent()
        .get(&DataKey::Checkpoint(index))
}

pub fn get_latest_checkpoint_index(env: Env) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::LatestCheckpointIndex)
        .unwrap_or(0u32)
}
```

Both functions require no authorisation.

---

## Data Models

### `StateCheckpoint`

| Field | Type | Description |
|---|---|---|
| `index` | `u32` | Monotonically increasing checkpoint number (1, 2, 3, …) |
| `raffle_count` | `u32` | Total raffles created at checkpoint time |
| `ledger_timestamp` | `u64` | `env.ledger().timestamp()` at checkpoint time |
| `aggregate_hash` | `BytesN<32>` | SHA-256 of `raffle_count ‖ ledger_sequence ‖ ledger_timestamp` |

### Storage layout

| Key | Type | Storage tier | Description |
|---|---|---|---|
| `DataKey::Checkpoint(u32)` | `StateCheckpoint` | Persistent | One entry per checkpoint index |
| `DataKey::LatestCheckpointIndex` | `u32` | Persistent | Pointer to the most recent checkpoint |

### Hash input serialisation

The hash input is the concatenation of three big-endian byte arrays:

```
raffle_count      (4 bytes, u32 BE)
ledger_sequence   (4 bytes, u32 BE)
ledger_timestamp  (8 bytes, u64 BE)
```

Total: 16 bytes. This is deterministic and reproducible off-chain.

---

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system — essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: Checkpoint created if and only if raffle count is a multiple of 1,000

*For any* raffle count `n`, a `StateCheckpoint` is created and persisted if and only if `n % 1_000 == 0`. For counts that are not multiples of 1,000, `get_latest_checkpoint_index` must remain unchanged after `create_raffle` returns.

**Validates: Requirements 1.1, 1.4, 1.5**

### Property 2: Checkpoint fields are internally consistent

*For any* `StateCheckpoint` stored at index `i`, the following invariants must all hold simultaneously:
- `checkpoint.index == i`
- `checkpoint.raffle_count == i * 1_000`
- `checkpoint.index == checkpoint.raffle_count / 1_000`
- `checkpoint.ledger_timestamp` equals the ledger timestamp at the time `create_raffle` was called for the `i * 1_000`-th raffle

**Validates: Requirements 1.2, 1.3, 7.1, 7.2**

### Property 3: Aggregate hash matches recomputed SHA-256

*For any* `StateCheckpoint`, recomputing `SHA-256(raffle_count_BE4 ‖ ledger_sequence_BE4 ‖ ledger_timestamp_BE8)` using the values recorded in the checkpoint must produce a byte sequence equal to `checkpoint.aggregate_hash`.

**Validates: Requirements 2.1, 2.2, 2.3**

### Property 4: Latest checkpoint index pointer is always current

*For any* sequence of `create_raffle` calls, after each call `get_latest_checkpoint_index()` must return the index of the most recently created checkpoint. If no checkpoint has been created, it must return `0`.

**Validates: Requirements 3.2, 4.2**

### Property 5: No gaps in the checkpoint sequence

*For any* latest checkpoint index `L` returned by `get_latest_checkpoint_index()`, every index `i` in `[1, L]` must return `Some(checkpoint)` from `get_checkpoint(i)`. There must be no gaps.

**Validates: Requirements 3.1, 4.1, 7.3**

### Property 6: Non-existent index returns None

*For any* index `i` greater than the current latest checkpoint index, `get_checkpoint(i)` must return `None`.

**Validates: Requirements 4.4**

### Property 7: Checkpoint event data matches stored checkpoint

*For any* checkpoint creation, the `checkpoint_created` event emitted under the `("tikka", "checkpoint_created")` topic pair must contain `index`, `raffle_count`, `ledger_timestamp`, and `aggregate_hash` values that are identical to those stored in the corresponding `StateCheckpoint`.

**Validates: Requirements 5.1, 5.2**

---

## Error Handling

This feature introduces no new error variants. All failure modes are handled by the existing `ContractError` enum:

| Scenario | Behaviour |
|---|---|
| `create_raffle` called on a paused factory | Returns `ContractError::ContractPaused` (existing guard, unchanged) |
| `get_checkpoint` called with a non-existent index | Returns `None` (not an error) |
| `get_latest_checkpoint_index` called before any checkpoint | Returns `0` (not an error) |

The `maybe_create_checkpoint` helper is infallible by design. SHA-256 computation via `env.crypto().sha256()` does not return a `Result` in the Soroban SDK — it panics only on internal SDK errors, which are outside the contract's control. Persistent storage writes are also infallible within a Soroban transaction.

---

## Testing Strategy

### Dual testing approach

Both unit tests and property-based tests are required. Unit tests cover specific examples and edge cases; property tests verify universal invariants across many generated inputs.

### Unit tests (specific examples and edge cases)

Located in `contracts/raffle/src/lib.rs` under `#[cfg(test)]`, following the existing test style:

| Test | Validates |
|---|---|
| `test_no_checkpoint_before_first_milestone` — call `create_raffle` 999 times, assert `get_latest_checkpoint_index() == 0` | Req 1.4, 3.3 |
| `test_checkpoint_created_at_1000` — call `create_raffle` 1,000 times, assert checkpoint exists at index 1 with correct fields | Req 1.1, 1.2 |
| `test_checkpoint_fields_correct` — verify `index`, `raffle_count`, `ledger_timestamp`, `aggregate_hash` on a freshly created checkpoint | Req 1.2, 2.1, 7.1, 7.2 |
| `test_get_checkpoint_returns_none_for_missing_index` — call `get_checkpoint(999)` on a fresh factory, assert `None` | Req 4.4 |
| `test_get_latest_checkpoint_index_initial_zero` — fresh factory, assert `get_latest_checkpoint_index() == 0` | Req 3.3 |
| `test_query_functions_require_no_auth` — call both query functions without `mock_all_auths`, assert they succeed | Req 4.3 |
| `test_paused_factory_rejects_create_raffle_at_milestone` — pause factory, attempt 1,000th raffle, assert `ContractPaused` | Req 6.3 |
| `test_checkpoint_event_emitted` — trigger a checkpoint, inspect `env.events().all()` for `checkpoint_created` topic and correct payload | Req 5.1, 5.2 |
| `test_two_checkpoints_sequential` — create 2,000 raffles, assert checkpoints at index 1 and 2 both exist with correct fields | Req 7.3 |

### Property-based tests

Use the [`proptest`](https://github.com/proptest-rs/proptest) crate (add `proptest = "1"` under `[dev-dependencies]` in `contracts/raffle/Cargo.toml`). Each test runs a minimum of 100 iterations.

```toml
[dev-dependencies]
proptest = "1"
```

Each property test must carry a comment referencing the design property it validates, using the format:
`// Feature: periodic-state-snapshots, Property N: <property_text>`

| Property test | Design property |
|---|---|
| For any `n` in `[1, 10]`, create `n * 1_000` raffles and assert a checkpoint exists at index `n` with `raffle_count == n * 1_000` | Property 1, 2 |
| For any `n` in `[1, 5]`, create `n * 1_000` raffles and recompute the hash; assert it matches `checkpoint.aggregate_hash` | Property 3 |
| For any `n` in `[1, 5]`, create `n * 1_000` raffles and assert `get_latest_checkpoint_index() == n` | Property 4 |
| For any `n` in `[1, 5]`, create `n * 1_000` raffles and assert every index in `[1, n]` returns `Some` from `get_checkpoint` | Property 5 |
| For any `n` in `[1, 5]` and `m > n`, create `n * 1_000` raffles and assert `get_checkpoint(m)` returns `None` | Property 6 |
| For any `n` in `[1, 5]`, create `n * 1_000` raffles and assert the last `checkpoint_created` event payload matches the stored checkpoint | Property 7 |

> Note: Because Soroban's test environment does not support true randomness injection for ledger fields, property tests parameterise over the number of checkpoints `n` rather than raw raffle counts. The Soroban `Env` in tests uses a fixed ledger sequence/timestamp unless explicitly advanced with `env.ledger().set(...)`, so tests that need distinct timestamps should advance the ledger between batches.
