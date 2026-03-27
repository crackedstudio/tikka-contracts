# Implementation Plan: Paginated Query System

## Overview

Introduce limit/offset pagination to `get_raffles` (RaffleFactory) and add a new `get_tickets` query to the raffle instance contract. Shared types live in a new `types.rs` module; both contracts import from it.

## Tasks

- [x] 1. Create `contracts/raffle/src/types.rs` with shared pagination types
  - Define constants `DEFAULT_PAGE_LIMIT = 100` and `MAX_PAGE_LIMIT = 200`
  - Implement `#[contracttype]` structs: `PaginationParams`, `PageResult_Raffles`, `PageResult_Tickets`
  - Implement `pub fn effective_limit(requested: u32) -> u32` with zero-default and clamp logic
  - Import `Ticket` from `crate::instance`
  - _Requirements: 1.1, 1.3, 1.4, 1.5, 1.6_

- [x] 2. Add `proptest` dependency and wire up the `types` module
  - [x] 2.1 Add `proptest = "1"` under `[dev-dependencies]` in `contracts/raffle/Cargo.toml`
    - _Requirements: (testing infrastructure)_
  - [x] 2.2 Declare `pub mod types;` in `contracts/raffle/src/lib.rs` and add `pub use types::{PaginationParams, PageResult_Raffles, PageResult_Tickets, effective_limit};`
    - _Requirements: 1.1, 1.2_

- [x] 3. Implement `get_raffles_page` on `RaffleFactory` in `lib.rs`
  - Add `pub fn get_raffles_page(env: Env, params: PaginationParams) -> PageResult_Raffles` to the `RaffleFactory` contract impl
  - Load `Vec<Address>` from `DataKey::RaffleInstances` (or empty vec if absent)
  - Compute `total`, apply `effective_limit`, slice `offset..end`, set `has_more`
  - Must not call any storage `set` or `remove` — read-only
  - Verify existing `get_raffles` function is unchanged
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 4.1_

- [x] 4. Implement `get_tickets` on `Contract` in `instance/mod.rs`
  - Import `PaginationParams`, `PageResult_Tickets`, `effective_limit` from `crate::types`
  - Add `pub fn get_tickets(env: Env, params: PaginationParams) -> PageResult_Tickets` to the `Contract` impl
  - Read `Raffle` via `read_raffle` to get `tickets_sold` as `total`
  - Iterate `offset..end`, loading `DataKey::Ticket(i + 1)` for each index (1-based ticket_number)
  - Set `has_more` as `offset + items.len() < total`
  - Must not call any storage `set` or `remove` — read-only
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7_

- [x] 5. Checkpoint — ensure the project compiles
  - Ensure all modules resolve, types import correctly, and `cargo build` succeeds with no errors.
  - Ask the user if any questions arise before proceeding to tests.

- [-] 6. Write property-based tests for `effective_limit` and `get_raffles_page`
  - [-] 6.1 Write property test for `effective_limit` correctness
    - Add to a new test file `contracts/raffle/src/factory_test.rs` (or inline in `lib.rs` test module)
    - Use `proptest!` macro; tag with `// Feature: paginated-query-system, Property 1`
    - Assert: `effective_limit(0) == 100`, `effective_limit(x) == 200` when `x > 200`, `effective_limit(x) == x` when `0 < x <= 200`
    - _Requirements: 1.3, 1.4, 1.5, 1.6_

  - [ ]* 6.2 Write property test for `get_raffles_page` slice correctness
    - Tag with `// Feature: paginated-query-system, Property 2`
    - For random `(n_raffles, offset, limit)`: assert `total == n_raffles`, `items` matches expected sub-slice, `has_more` matches formula
    - _Requirements: 2.2, 2.3, 2.4, 2.5, 5.1_

  - [ ]* 6.3 Write property test for `get_raffles_page` read-only
    - Tag with `// Feature: paginated-query-system, Property 4`
    - After calling `get_raffles_page`, assert stored raffle list is unchanged
    - _Requirements: 2.6_

  - [ ]* 6.4 Write property test for `get_raffles_page` pagination completeness
    - Tag with `// Feature: paginated-query-system, Property 6`
    - Iterate all pages with sequential offsets; concatenate `items`; assert equals full `get_raffles` result
    - _Requirements: 4.2, 5.2_

