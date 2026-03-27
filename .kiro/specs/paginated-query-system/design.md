# Design Document: Paginated Query System

## Overview

This design adds limit/offset pagination to the two unbounded list-returning query functions in the Tikka raffle protocol. The goal is to prevent Soroban CPU/memory budget exhaustion as the factory accumulates raffle instances and individual raffles accumulate ticket entries.

Two functions are affected:
- `get_raffles` on `RaffleFactory` (`contracts/raffle/src/lib.rs`) — currently returns all deployed raffle addresses as a `Vec<Address>`
- `get_tickets` on `Contract` (`contracts/raffle/src/instance/mod.rs`) — currently does not exist as a public query; tickets are stored individually under `DataKey::Ticket(u32)` keys

The approach introduces a shared `PaginationParams` struct and two `PageResult` structs, adds a new `get_raffles_page` function to `RaffleFactory`, and adds a new `get_tickets` function to `Contract`. The existing `get_raffles` function is preserved unchanged for backward compatibility.

### Constants

```
DEFAULT_PAGE_LIMIT = 100   // effective limit when params.limit == 0
MAX_PAGE_LIMIT     = 200   // hard cap; requests above this are clamped
```

---

## Architecture

The pagination types live in a new `types` module at `contracts/raffle/src/types.rs`, re-exported from `lib.rs`. Both `RaffleFactory` and `Contract` import from this shared module, ensuring a single canonical definition.

```
contracts/raffle/src/
├── lib.rs              ← RaffleFactory; imports types; adds get_raffles_page
├── types.rs            ← NEW: PaginationParams, PageResult_Raffles, PageResult_Tickets
├── events.rs           ← unchanged
└── instance/
    ├── mod.rs          ← Contract; imports types; adds get_tickets
    └── test.rs         ← existing + new pagination tests
```

### Data flow for `get_raffles_page`

```
Caller → get_raffles_page(env, PaginationParams { limit, offset })
  │
  ├─ resolve effective_limit (clamp/default)
  ├─ load Vec<Address> from DataKey::RaffleInstances (persistent storage)
  ├─ total = vec.len()
  ├─ slice = vec[offset .. min(offset + effective_limit, total)]
  └─ return PageResult_Raffles { items: slice, total, has_more }
```

### Data flow for `get_tickets`

```
Caller → get_tickets(env, PaginationParams { limit, offset })
  │
  ├─ resolve effective_limit (clamp/default)
  ├─ read Raffle from instance storage → total = raffle.tickets_sold
  ├─ for i in offset .. min(offset + effective_limit, total):
  │     load DataKey::Ticket(i + 1)   ← ticket_number is 1-based
  └─ return PageResult_Tickets { items, total, has_more }
```

Tickets are stored individually under `DataKey::Ticket(id)` where `id` is the 1-based `ticket_number` assigned at purchase time (see `buy_ticket` in `instance/mod.rs`). Iterating by sequential `ticket_number` naturally produces ascending order without sorting.

---

## Components and Interfaces

### `contracts/raffle/src/types.rs` (new file)

```rust
use soroban_sdk::{contracttype, Address, Vec};
use crate::instance::Ticket;

pub const DEFAULT_PAGE_LIMIT: u32 = 100;
pub const MAX_PAGE_LIMIT: u32 = 200;

#[derive(Clone)]
#[contracttype]
pub struct PaginationParams {
    pub limit: u32,
    pub offset: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct PageResult_Raffles {
    pub items: Vec<Address>,
    pub total: u32,
    pub has_more: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct PageResult_Tickets {
    pub items: Vec<Ticket>,
    pub total: u32,
    pub has_more: bool,
}
```

A helper function for resolving the effective limit is shared:

```rust
pub fn effective_limit(requested: u32) -> u32 {
    if requested == 0 {
        DEFAULT_PAGE_LIMIT
    } else if requested > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT
    } else {
        requested
    }
}
```

### `RaffleFactory::get_raffles_page` (new function in `lib.rs`)

```rust
pub fn get_raffles_page(env: Env, params: PaginationParams) -> PageResult_Raffles {
    let all: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RaffleInstances)
        .unwrap_or_else(|| Vec::new(&env));

    let total = all.len();
    let lim = effective_limit(params.limit);
    let offset = params.offset;

    if offset >= total {
        return PageResult_Raffles {
            items: Vec::new(&env),
            total,
            has_more: false,
        };
    }

    let end = (offset + lim).min(total);
    let mut items = Vec::new(&env);
    for i in offset..end {
        items.push_back(all.get(i).unwrap());
    }

    let has_more = (offset + items.len()) < total;
    PageResult_Raffles { items, total, has_more }
}
```

This function is read-only; it does not call any `set` or `remove` on storage.

### `Contract::get_tickets` (new function in `instance/mod.rs`)

