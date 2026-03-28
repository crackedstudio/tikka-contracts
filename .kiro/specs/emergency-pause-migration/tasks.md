# Implementation Plan: Emergency Pause & Migration (Circuit Breaker)

## Overview

Small delta implementation: add `require_not_paused` guards to `buy_ticket` and `deposit_prize` in `RaffleInstance`, verify all factory-level pause infrastructure is wired correctly, then write the full unit and property-based test suite covering all 8 correctness properties.

## Tasks

- [x] 1. Add `require_not_paused` guard to RaffleInstance write operations
  - In `contracts/raffle/src/instance/mod.rs`, confirm or add the `require_not_paused` helper:
    ```rust
    fn require_not_paused(env: &Env) -> Result<(), Error> {
        if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {
            return Err(Error::ContractPaused);
        }
        Ok(())
    }
    ```
  - Add `require_not_paused(&env)?;` as the first statement in `buy_ticket`
  - Add `require_not_paused(&env)?;` as the first statement in `deposit_prize`
  - _Requirements: 4.1, 4.2, 4.3, 4.4_

- [x] 2. Verify RaffleInstance pause/unpause/is_paused entry points
  - Confirm `pause()`, `unpause()`, and `is_paused()` exist in `contracts/raffle/src/instance/mod.rs`
  - If any are missing, implement them using `DataKey::Paused` in instance storage, requiring the stored Factory address for auth on `pause`/`unpause`, and emitting `ContractPaused`/`ContractUnpaused` events
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_

- [x] 3. Verify RaffleFactory pause delegation entry points
  - Confirm `pause_instance(addr)` and `unpause_instance(addr)` exist in `contracts/raffle/src/lib.rs`
  - If missing, implement them: require Factory_Admin auth, then cross-contract call `RaffleInstance::pause()` / `RaffleInstance::unpause()` on the given address
  - Confirm `is_paused()` view exists on the factory
  - _Requirements: 6.1, 6.2, 6.3, 1.5_

- [x] 4. Write unit tests for RaffleFactory pause behaviour
  - Add tests in the factory test module (or `contracts/raffle/src/lib.rs` test section):
    - `pause` sets flag to `true`, emits `ContractPaused` event
    - `unpause` sets flag to `false`, emits `ContractUnpaused` event
    - `is_paused` returns `false` on a freshly initialised factory (absent key)
    - `create_raffle` returns `ContractError::ContractPaused` when factory is paused
    - `create_raffle` succeeds when factory is unpaused
    - Non-admin caller on `pause`/`unpause` returns `ContractError::NotAuthorized`
    - `pause_instance` / `unpause_instance` by non-admin returns `ContractError::NotAuthorized`
  - _Requirements: 1.2, 1.3, 1.4, 1.5, 2.1, 2.2, 6.3_

  - [ ]* 4.1 Write property test for factory pause flag round-trip (Property 1)
    - Use `proptest` to generate random sequences of `pause`/`unpause` calls on the factory
    - Assert `is_paused()` returns `true` iff the last call was `pause`
    - Minimum 100 iterations
    - Include comment: `// Feature: emergency-pause-migration, Property 1: Factory pause flag round-trip`
    - **Property 1: Factory pause flag round-trip**
    - **Validates: Requirements 1.2, 1.3, 7.4**

  - [ ]* 4.2 Write property test for paused factory blocking create_raffle (Property 3)
    - Generate random valid `create_raffle` argument sets; assert call returns `ContractError::ContractPaused` when factory is paused
    - Include comment: `// Feature: emergency-pause-migration, Property 3: Paused factory blocks create_raffle`
    - **Property 3: Paused factory blocks create_raffle**
    - **Validates: Requirements 2.1**

  - [ ]* 4.3 Write property test for unpaused factory allowing create_raffle (Property 4)
    - Generate random valid `create_raffle` argument sets; assert call succeeds and raffle list grows by 1 when factory is unpaused
    - Include comment: `// Feature: emergency-pause-migration, Property 4: Unpaused factory allows create_raffle`
    - **Property 4: Unpaused factory allows create_raffle**
    - **Validates: Requirements 2.2**

  - [ ]* 4.4 Write property test for unauthorised pause/unpause rejection (Property 7)
    - Generate random non-admin addresses; assert `pause`/`unpause` on factory returns `ContractError::NotAuthorized`
    - Include comment: `// Feature: emergency-pause-migration, Property 7: Unauthorised pause/unpause rejected`
    - **Property 7: Unauthorised pause/unpause rejected (factory side)**
    - **Validates: Requirements 1.4, 6.3**

