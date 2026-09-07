# Storage Layout and TTL Policy

Canonical map of Soroban storage keys for **raffle-factory** and **raffle-instance**: tier (instance vs persistent), write pattern, archival risk, and operator TTL guidance.

> Soroban deletes expired entries permanently. These contracts **do** call `extend_ttl` on-chain in hot paths (`buy_tickets`, `finalize_raffle`) and expose a permissionless `extend_ttl()` entrypoint on every raffle instance so anyone can keep a raffle alive. Operators may still bump TTLs externally (Stellar CLI / cron) for added resilience. See also the TTL section in [DEVELOPMENT.md](DEVELOPMENT.md).

## Storage tiers (quick reference)

| Tier | TTL model | Used when |
|---|---|---|
| **Instance** | One TTL for the whole contract instance entry | Hot operational flags and the live `Raffle` blob |
| **Persistent** | Per-key TTL | Registry indexes, tickets, fairness audit data, factory config |
| **Temporary** | Short-lived | **Not used** by either contract today |

Approximate ledger timing on public networks: **~5 seconds per ledger**. Default persistent minimum TTL is on the order of **~120 days**; plan bumps well before expiry.

---

## RaffleFactory (`contracts/raffle-factory/src/lib.rs`)

Source of truth: `DataKey` enum.