```rust
pub fn get_tickets(env: Env, params: PaginationParams) -> PageResult_Tickets {
    let raffle = read_raffle(&env).expect("not initialized");
    let total = raffle.tickets_sold;
    let lim = effective_limit(params.limit);
    let offset = params.offset;

    if offset >= total {
        return PageResult_Tickets {
            items: Vec::new(&env),
            total,
            has_more: false,
        };
    }

    let end = (offset + lim).min(total);
    let mut items = Vec::new(&env);
    for i in offset..end {
        let ticket_number = i + 1; // ticket_number is 1-based
        let ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_number))
            .expect("ticket missing");
        items.push_back(ticket);
    }

    let has_more = (offset + items.len()) < total;
    PageResult_Tickets { items, total, has_more }
}
```

This function is read-only; it does not call any `set` or `remove` on storage.

### `RaffleFactory::get_raffles` (unchanged)

The existing function signature and behavior are preserved exactly:

```rust
pub fn get_raffles(env: Env) -> Vec<Address> { ... }
```

---

## Data Models

### `PaginationParams`

| Field    | Type  | Description                                              |
|----------|-------|----------------------------------------------------------|
| `limit`  | `u32` | Max items to return. `0` → `DEFAULT_PAGE_LIMIT` (100).  |
| `offset` | `u32` | Zero-based index of the first item to return.            |

### `PageResult_Raffles`

| Field      | Type           | Description                                                  |
|------------|----------------|--------------------------------------------------------------|
| `items`    | `Vec<Address>` | Slice of raffle addresses for this page.                     |
| `total`    | `u32`          | Total number of raffle addresses stored at call time.        |
| `has_more` | `bool`         | `true` iff `offset + items.len() < total`.                   |

### `PageResult_Tickets`

| Field      | Type          | Description                                                  |
|------------|---------------|--------------------------------------------------------------|
| `items`    | `Vec<Ticket>` | Slice of `Ticket` structs for this page.                     |
| `total`    | `u32`         | Value of `Raffle.tickets_sold` at call time.                 |
| `has_more` | `bool`        | `true` iff `offset + items.len() < total`.                   |

### `Ticket` (existing, unchanged)

| Field           | Type      | Description                              |
|-----------------|-----------|------------------------------------------|
| `id`            | `u32`     | Auto-incremented ticket ID.              |
| `buyer`         | `Address` | Purchaser's address.                     |
| `purchase_time` | `u64`     | Ledger timestamp at purchase.            |
| `ticket_number` | `u32`     | 1-based sequential number within raffle. |

### Storage key mapping

Tickets are stored under `DataKey::Ticket(ticket_number)` where `ticket_number` starts at 1 and increments with each `buy_ticket` call. The `get_tickets` implementation iterates `offset+1 ..= end` to load tickets in ascending `ticket_number` order without requiring a separate index.


---

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system — essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: effective_limit correctness

*For any* requested limit value, `effective_limit(0)` must equal `DEFAULT_PAGE_LIMIT` (100), `effective_limit(x)` for `x > MAX_PAGE_LIMIT` must equal `MAX_PAGE_LIMIT` (200), and `effective_limit(x)` for `0 < x <= MAX_PAGE_LIMIT` must equal `x` unchanged.

**Validates: Requirements 1.3, 1.4, 1.5, 1.6**

### Property 2: get_raffles_page slice correctness

*For any* factory state with `N` stored raffle addresses and any `PaginationParams { offset, limit }`, the returned `PageResult_Raffles` must satisfy:
- `total == N`
- `items` equals the sub-slice `all_raffles[offset .. min(offset + effective_limit(limit), N)]` (empty slice when `offset >= N`)
- `has_more == (offset + items.len() < N)`

**Validates: Requirements 2.2, 2.3, 2.4, 2.5, 5.1**

### Property 3: get_tickets slice correctness

*For any* raffle instance with `N` tickets sold and any `PaginationParams { offset, limit }`, the returned `PageResult_Tickets` must satisfy:
- `total == N`
- `items` equals the sub-slice of tickets ordered by `ticket_number` from index `offset` to `min(offset + effective_limit(limit), N)` (empty slice when `offset >= N`)
- `has_more == (offset + items.len() < N)`

**Validates: Requirements 3.2, 3.3, 3.4, 3.5, 5.1**

### Property 4: get_raffles_page is read-only

*For any* factory state, calling `get_raffles_page` with any `PaginationParams` must leave the stored raffle address list identical to what it was before the call.

**Validates: Requirements 2.6**

### Property 5: get_tickets is read-only

*For any* raffle instance state, calling `get_tickets` with any `PaginationParams` must leave `Raffle.tickets_sold` and all `DataKey::Ticket(n)` entries identical to what they were before the call.

**Validates: Requirements 3.6**

### Property 6: get_raffles_page pagination completeness

*For any* factory state with `N` stored raffle addresses, iterating through all pages with sequential offsets (page size `P`) and concatenating the `items` fields must produce a sequence equal to the full list returned by `get_raffles`.

**Validates: Requirements 4.2, 5.2**

### Property 7: get_tickets pagination completeness and ordering

*For any* raffle instance with `N` tickets sold, iterating through all pages with sequential offsets and concatenating the `items` fields must produce a sequence of all `N` tickets in strictly ascending `ticket_number` order with no duplicates and no omissions.

**Validates: Requirements 3.7, 5.3**