- [x] 5. Write unit tests for RaffleInstance pause behaviour
  - Add tests in `contracts/raffle/src/instance/test.rs`:
    - `pause` sets flag to `true`, emits `ContractPaused` event
    - `unpause` sets flag to `false`, emits `ContractUnpaused` event
    - `is_paused` returns `false` on a freshly deployed instance
    - `buy_ticket` returns `Error::ContractPaused` when instance is paused; no token transfer occurs
    - `deposit_prize` returns `Error::ContractPaused` when instance is paused; no token transfer occurs
    - `buy_ticket` succeeds when instance is unpaused
    - `deposit_prize` succeeds when instance is unpaused
    - Non-factory caller on `pause`/`unpause` returns `Error::NotAuthorized`
    - All exit operations (`finalize_raffle`, `provide_randomness`, `claim_prize`, `cancel_raffle`, `refund_ticket`) complete normally while instance is paused
  - _Requirements: 3.2, 3.3, 3.4, 3.5, 4.1, 4.2, 4.3, 4.4, 5.1, 5.2, 5.3, 5.4, 5.5_

  - [ ]* 5.1 Write property test for instance pause flag round-trip (Property 2)
    - Generate random sequences of `pause`/`unpause` calls on a RaffleInstance; assert `is_paused()` matches last call
    - Minimum 100 iterations
    - Include comment: `// Feature: emergency-pause-migration, Property 2: Instance pause flag round-trip`
    - **Property 2: Instance pause flag round-trip**
    - **Validates: Requirements 3.2, 3.3, 7.5**

  - [ ]* 5.2 Write property test for paused instance blocking write operations (Property 5)
    - Generate random `buy_ticket` / `deposit_prize` inputs on a paused instance; assert `Error::ContractPaused` and no state change
    - Include comment: `// Feature: emergency-pause-migration, Property 5: Paused instance blocks write operations`
    - **Property 5: Paused instance blocks write operations**
    - **Validates: Requirements 4.1, 4.2**

  - [ ]* 5.3 Write property test for exit operations unaffected by pause (Property 6)
    - For random raffle states, assert exit operations return the same result regardless of the pause flag
    - Include comment: `// Feature: emergency-pause-migration, Property 6: Exit operations unaffected by pause`
    - **Property 6: Exit operations unaffected by pause**
    - **Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.5**

  - [ ]* 5.4 Write property test for unauthorised instance pause/unpause rejection (Property 7)
    - Generate random non-factory addresses; assert `pause`/`unpause` on instance returns `Error::NotAuthorized`
    - Include comment: `// Feature: emergency-pause-migration, Property 7: Unauthorised pause/unpause rejected (instance side)`
    - **Property 7: Unauthorised pause/unpause rejected (instance side)**
    - **Validates: Requirements 3.4**

- [x] 6. Write unit and property tests for factory-to-instance delegation
  - Add tests verifying `pause_instance` causes the target instance's `is_paused()` to return `true`
  - Add tests verifying `unpause_instance` causes the target instance's `is_paused()` to return `false`
  - _Requirements: 6.1, 6.2_

  - [ ]* 6.1 Write property test for factory delegation propagating pause to instance (Property 8)
    - For random instance addresses, assert `pause_instance(addr)` → instance `is_paused()` is `true`; `unpause_instance(addr)` → `false`
    - Include comment: `// Feature: emergency-pause-migration, Property 8: Factory delegation propagates pause to instance`
    - **Property 8: Factory delegation propagates pause to instance**
    - **Validates: Requirements 6.1, 6.2**

- [x] 7. Final checkpoint — ensure all tests pass
  - Run `cargo test -p raffle` and confirm all tests pass
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Property tests require `proptest` (or `quickcheck`) in `[dev-dependencies]` in `contracts/raffle/Cargo.toml`
- Each property test must run a minimum of 100 iterations and include the feature/property comment header
- All error paths must return before any state mutation or token transfer
