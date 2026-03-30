# Implementation Plan: Randomness Oracle Abstraction

## Tasks

- [x] 1. Create `contracts/raffle/src/oracle.rs`
  - Define `RandomnessRequest` struct (`raffle_id`, `request_id`, `callback_address`)
  - Define `RandomnessOracleTrait` with `request_randomness(env, request)` — generates `RandomnessOracleClient`
  - Define `RandomnessReceiverTrait` with `receive_randomness(env, request_id, random_seed)` — generates `RandomnessReceiverClient`

- [x] 2. Expose `oracle` module in `contracts/raffle/src/lib.rs`

- [x] 3. Add `DataKey::PendingRequestId` variant to `DataKey` enum in `mod.rs`

- [x] 4. Import `RandomnessOracleClient` and `RandomnessRequest` in `mod.rs`

- [x] 5. Extract shared winner-selection into `do_finalize_with_seed(env, seed)` free function
  - Eliminates duplication between `provide_randomness` and `receive_randomness`

- [x] 6. Update `finalize_raffle` External branch
  - Build `RandomnessRequest` with `request_id = ledger.sequence()`
  - Store `PendingRequestId` in instance storage
  - Dispatch via `RandomnessOracleClient::request_randomness` instead of bare event

- [x] 7. Add `receive_randomness(env, request_id, random_seed)` public function
  - Validates oracle auth, raffle state, and `request_id` against stored pending value
  - Clears `PendingRequestId` then delegates to `do_finalize_with_seed`

- [x] 8. Simplify `provide_randomness` to delegate to `do_finalize_with_seed`
  - Preserves existing public ABI for backward compatibility