---

## Error Handling

### Offset out of bounds

When `params.offset >= total`, both functions return a valid `PageResult` with `items: []`, `total` set to the actual count, and `has_more: false`. No panic or error is returned — an out-of-range offset is a valid query that happens to return no results.

### Uninitialized contract

`get_tickets` calls `read_raffle` internally. If the instance contract has not been initialized, `read_raffle` returns `Err(Error::NotInitialized)`. The function propagates this as a contract error. Callers should only query initialized instances.

### Missing ticket entries

If a `DataKey::Ticket(n)` entry is absent for a ticket number within `[1, tickets_sold]`, the implementation panics with `expect("ticket missing")`. This should never occur in a correctly operating contract because `buy_ticket` always writes the ticket before incrementing `tickets_sold`. If it does occur it indicates storage corruption, which is unrecoverable.

### Arithmetic overflow

`offset + effective_limit` could theoretically overflow `u32` for extreme inputs. Since `MAX_PAGE_LIMIT` is 200 and `offset` is bounded by the collection size (which is itself bounded by Soroban storage limits well below `u32::MAX`), overflow is not reachable in practice. No explicit overflow guard is needed.

---

## Testing Strategy

### Dual testing approach

Both unit tests and property-based tests are required. Unit tests cover specific examples and edge cases; property tests verify universal correctness across randomized inputs. Together they provide comprehensive coverage.

### Property-based testing library

Use [`proptest`](https://github.com/proptest-rs/proptest) (crate `proptest = "1"`). Each property test runs a minimum of **100 iterations** (proptest default). Tests are tagged with a comment referencing the design property.

Each correctness property above is implemented by exactly one property-based test.

### Property test specifications

```
// Feature: paginated-query-system, Property 1: effective_limit correctness
proptest! {
    fn prop_effective_limit_correctness(limit: u32) { ... }
}

// Feature: paginated-query-system, Property 2: get_raffles_page slice correctness
proptest! {
    fn prop_get_raffles_page_slice(n_raffles: u32, offset: u32, limit: u32) { ... }
}

// Feature: paginated-query-system, Property 3: get_tickets slice correctness
proptest! {
    fn prop_get_tickets_slice(n_tickets: u32, offset: u32, limit: u32) { ... }
}

// Feature: paginated-query-system, Property 4: get_raffles_page is read-only
proptest! {
    fn prop_get_raffles_page_readonly(n_raffles: u32, offset: u32, limit: u32) { ... }
}

// Feature: paginated-query-system, Property 5: get_tickets is read-only
proptest! {
    fn prop_get_tickets_readonly(n_tickets: u32, offset: u32, limit: u32) { ... }
}

// Feature: paginated-query-system, Property 6: get_raffles_page pagination completeness
proptest! {
    fn prop_get_raffles_page_completeness(n_raffles: u32, page_size: u32) { ... }
}

// Feature: paginated-query-system, Property 7: get_tickets pagination completeness and ordering
proptest! {
    fn prop_get_tickets_completeness_and_ordering(n_tickets: u32, page_size: u32) { ... }
}
```

### Unit test specifications

Unit tests focus on specific examples and edge cases not well-served by property tests:

- `test_get_raffles_page_empty_factory` — factory with zero raffles, any params → empty result
- `test_get_raffles_page_exact_page` — offset=0, limit=5, 5 raffles → items.len()==5, has_more=false
- `test_get_raffles_page_partial_last_page` — offset=3, limit=5, 7 raffles → items.len()==4, has_more=false
- `test_get_raffles_page_has_more_true` — offset=0, limit=3, 7 raffles → has_more=true
- `test_get_raffles_page_offset_equals_total` — offset==total → empty items, has_more=false (Req 2.3)
- `test_get_raffles_page_offset_beyond_total` — offset > total → empty items, has_more=false
- `test_get_raffles_page_limit_zero_defaults` — limit=0, 150 raffles → exactly 100 returned (Req 1.3)
- `test_get_raffles_page_limit_clamped` — limit=300, 250 raffles → exactly 200 returned (Req 1.4)
- `test_get_raffles_backward_compat` — get_raffles still returns all addresses (Req 4.1)
- `test_get_tickets_zero_tickets` — offset=0, limit=0, 0 tickets → `{ items:[], total:0, has_more:false }` (Req 5.4)
- `test_get_tickets_exact_page` — offset=0, limit=3, 3 tickets → items.len()==3, has_more=false
- `test_get_tickets_partial_last_page` — offset=2, limit=5, 4 tickets → items.len()==2, has_more=false
- `test_get_tickets_has_more_true` — offset=0, limit=2, 5 tickets → has_more=true
- `test_get_tickets_offset_equals_total` — offset==tickets_sold → empty items, has_more=false (Req 3.3)
- `test_get_tickets_ascending_order` — verify ticket_number is strictly increasing across returned items
- `test_get_tickets_limit_zero_defaults` — limit=0, 150 tickets → exactly 100 returned (Req 1.5)
- `test_get_tickets_limit_clamped` — limit=300, 250 tickets → exactly 200 returned (Req 1.6)
