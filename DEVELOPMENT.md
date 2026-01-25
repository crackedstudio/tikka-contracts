# Development Guide

This guide helps developers build and test the Soroban raffle contract locally.

## Project Layout

-   `contracts/hello-world/src/lib.rs`: Soroban raffle contract
-   `contracts/hello-world/src/test.rs`: Contract tests
-   `README.md`: Project overview

## Prerequisites

-   Rust toolchain (stable)
-   Cargo (bundled with Rust)
-   Stellar CLI (optional, for deployment)

## Build

```bash
cargo build -p hello-world
```

## Test

```bash
cargo test -p hello-world
```

## Notes

-   The contract uses Soroban SDK v23 from the workspace.
-   Network access is required the first time dependencies are fetched.

## Recent Contributions

### TicketPurchased Event System (Issue #2)

**What was added:**
- `TicketPurchased` event struct with 6 fields: `raffle_id`, `buyer`, `ticket_ids`, `quantity`, `total_paid`, `timestamp`
- Event emission in `buy_ticket()` for single purchases
- New `buy_tickets()` function for batch purchases with event emission
- Comprehensive test coverage (4 tests, all passing)

**Test Data & Mock Examples:**
- All test data is in `contracts/hello-world/src/test.rs`
- Tests use `env.mock_all_auths()` and `Address::generate()` for mock addresses
- Token mints: 1,000 tokens per test participant
- Test raffles: 10 max tickets, 10 token price, 100 token prize
- Event retrieval pattern: `env.events().all()` → filter by `try_into_val<TicketPurchased>`

**Handoff Notes:**
- Events use topic `"TktPurch"` (symbol_short! max 9 chars)
- Event emission happens after state updates, before function return
- For multiple transactions: retrieve events after each transaction separately
- Ticket IDs are 1-indexed (first ticket = ID 1)
- Batch purchases emit single event with all ticket IDs in `ticket_ids` Vec
