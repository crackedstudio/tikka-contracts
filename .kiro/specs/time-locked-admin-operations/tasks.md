# Implementation Plan: Time-Locked Admin Operations

## Overview

Replace the immediate `set_config` entry point with a propose → wait → execute lifecycle. Changes are queued as `PendingOp` entries with a 48-hour timelock before they can be applied. Implementation touches `lib.rs`, `events.rs`, and the test file.

## Tasks

- [x] 1. Add types, constants, and storage keys to `lib.rs`
  - Add `pub const TIMELOCK_DELAY_SECONDS: u64 = 172800;` at the top of `lib.rs`
  - Add `AdminOp` enum and `PendingOp` struct as `#[contracttype]` definitions
  - Add `DataKey::PendingOp(u32)` and `DataKey::OpCounter` variants to the existing `DataKey` enum
  - Add `ContractError::TimelockNotElapsed = 6` and `ContractError::NoPendingOp = 7` to the existing `ContractError` enum
  - _Requirements: 6.1, 6.2, 8.1, 8.2_

- [x] 2. Add event structs to `events.rs`
  - Add `AdminOpProposed { op_id: u32, op: AdminOp, effective_timestamp: u64, proposed_by: Address }` struct with `#[contracttype]` and `#[derive(Clone)]`
  - Add `AdminOpExecuted { op_id: u32, op: AdminOp, executed_by: Address, executed_at: u64 }` struct
  - Add `AdminOpCancelled { op_id: u32, cancelled_by: Address, cancelled_at: u64 }` struct
  - Import `AdminOp` from `crate` in `events.rs`
  - _Requirements: 7.1, 7.2, 7.3, 7.4_

- [x] 3. Implement `propose_config_change` entry point
  - Add `pub fn propose_config_change(env: Env, protocol_fee_bp: u32, treasury: Address) -> Result<u32, ContractError>` to `RaffleFactory`
  - Require admin auth via the existing auth helper
  - Read `DataKey::OpCounter` from persistent storage (default 0), increment by 1, write back
  - Construct `PendingOp { op: AdminOp::SetConfig { protocol_fee_bp, treasury }, effective_timestamp: env.ledger().timestamp() + TIMELOCK_DELAY_SECONDS, proposed_by: admin_address }`
  - Store under `DataKey::PendingOp(op_id)` in persistent storage
  - Emit `AdminOpProposed` event under topic `("tikka", "admin_op_proposed")`
  - Return `op_id`
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6_

- [x] 4. Implement `execute_config_change` entry point
  - Add `pub fn execute_config_change(env: Env, op_id: u32) -> Result<(), ContractError>` to `RaffleFactory`
  - Require admin auth
  - Read `DataKey::PendingOp(op_id)` from persistent storage; return `ContractError::NoPendingOp` if absent
  - Return `ContractError::TimelockNotElapsed` if `env.ledger().timestamp() < pending_op.effective_timestamp`
  - Apply `AdminOp::SetConfig` parameters to `DataKey::ProtocolFeeBP` and `DataKey::Treasury` in persistent storage
  - Remove `DataKey::PendingOp(op_id)` from persistent storage
  - Emit `AdminOpExecuted` event under topic `("tikka", "admin_op_executed")`
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7_

- [x] 5. Implement `cancel_config_change` entry point
  - Add `pub fn cancel_config_change(env: Env, op_id: u32) -> Result<(), ContractError>` to `RaffleFactory`
  - Require admin auth
  - Read `DataKey::PendingOp(op_id)` from persistent storage; return `ContractError::NoPendingOp` if absent
  - Remove `DataKey::PendingOp(op_id)` from persistent storage
  - Emit `AdminOpCancelled` event under topic `("tikka", "admin_op_cancelled")`
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_

- [x] 6. Implement `get_pending_op` and `get_op_counter` view functions
  - Add `pub fn get_pending_op(env: Env, op_id: u32) -> Option<PendingOp>` — reads `DataKey::PendingOp(op_id)` from persistent storage, no auth required
  - Add `pub fn get_op_counter(env: Env) -> u32` — reads `DataKey::OpCounter` from persistent storage (default 0), no auth required
  - _Requirements: 4.1, 4.2, 4.3_

- [x] 7. Remove `set_config` entry point
  - Delete the `set_config` function from `RaffleFactory` in `lib.rs`
  - Verify `init_factory` still sets `ProtocolFeeBP` and `Treasury` directly (bootstrap exemption)
  - _Requirements: 5.1, 5.2_

- [x] 8. Checkpoint — ensure the contract compiles
  - Run `cargo build` in `contracts/raffle` and confirm zero errors before writing tests
  - Ask the user if any questions arise