- [~] 7. Write unit tests for `get_raffles_page` in the factory test module
  - [ ]* 7.1 Write unit tests covering all 9 `get_raffles_page` / `get_raffles` edge cases
    - `test_get_raffles_page_empty_factory` — zero raffles, any params → empty result
    - `test_get_raffles_page_exact_page` — offset=0, limit=5, 5 raffles → len==5, has_more=false
    - `test_get_raffles_page_partial_last_page` — offset=3, limit=5, 7 raffles → len==4, has_more=false
    - `test_get_raffles_page_has_more_true` — offset=0, limit=3, 7 raffles → has_more=true
    - `test_get_raffles_page_offset_equals_total` — offset==total → empty, has_more=false
    - `test_get_raffles_page_offset_beyond_total` — offset > total → empty, has_more=false
    - `test_get_raffles_page_limit_zero_defaults` — limit=0, 150 raffles → exactly 100 returned
    - `test_get_raffles_page_limit_clamped` — limit=300, 250 raffles → exactly 200 returned
    - `test_get_raffles_backward_compat` — `get_raffles` still returns all addresses
    - _Requirements: 1.3, 1.4, 2.2, 2.3, 2.4, 2.5, 4.1_

- [~] 8. Write property-based tests for `get_tickets` in `instance/test.rs`
  - [ ]* 8.1 Write property test for `get_tickets` slice correctness
    - Tag with `// Feature: paginated-query-system, Property 3`
    - For random `(n_tickets, offset, limit)`: assert `total`, `items` sub-slice, `has_more` formula
    - _Requirements: 3.2, 3.3, 3.4, 3.5, 5.1_

  - [ ]* 8.2 Write property test for `get_tickets` read-only
    - Tag with `// Feature: paginated-query-system, Property 5`
    - After calling `get_tickets`, assert `tickets_sold` and all `DataKey::Ticket(n)` entries are unchanged
    - _Requirements: 3.6_

  - [ ]* 8.3 Write property test for `get_tickets` pagination completeness and ordering
    - Tag with `// Feature: paginated-query-system, Property 7`
    - Iterate all pages; concatenate `items`; assert equals all tickets in strictly ascending `ticket_number` order, no duplicates, no omissions
    - _Requirements: 3.7, 5.3_

- [~] 9. Write unit tests for `get_tickets` in `instance/test.rs`
  - [ ]* 9.1 Write unit tests covering all 8 `get_tickets` edge cases
    - `test_get_tickets_zero_tickets` — offset=0, limit=0, 0 tickets → `{items:[], total:0, has_more:false}`
    - `test_get_tickets_exact_page` — offset=0, limit=3, 3 tickets → len==3, has_more=false
    - `test_get_tickets_partial_last_page` — offset=2, limit=5, 4 tickets → len==2, has_more=false
    - `test_get_tickets_has_more_true` — offset=0, limit=2, 5 tickets → has_more=true
    - `test_get_tickets_offset_equals_total` — offset==tickets_sold → empty, has_more=false
    - `test_get_tickets_ascending_order` — verify ticket_number is strictly increasing across returned items
    - `test_get_tickets_limit_zero_defaults` — limit=0, 150 tickets → exactly 100 returned
    - `test_get_tickets_limit_clamped` — limit=300, 250 tickets → exactly 200 returned
    - _Requirements: 1.5, 1.6, 3.2, 3.3, 3.4, 3.5, 3.7, 5.4_

- [~] 10. Final checkpoint — ensure all tests pass
  - Run `cargo test` in `contracts/raffle`; all tests must pass with no warnings on new code.
  - Ask the user if any questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for a faster MVP
- Property tests use `proptest = "1"` and run 100 iterations by default
- Each property test references a numbered property from the design document
- `ticket_number` is 1-based; the loop index `i` maps to `DataKey::Ticket(i + 1)`
- `get_raffles` is preserved unchanged throughout — backward compatibility is non-negotiable
