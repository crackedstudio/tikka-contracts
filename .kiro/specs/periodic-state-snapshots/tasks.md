# Implementation Plan: Periodic State Snapshots (Sync Checkpoints)

## Overview

Implement automatic `StateCheckpoint` creation every 1,000 raffles in the `RaffleFactory` contract. The feature hooks into the existing `create_raffle` path, adds new `DataKey` variants and a `StateCheckpoint` struct, and exposes two read-only query functions. All changes are additive — no existing behaviour is modified.

## Tasks

- [x] 1. Add `StateCheckpoint` struct and new `DataKey` variants
  - Add `Checkpoint(u32)` and `LatestCheckpointIndex` variants to the `DataKey` enum in `contracts/raffle/src/lib.rs`
  - Add the `StateCheckpoint` struct annotated with `#[contracttype]` and `#[derive(Clone)]` in `contracts/raffle/src/lib.rs`
  - Fields: `index: u32`, `raffle_count: u32`, `ledger_timestamp: u64`, `aggregate_hash: soroban_sdk::BytesN<32>`
  - _Requirements: 1.2, 2.3, 3.1, 3.2_

- [x] 2. Add `CheckpointCreated` event struct
  - Add `CheckpointCreated` struct to `contracts/raffle/src/events.rs` with fields matching `StateCheckpoint` (`index`, `raffle_count`, `ledger_timestamp`, `aggregate_hash`)
  - Annotate with `#[contracttype]` and `#[derive(Clone)]`
  - _Requirements: 5.1, 5.2_

- [x] 3. Implement `maybe_create_checkpoint` helper and wire into `create_raffle`
  - Add `const CHECKPOINT_INTERVAL: u32 = 1_000;` at module level in `contracts/raffle/src/lib.rs`
  - Implement private `fn maybe_create_checkpoint(env: &Env, raffle_count: u32)` that:
    - Returns early if `raffle_count == 0 || raffle_count % CHECKPOINT_INTERVAL != 0`
    - Computes `index = raffle_count / CHECKPOINT_INTERVAL`
    - Reads `env.ledger().timestamp()` and `env.ledger().sequence()`
    - Serialises `raffle_count` (u32 BE, 4 bytes) ‖ `ledger_sequence` (u32 BE, 4 bytes) ‖ `ledger_timestamp` (u64 BE, 8 bytes) into a `soroban_sdk::Bytes`
    - Calls `env.crypto().sha256(&input)` to produce `aggregate_hash: BytesN<32>`
    - Constructs and persists `StateCheckpoint` under `DataKey::Checkpoint(index)`
    - Persists `index` under `DataKey::LatestCheckpointIndex`
    - Emits `CheckpointCreated` event via `publish_factory_event`
  - In `create_raffle`, after `env.storage().persistent().set(&DataKey::RaffleInstances, &instances)`, add: `let raffle_count = instances.len(); maybe_create_checkpoint(&env, raffle_count);`
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 2.1, 2.2, 2.3, 3.1, 3.2, 5.1, 5.2, 6.1, 6.2_

  - [ ]* 3.1 Write property test for checkpoint creation trigger (Property 1)
    - `// Feature: periodic-state-snapshots, Property 1: Checkpoint created if and only if raffle count is a multiple of 1,000`
    - For `n` in `[1, 10]`, create `n * 1_000` raffles and assert `get_checkpoint(n)` is `Some` and `get_latest_checkpoint_index() == n`
    - Also assert that after `n * 1_000 - 1` raffles, `get_latest_checkpoint_index() == n - 1`
    - _Requirements: 1.1, 1.4, 1.5_

  - [ ]* 3.2 Write property test for checkpoint field consistency (Property 2)
    - `// Feature: periodic-state-snapshots, Property 2: Checkpoint fields are internally consistent`
    - For `n` in `[1, 5]`, create `n * 1_000` raffles; for each checkpoint at index `i` assert `checkpoint.index == i`, `checkpoint.raffle_count == i * 1_000`, `checkpoint.index == checkpoint.raffle_count / 1_000`
    - _Requirements: 1.2, 1.3, 7.1, 7.2_

  - [ ]* 3.3 Write property test for aggregate hash correctness (Property 3)
    - `// Feature: periodic-state-snapshots, Property 3: Aggregate hash matches recomputed SHA-256`
    - For `n` in `[1, 5]`, create `n * 1_000` raffles; recompute `SHA-256(raffle_count_BE4 ‖ ledger_sequence_BE4 ‖ ledger_timestamp_BE8)` using stored checkpoint fields and assert it equals `checkpoint.aggregate_hash`
    - _Requirements: 2.1, 2.2, 2.3_