- [x] 9. Write unit tests
  - [x] 9.1 `test_constant_value` — assert `TIMELOCK_DELAY_SECONDS == 172800`
    - _Requirements: 6.1_
  - [x] 9.2 `test_init_factory_sets_config_directly` — verify bootstrap exemption: `init_factory` sets `ProtocolFeeBP` and `Treasury` without a timelock
    - _Requirements: 5.2_
  - [x] 9.3 `test_get_pending_op_returns_none_for_missing_id` — `get_pending_op` on an absent key returns `None`
    - _Requirements: 4.1_
  - [x] 9.4 `test_get_op_counter_returns_zero_before_any_proposal` — counter starts at 0
    - _Requirements: 4.2_
  - [x] 9.5 `test_execute_returns_no_pending_op_for_missing_id` — `execute_config_change` with unknown `op_id` returns `ContractError::NoPendingOp`
    - _Requirements: 2.6_
  - [x] 9.6 `test_cancel_returns_no_pending_op_for_missing_id` — `cancel_config_change` with unknown `op_id` returns `ContractError::NoPendingOp`
    - _Requirements: 3.4_
  - [x] 9.7 `test_view_functions_require_no_auth` — `get_pending_op` and `get_op_counter` callable without admin auth
    - _Requirements: 4.3_
  - [x] 9.8 `test_set_config_removed` — compile-time verification that `set_config` no longer exists (confirmed by absence in source after task 7)
    - _Requirements: 5.1_

- [ ] 10. Write property-based tests using `proptest`
  - Add `proptest` to `[dev-dependencies]` in `contracts/raffle/Cargo.toml` if not already present
  - Use the hybrid approach: `proptest` generates raw `u32`/byte values; construct `soroban_sdk::Address` values inside a fresh `Env`; advance ledger timestamp via `env.ledger().set_timestamp(t)`
  - [ ]* 10.1 `prop_admin_only_authorization` — for any non-admin caller, all three mutating entry points return `ContractError::NotAuthorized`
    - `// Feature: time-locked-admin-operations, Property 1: Admin-only authorization`
    - _Requirements: 1.1, 1.5, 2.1, 2.7, 3.1, 3.5_
  - [ ]* 10.2 `prop_propose_round_trip_storage` — after `propose_config_change`, `get_pending_op` returns a `PendingOp` matching the proposed params and correct `effective_timestamp`
    - `// Feature: time-locked-admin-operations, Property 2: Propose round-trip storage`
    - _Requirements: 1.2, 4.1, 8.1, 8.4_
  - [ ]* 10.3 `prop_counter_monotonically_increments` — after N proposals, `get_op_counter` equals N and each successive `op_id` is exactly one greater
    - `// Feature: time-locked-admin-operations, Property 3: Counter monotonically increments`
    - _Requirements: 1.3, 4.2, 8.2_
  - [ ]* 10.4 `prop_propose_emits_correct_event` — emitted `AdminOpProposed` event contains correct `op_id`, `AdminOp` payload, `effective_timestamp`, and proposer address under `("tikka", "admin_op_proposed")`
    - `// Feature: time-locked-admin-operations, Property 4: Propose emits correct event`
    - _Requirements: 1.4, 7.1, 7.4_
  - [ ]* 10.5 `prop_execute_applies_config_and_removes_op` — after timelock elapses and `execute_config_change` succeeds, stored `ProtocolFeeBP` and `Treasury` match op params and `get_pending_op` returns `None`
    - `// Feature: time-locked-admin-operations, Property 5: Execute applies config and removes pending op`
    - _Requirements: 2.2, 2.3, 8.3_
  - [ ]* 10.6 `prop_execute_emits_correct_event` — emitted `AdminOpExecuted` event contains correct `op_id`, `AdminOp` payload, executor address, and execution timestamp under `("tikka", "admin_op_executed")`
    - `// Feature: time-locked-admin-operations, Property 6: Execute emits correct event`
    - _Requirements: 2.4, 7.2, 7.4_
  - [ ]* 10.7 `prop_timelock_guard` — calling `execute_config_change` at any timestamp strictly less than `effective_timestamp` returns `ContractError::TimelockNotElapsed` and stored config is unchanged
    - `// Feature: time-locked-admin-operations, Property 7: Timelock guard`
    - _Requirements: 2.5_
  - [ ]* 10.8 `prop_multiple_ops_coexist` — after N proposals without executions or cancellations, all N `PendingOp` entries are independently retrievable by their distinct `op_id` values
    - `// Feature: time-locked-admin-operations, Property 8: Multiple ops coexist`
    - _Requirements: 1.6_
  - [ ]* 10.9 `prop_cancel_removes_pending_op` — after `cancel_config_change`, `get_pending_op` returns `None` and stored config is unchanged
    - `// Feature: time-locked-admin-operations, Property 9: Cancel removes pending op`
    - _Requirements: 3.2, 8.3_
  - [ ]* 10.10 `prop_cancel_emits_correct_event` — emitted `AdminOpCancelled` event contains correct `op_id`, canceller address, and cancellation timestamp under `("tikka", "admin_op_cancelled")`
    - `// Feature: time-locked-admin-operations, Property 10: Cancel emits correct event`
    - _Requirements: 3.3, 7.3, 7.4_

- [x] 11. Final checkpoint — ensure all tests pass
  - Run `cargo test` in `contracts/raffle` and confirm all unit and property tests pass
  - Ask the user if any questions arise

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Each task references specific requirements for traceability
- Property tests use `proptest` with a minimum of 100 iterations each
- `set_config` removal (task 7) must happen before tests are written to avoid compile errors
