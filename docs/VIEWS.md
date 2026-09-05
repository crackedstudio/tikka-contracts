# Views — Read-Only Query Surface

**Audience:** Integrators, Frontend Developers

All read-only query functions for the RaffleFactory contract are extracted into
`contracts/raffle-factory/src/views.rs`. This separation makes one property
mechanically checkable: **no view mutates state**.

## Pagination

Paginated views use [`raffle_shared::effective_limit`] to clamp the requested
`limit`:

| Requested | Actual |
|-----------|--------|
| `0` | `DEFAULT_PAGE_LIMIT` (100) |
| `1..=MAX_PAGE_LIMIT` | as-is |
| `> MAX_PAGE_LIMIT` | `MAX_PAGE_LIMIT` (200) |

Every paginated view returns `PageResultRaffles { items, total, has_more }`:

- `total` — number of live records (not tombstoned / cleaned).
- `has_more` — `true` when there are more records beyond the returned window.

## Query Surface

| Function | Returns | Auth | Notes |
|---|---|---|---|
| `get_protocol_stats` | `ProtocolStats` | None | Aggregate metrics: raffle count, fee BPS, paused, participants |
| `get_raffle_by_id` | `Option<Address>` | None | O(1) lookup by stable ID |
| `get_next_raffle_id` | `u32` | None | Next ID to be assigned |
| `predict_raffle_address` | `Address` | None | Precompute deterministic raffle address |
| `get_raffle_count` | `u32` | None | Live (non-tombstoned) raffle count |
| `get_total_volume` | `i128` | None | Cumulative ticket-sale volume per asset |
| `get_admin` | `Result<Address>` | None | Current admin address |
| `get_raffles_page` | `PageResultRaffles` | None | Paginated live raffles by stable ID |
| `get_raffles_by_creator` | `PageResultRaffles` | None | Paginated raffles for a creator |
| `get_raffles_by_category` | `PageResultRaffles` | None | Paginated raffles by category tag |
| `get_checkpoint` | `Option<StateCheckpoint>` | None | Periodic state snapshot by index |
| `get_latest_checkpoint_index` | `u32` | None | Index of most recent checkpoint |
| `get_unique_participants` | `u32` | None | Total unique participants |
| `get_raffle_fairness_data` | `Result<FairnessData>` | None | Fairness data for a raffle instance |

## Edge-Case Tests

The `tests/views.rs` module covers:

- **Offset beyond total** — returns empty `items`, correct `total`, `has_more: false`.
- **Limit = 0** — clamped to `DEFAULT_PAGE_LIMIT` via `effective_limit`.
- **Limit > MAX_PAGE_LIMIT** — clamped to `MAX_PAGE_LIMIT`.
- **No storage writes** — calling a view twice returns identical results.

## Extending

When adding a new read-only query:

1. Place it in `views.rs` inside the `#[contractimpl] impl RaffleFactory` block.
2. Ensure it reads storage only — no `env.storage().persistent().set(…)` or `.remove(…)`.
3. For paginated results, use `paginate_raffles()` or apply `effective_limit()`.
4. Add edge-case tests to `tests/views.rs`.
5. Update the table above.