- [x] 4. Implement `get_checkpoint` and `get_latest_checkpoint_index` query functions
  - Add `pub fn get_checkpoint(env: Env, index: u32) -> Option<StateCheckpoint>` to `RaffleFactory` in `contracts/raffle/src/lib.rs`; reads from `DataKey::Checkpoint(index)` with no auth requirement
  - Add `pub fn get_latest_checkpoint_index(env: Env) -> u32` to `RaffleFactory`; reads `DataKey::LatestCheckpointIndex` with `.unwrap_or(0u32)` and no auth requirement
  - _Requirements: 3.2, 3.3, 4.1, 4.2, 4.3, 4.4_

  - [ ]* 4.1 Write property test for latest checkpoint index pointer (Property 4)
    - `// Feature: periodic-state-snapshots, Property 4: Latest checkpoint index pointer is always current`
    - For `n` in `[1, 5]`, create `n * 1_000` raffles and assert `get_latest_checkpoint_index() == n` after each batch
    - Also assert `get_latest_checkpoint_index() == 0` on a fresh factory
    - _Requirements: 3.2, 4.2_

  - [ ]* 4.2 Write property test for no gaps in checkpoint sequence (Property 5)
    - `// Feature: periodic-state-snapshots, Property 5: No gaps in the checkpoint sequence`
    - For `n` in `[1, 5]`, create `n * 1_000` raffles and assert every index `i` in `[1, n]` returns `Some` from `get_checkpoint(i)`
    - _Requirements: 3.1, 4.1, 7.3_

  - [ ]* 4.3 Write property test for non-existent index returns None (Property 6)
    - `// Feature: periodic-state-snapshots, Property 6: Non-existent index returns None`
    - For `n` in `[1, 5]` and `m = n + 1`, create `n * 1_000` raffles and assert `get_checkpoint(m)` returns `None`
    - _Requirements: 4.4_

- [x] 5. Add `proptest` dev-dependency and write unit tests
  - Add `proptest = "1"` under `[dev-dependencies]` in `contracts/raffle/Cargo.toml`
  - In the `#[cfg(test)]` module in `contracts/raffle/src/lib.rs`, add the following unit tests:
    - `test_no_checkpoint_before_first_milestone` — create 999 raffles, assert `get_latest_checkpoint_index() == 0` (_Req 1.4, 3.3_)
    - `test_checkpoint_created_at_1000` — create 1,000 raffles, assert `get_checkpoint(1)` is `Some` (_Req 1.1, 1.2_)
    - `test_checkpoint_fields_correct` — verify `index`, `raffle_count`, `ledger_timestamp`, `aggregate_hash` on checkpoint at index 1 (_Req 1.2, 2.1, 7.1, 7.2_)
    - `test_get_checkpoint_returns_none_for_missing_index` — fresh factory, assert `get_checkpoint(999)` is `None` (_Req 4.4_)
    - `test_get_latest_checkpoint_index_initial_zero` — fresh factory, assert `get_latest_checkpoint_index() == 0` (_Req 3.3_)
    - `test_query_functions_require_no_auth` — call both query functions without `mock_all_auths`, assert they succeed (_Req 4.3_)
    - `test_paused_factory_rejects_create_raffle_at_milestone` — pause factory, attempt 1,000th raffle, assert `ContractPaused` (_Req 6.3_)
    - `test_checkpoint_event_emitted` — trigger a checkpoint, inspect `env.events().all()` for `("tikka", "checkpoint_created")` topic and correct payload (_Req 5.1, 5.2_)
    - `test_two_checkpoints_sequential` — create 2,000 raffles, assert checkpoints at index 1 and 2 both exist with correct fields (_Req 7.3_)
  - _Requirements: 1.1, 1.4, 2.1, 3.3, 4.3, 4.4, 5.1, 5.2, 6.3, 7.3_

  - [ ]* 5.1 Write property test for checkpoint event data matching stored checkpoint (Property 7)
    - `// Feature: periodic-state-snapshots, Property 7: Checkpoint event data matches stored checkpoint`
    - For `n` in `[1, 5]`, create `n * 1_000` raffles; retrieve the last `checkpoint_created` event from `env.events().all()` and assert its `index`, `raffle_count`, `ledger_timestamp`, and `aggregate_hash` match the stored `StateCheckpoint`
    - _Requirements: 5.1, 5.2_

- [x] 6. Checkpoint — Ensure all tests pass
  - Run `cargo test` in `contracts/raffle/` and confirm all unit and property tests pass. Ask the user if any questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Each task references specific requirements for traceability
- Property tests parameterise over the number of checkpoints `n` rather than raw raffle counts, because the Soroban test `Env` uses a fixed ledger sequence/timestamp unless explicitly advanced with `env.ledger().set(...)`
- Tests that need distinct timestamps per checkpoint batch should advance the ledger between batches
- `proptest` macros must be wrapped in a `proptest! { ... }` block; import with `use proptest::prelude::*;`