| DataKey | Tier | Written | Consensus-critical? | Archive-safe? | Notes |
|---|---|---|---|---|---|
| `Initialized` | Persistent | Once (`init_factory`) | Yes | No | Guards single init |
| `Admin` | Persistent | Once + on admin transfer accept | Yes | No | Loss = permanent lockout |
| `InstanceWasmHash` | Persistent | Init + timelocked upgrade ops | Yes | No | Required to deploy instances |
| `ProtocolFeeBP` | Persistent | Init + executed `PendingOp` | Yes | No | Fee policy |
| `Treasury` | Persistent | Init + executed `PendingOp` | Yes | No | Fee recipient |
| `Paused` | **Instance** | Updated on pause/unpause | Yes | No | Absent ⇒ not paused |
| `PendingAdmin` | Persistent | Set on propose; removed on accept/cancel | Yes (while pending) | No while pending | Two-step admin transfer |
| `PendingOp(u32)` | Persistent | Per proposed op; removed on execute/cancel | Yes while pending | After execute/cancel OK | Timelock payload (`TIMELOCK_DELAY_SECONDS` = 172800) |
| `OpCounter` | Persistent | Incremented per propose | Yes | No | Allocates op IDs |
| `RaffleById(u32)` | Persistent | On `create_raffle`; removed on tombstone | Yes | No for live IDs | O(1) stable ID → address map |
| `NextRaffleId` | Persistent | Incremented on create | Yes | No | Next stable ID (never decremented) |
| `RaffleCount` | Persistent | Updated on create/tombstone | Operational | Prefer keep | Live (non-tombstoned) count |
| `TotalRafflesCreated` | Persistent | Incremented on create | Audit | Prefer keep | Cumulative creations |
| `CreatorRaffles(Address)` | Persistent | Appended on create | Index | Prefer keep | Per-creator list |
| `CategoryRaffles(String)` | Persistent | Appended when config has category | Index | Prefer keep | Category filter index (#439) |
| `UniqueParticipant(Address)` | Persistent | Once per address | Analytics | Soft | First-seen flag |
| `TotalUniqueParticipants` | Persistent | Incremented with unique | Analytics | Soft | Aggregate |
| `MinCreationDelay` | Persistent | Admin config | Yes if rate-limit on | Prefer keep | Creation rate limit |
| `LastCreationTime(Address)` | Persistent | Per successful create | Yes if rate-limit on | Soft after delay window | Per-creator timestamp |
| `WhitelistedPartner(Address)` | Persistent | Admin set | Yes if whitelist used | Prefer keep | Partner bypass / privileges |
| `TotalVolumePerAsset(Address)` | Persistent | Updated on volume record | Analytics | Soft | Cumulative volume |
| `Checkpoint(u32)` | Persistent | Every `CHECKPOINT_INTERVAL` (1000) raffles | Audit | Soft historically | Periodic snapshot |
| `LatestCheckpointIndex` | Persistent | With checkpoint write | Audit | Prefer keep | Points at latest checkpoint |
| `RaffleInstancesCount` | Persistent | Test path only | Test | N/A | Address generation under `cfg(test)` |

### Factory write patterns

- **Write-once (effectively):** `Initialized`, first `Admin` / `InstanceWasmHash` / `ProtocolFeeBP` / `Treasury` (later changes go through timelock or transfer flows).
- **Per-raffle create:** `RaffleById`, `NextRaffleId`, `RaffleCount`, `TotalRafflesCreated`, `CreatorRaffles`, optional `CategoryRaffles`, rate-limit + volume keys.
- **Per admin action:** `Paused`, `PendingAdmin`, `PendingOp` / `OpCounter`, partner/delay config.

### Factory archival guidance

| Keep forever (operator must bump) | Can expire only after lifecycle ends |
|---|---|
| `Admin`, `Initialized`, `InstanceWasmHash`, `Treasury`, `ProtocolFeeBP`, `NextRaffleId`, live `RaffleById(*)` | Historical `Checkpoint(*)` (audit degradation only), stale `LastCreationTime`, old tombstoned indexes |

---

## RaffleInstance (`contracts/raffle-instance/src/lib.rs`)

Source of truth: `DataKey` enum (and admin-cancel flow keys noted below).

| DataKey | Tier | Written | Consensus-critical? | Archive-safe? | Notes |
|---|---|---|---|---|---|
| `Raffle` | **Instance** | Init; updated every lifecycle tx | Yes | No until fully claimed/cleaned | Full raffle state (status, winners, fees, …) |
| `Factory` | **Instance** | Once at `init` | Yes | No while live | Parent factory address |
| `Admin` | **Instance** (primary); also cleared from persistent on cleanup | Init / admin update paths | Yes | No | Authorization |
| `Paused` | **Instance** | Pause / unpause | Yes | No | Instance circuit breaker |
| `ReentrancyGuard` | **Instance** | Set around guarded calls; removed after | Transient | Yes after call | Must not stick `true` across txs |
| `DrawingLock` | **Instance** | Set on draw start; cleared on complete/fallback/cancel | Yes during draw | Yes after clear | Single-owner draw guard |
| `RandomnessRequested` | **Instance** | External draw request | Yes while pending | Yes after resolve | Oracle pending flag |
| `RandomnessRequestLedger` | **Instance** | With request | Yes while pending | Yes after resolve | Timeout base (`ORACLE_TIMEOUT_LEDGERS` = 200) |
| `RandomnessRequestId` | **Instance** | With request (when ID allocated) | Yes while pending | Yes after resolve | Correlates oracle callback |
| `AccumulatedFees` | **Instance** | Ticket buys / fee withdraw | Yes | No while balance > 0 | Escrowed protocol fees |
| `FinishTime` | **Instance** | Lifecycle helper / cleanup target | Operational | After terminal state | Present in enum; extend with instance TTL |
| `RandomnessSeed` | **Persistent** (fairness metadata); readers may also check instance in older paths | On successful finalize | Audit / dispute | Prefer keep after finalize | `FairnessMetadata` / seed fingerprint for `get_fairness_data` |
| `Ticket(u32)` | **Persistent** | Per purchased ticket | Yes | No until refunds/claims done | Owner + purchase metadata |
| `TicketCount(Address)` | **Persistent** | Updated per buy | Yes | No until cleanup | Per-buyer counts |
| `OwnerTickets(Address)` | **Persistent** | Reserved; not currently written | No | N/A | Intended owner → ticket IDs index |
| `TicketBuyers` | **Persistent** | Appended when buyer first appears | Yes | No until cleanup | Buyer enumeration for cleanup/refunds |
| `TicketRefunded(u32)` | **Persistent** | Once per refunded/claimed ticket | Yes | After full settlement | Idempotency marker |
| `CommitEntry(u32)` | **Persistent** | `submit_commit` (CommitReveal mode) | Yes until draw | After finalize OK | Hash keyed by **ticket ID** (survives transfer) |

### `Winner` entry (within the `Raffle` blob)

`Raffle.winners` is a `Vec<Winner>` holding one entry per prize tier in draw
order. It is a unified winner record that replaces the old parallel-array
pattern (`winners: Vec<Address>` + `claimed_winners: Vec<bool>`). Each entry is
written at finalization, and its `claimed` flag flips true on claim or sweep:

| Field          | Type     | Meaning                                                                 |
|----------------|----------|-------------------------------------------------------------------------|
| `address`      | `Address`| Address that owned the winning ticket at draw time.                     |
| `claimed`      | `bool`   | True once this tier's prize has been paid out or swept.                 |
| `tier_index`   | `u32`    | Index into `Raffle::prizes` identifying the tier won.                   |

`tier_index` names the tier consistently with `WinnerDrawn`, `PrizeClaimed`,
`PrizeSwept`, and `claim_prize`. The type is defined in `raffle-shared` with
`#[contracttype]` so it can be shared across contracts.

### Admin-cancel timelock key

Admin cancellation scheduling (`execute_admin_cancel` / related flows) stores a unlock timestamp under **instance** storage as `PendingAdminCancel` (u64). Treat it as consensus-critical while a cancel is scheduled; remove after execution. If your checkout’s `DataKey` enum is mid-refactor, keep this key’s tier/lifetime aligned with those call sites.

### Instance write patterns

| Pattern | Keys |
|---|---|
| Once at init | `Factory`, `Admin`, initial `Raffle` |
| Every ticket purchase | `Ticket`, `TicketCount`, `OwnerTickets`, `TicketBuyers`, `Raffle`, maybe `AccumulatedFees` |
| Commit-reveal | `CommitEntry(ticket_id)` once per commit |
| Draw / oracle | `DrawingLock`, `RandomnessRequested`, `RandomnessRequestLedger`, `RandomnessRequestId` |
| Finalize | `Raffle` (winners/status), `RandomnessSeed` |
| Claim / refund | `TicketRefunded`, `Raffle.claimed_winners` |
| Transient | `ReentrancyGuard` |

### Instance archival guidance

| Must not archive while… | Safer to drop only after… |
|---|---|
| `Raffle` / instance TTL — Active, Drawing, Finalized with unclaimed prizes | `Claimed`, `Cancelled`, or `Failed` **and** cleanup |
| Any `Ticket(*)` / `OwnerTickets` / `TicketCount` — sales or refunds open | Explicit cleanup / all refunds processed |
| `RandomnessSeed` — disputes possible | Policy retention window post-finalize |
| Oracle pending keys | `provide_randomness` or fallback completes |

### `wipe_storage`

The `wipe_storage` entrypoint is an authenticated factory operation for
reclaiming settlement-era storage. It is allowed only when the raffle status
is `Claimed`, `Cancelled`, or `Failed`, and the instance balance is zero for
both `payment_token` and `prize_token`. A non-zero balance rejects the call so
that escrowed funds cannot become unreachable.

On success it removes ticket records, refund markers, commit-reveal entries,
buyer and owner-ticket indexes, quorum randomness entries, and transient
lifecycle keys. It retains `Raffle`, `Factory`, `Admin`, and
`RandomnessSeed`; these remain required by read, attestation, fairness, and
privileged paths. The operation emits `StorageWiped` for indexers.

---

## Recommended TTL bump policy

| Contract | Extend instance by | Re-bump cadence | Priority persistent keys |
|---|---|---|---|
| Factory (long-lived) | ~1 year (`6220800` ledgers) | Every ~6 months | `Admin`, `InstanceWasmHash`, `Treasury`, `ProtocolFeeBP`, `NextRaffleId`, live `RaffleById` |
| Instance (per raffle) | ~6 months (`3110400` ledgers) while Active/Drawing/Finalized | Monthly for open raffles | All `Ticket(*)`, `TicketCount`, `OwnerTickets`, `CommitEntry`, `RandomnessSeed` |

Example (factory instance TTL):

```bash
stellar contract extend \
  --id <FACTORY_CONTRACT_ADDRESS> \
  --ledgers-to-extend 6220800 \
  --network <NETWORK> \
  --source-account <OPERATOR_KEY>
```

Persistent keys need `--durability persistent` and `--key …` (or batched tooling). Ticket keys are **independent** of the instance TTL — bumping only the instance is not enough for long-running sales.

## Tombstone model for `clean_old_raffle`

`clean_old_raffle` removes a raffle from the stable map without shifting IDs:

- `RaffleById(id)` is removed — the slot becomes a tombstone.
- `NextRaffleId` is never decremented — IDs are never reused.
- `RaffleCount` is decremented — this is the count of **live** raffles.
- `CreatorRaffles` and `CategoryRaffles` are pruned — tombstones do not appear in per-creator or per-category views.

### What `total` means in pagination

- `get_raffles_page` returns `total = RaffleCount` (live raffles only).
- `get_raffles_by_creator` returns `total = creator_vec.len()` (after pruning).
- `get_raffles_by_category` returns `total = category_vec.len()` (after pruning).

### Pagination semantics over a sparse ID space

`get_raffles_page` collects all live raffles by scanning `RaffleById(0..NextRaffleId)`, skipping tombstones, then applies `offset` and `limit` on the dense live list. This guarantees:

- No gaps or repeats across pages.
- `has_more` is consistent with `total`.
- Tombstoned raffles are invisible to every query path.

## Related docs

- [DEPLOYMENT.md](DEPLOYMENT.md) — deploy then operate
- [RANDOMNESS.md](RANDOMNESS.md) — how draw keys are used
- [ARCHITECTURE.md](ARCHITECTURE.md) — lifecycle states
- [COMMIT_REVEAL.md](COMMIT_REVEAL.md) — `CommitEntry` protocol

## Raffle TTL Management

### Instance TTL
The raffle instance entry is bumped on every `buy_tickets` and `finalize_raffle` call via `bump_raffle_ttl()`.

- **Threshold**: `INSTANCE_TTL_THRESHOLD_LEDGERS` (1,555,200 ledgers / ~3 months)
- **Bump to**: `INSTANCE_TTL_BUMP_LEDGERS` (3,110,400 ledgers / ~6 months)
- **Frequency**: Bumped unconditionally on every purchase

### Ticket TTL (Amortised Bumping)
Ticket entries are NOT bumped all at once to avoid exceeding the Soroban resource budget.

- **Window size**: 100 tickets per call
- **Strategy**: Each `bump_raffle_ttl` call bumps the next `WINDOW_SIZE` tickets
- **Cycle**: Once all tickets are bumped, the cycle resets to keep entries alive
- **Cost**: O(window_size) regardless of total tickets sold

### Why This Works
- Each purchase calls `bump_raffle_ttl`
- Over time, all tickets get bumped
- Tickets expire after ~6 months if not bumped
- Winners have plenty of time to claim their prizes
