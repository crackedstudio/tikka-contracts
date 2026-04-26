# Implementation Plan: Ticket Refund System

## Tasks

- [x] 1. Implement `refund_ticket(env, ticket_id)` in `contracts/raffle/src/instance/mod.rs`
  - Validates raffle is in `Cancelled` status
  - Looks up ticket by `ticket_id` from persistent storage
  - Verifies caller is the ticket owner via `require_auth`
  - Checks `RefundStatus(ticket_id)` to prevent double-refund
  - Acquires reentrancy guard before state changes
  - Sets `RefundStatus(ticket_id) = true` before token transfer (CEI pattern)
  - Transfers `ticket_price` back to ticket owner via payment token
  - Releases reentrancy guard
  - Emits `TicketRefunded` event with buyer, ticket_id, amount, timestamp
  - Returns refunded amount as `i128`

- [x] 2. Verify `DataKey::RefundStatus(u32)` storage key exists in `DataKey` enum

- [x] 3. Verify `TicketRefunded` event struct exists in `contracts/raffle/src/events.rs`

- [x] 4. Write tests in `contracts/raffle/src/instance/test.rs`
  - `test_refund_ticket` — happy path: cancel raffle, refund ticket, verify balance restored
  - `test_double_refund_rejected` — second refund on same ticket panics with `InvalidStateTransition`
  - `test_refund_guard_released_after_success` — reentrancy guard is cleared after successful refund
  - `test_sequential_refunds_succeed_guard_properly_released` — multiple tickets refunded sequentially
  - `test_refund_blocked_by_active_reentrancy_guard` — refund panics when guard is already held
