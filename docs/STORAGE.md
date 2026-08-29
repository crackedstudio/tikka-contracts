# Storage & TTL Reference

This document is the **authoritative key→tier mapping** for every storage variant in both contracts. Every new `DataKey` variant added to a crate must be registered here, including:

- Which storage tier it uses (`Instance` / `Persistent` / `Temporary`).
- Semantics and expected lifecycle.
- TTL risk classification (catastrophic / degraded / low).
- Recommended TTL bump cadence for operators.

For the operator-facing TTL strategy, see `DEVELOPMENT.md` § "Storage Architecture & TTL Management".

---

## Key: Storage Tiers

| Tier | TTL behavior | Use in Tikka |
|---|---|---|
| **Instance** | All keys share one TTL, tied to the contract's own ledger entry. Extending the instance TTL extends every key inside it. | Operational state (Raffle struct, pause flags, admin, factory, reentrancy guards, fairness metadata). |
| **Persistent** | Each key has an independent TTL. Keys must be bumped individually or in batches. | Tickets, checkpoints, per-user records, the factory registry. |
| **Temporary** | Shortest TTL bucket (~1 day). Not used by Tikka. | N/A |

---

## RaffleFactory – `contracts/raffle/src/lib.rs`

| DataKey | Tier | Semantics | TTL risk | Bump priority |
|---|---|---|---|---|
| `Admin` | Persistent | Factory admin address. | **Catastrophic** – loss = permanent lockout of all admin-only functions. | #1 |
| `RaffleInstances` | Persistent | `Vec<Address>` of every deployed raffle instance. | **Catastrophic** – loss = registry erased, can no longer enumerate or clean up raffles. | #2 |
| `InstanceWasmHash` | Persistent | `BytesN<32>` WASM hash used by `deploy_v2` when creating a raffle instance. | **Catastrophic** – loss = cannot create new raffles. | #3 |
| `ProtocolFeeBP` | Persistent | Default protocol fee (basis points) applied to new raffles. | Degraded – loss = defaults to `0`, fee stream broken for new raffles until reset. | #4 |
| `Treasury` | Persistent | Address where protocol fees are swept. | Degraded – loss = factory `record_volume` returns `TreasuryNotSet`; new raffles can't be created. | #4 |
| `Paused` | **Instance** | Factory-level pause boolean (absent = `false`). | Low – kept alive by the instance TTL. | Included in factory instance bump |
| `PendingAdmin` | Persistent | Two-step admin transfer target. | Degraded – loss = in-flight admin transfer lost; must be re-initiated. | #6 |
| `PendingOp(u32)` | Persistent | Timelocked pending config changes (SetConfig / UpdateWasmHash). | Degraded – loss = op cannot execute; admin re-proposes via `set_config`. | #6 |
| `OpCounter` | Persistent | Monotonic counter for pending operations. | Low – loss restarts at 0; existing `PendingOp` IDs are still valid. | Low |
| `Checkpoint(u32)` | Persistent | Periodic state snapshots (hash + count). | Low – historical audit record; expiry only degrades provability. | As scheduled |
| `LatestCheckpointIndex` | Persistent | Index of the most recent checkpoint. | Low – syncs on next checkpoint creation. | Low |
| `TotalRafflesCreated` | Persistent | Cumulative count (used for checkpoints & stats). | Low – would resume from the last checkpoint or 0. | Low |
| `UniqueParticipant(Address)` | Persistent | Per-address flag for counting unique participants. | Degraded – expiry causes re-counting of already-counted users (inflated stats, not a security risk). | As batch |
| `TotalUniqueParticipants` | Persistent | Aggregate unique-participant count. | Degraded – same caveat as above. | Low |
| `MinCreationDelay` | Persistent | Rate-limit window for non-whitelisted creators (seconds). | Low – expiry resets to the built-in 300 s default. | Low |
| `LastCreationTime(Address)` | Persistent | Per-creator timestamp of last raffle create. | Low – expires naturally; creator just waits the delay again. | Low |
| `WhitelistedPartner(Address)` | Persistent | Bypasses creation rate limit when true. | Degraded – loss = partner is rate-limited again until re-whitelisted. | As batch |
| `TotalVolumePerAsset(Address)` | Persistent | Cumulative ticket volume in the native asset. | Low – analytics only. | Low |
| `RaffleInstancesCount` | Persistent | Test-only counter for deterministic test deployment. | N/A (test only) | N/A |

### Factory Recommended Bump Schedule

```
Bump interval: every 6 months, extend instance + top persistent keys by 1 year (6,220,800 ledgers).
Batch keys: UniqueParticipant / WhitelistedPartner / TotalVolumePerAsset / Checkpoint / LastCreationTime
  → bump on a rolling per-address cadence or whenever a new raffle touches them.
```

---

## RaffleInstance – `contracts/raffle-instance/src/lib.rs`

| DataKey | Tier | Semantics | TTL risk | Bump priority |
|---|---|---|---|---|
| `Raffle` | **Instance** | Full `Raffle` struct: status, winners, prize config, creator, claim data. | **Catastrophic** – wipes every operational field including winners; claim/cancel/emergency-withdraw all break. | #1 for every active instance |
| `Factory` | **Instance** | Back-reference to the factory contract address (for auth on pause/set_admin). | **Catastrophic** – pause/unpause/sync_admin/write_storage all blocked. | Included in instance bump |
| `Admin` | **Instance** | Synced factory admin; guards emergency_withdraw, set_protocol_fee_bp, rescue_tokens. | **Catastrophic** – loss = admin-only emergency pathways dead. | Included in instance bump |
| `Paused` | **Instance** | Instance-level pause flag. | Low – covered by instance TTL. | Included |
| `ReentrancyGuard` | **Instance** | Transient lock, cleared on function exit. | Low (transient). | N/A (not persisted across txs) |
| `RandomnessRequested` | **Instance** | Whether `provide_randomness` has been requested. | Degraded – expiry in Drawing state would allow a duplicate request to slip through (mitigated by RandomnessRequestLedger). | #2 for External draws |
| `RandomnessRequestLedger` | **Instance** | Ledger seq at request time (for fallback timeout calcs). | Degraded – expiry disables the `FallbackTooEarly` gate. | #2 for External draws |
| `RandomnessRequestId` | **Instance** | Replay-resistant request token. | Degraded – expiry disables request ID validation. | #2 for External draws |
| `RandomnessSeed` | **Instance** | Post-draw FairnessMetadata (seed, winning indices, sequence). | Degraded – loss = `get_fairness_data` fails, fairness audit trail broken. | #3 after finalization |
| `FinishTime` | **Instance** | Raffle finish timestamp (legacy / infrequently accessed). | Low. | Low |
| `AccumulatedFees` | **Instance** | Running fee balance available to `withdraw_fees`. | **Catastrophic for operators** – expiry loses accumulated fees; they remain in the contract but become invisible to the withdraw path. | #3 |
| `TicketCount(Address)` | **Persistent** | Per-buyer ticket count. | Degraded – expiry would allow a buyer holding multiple tickets already to evade `allow_multiple=false` again (stale data only). | Batch with tickets |
| `Ticket(u32)` | **Persistent** | Individual ticket record (owner, purchase_time, ticket_number). | **Catastrophic** – winner lookups depend on these; loss = winners unverifiable, refunds broken, claim blocked. | #2 per raffle for the entire active range |
| `TicketRefunded(u32)` | **Persistent** | Idempotency flag for `refund_ticket`. | Degraded – expiry could enable double-refund (mitigated by status gate; still, keep alive for the entire refund window). | Batch with tickets |

### Instance Recommended Bump Schedule

| Phase | Cadence | Target |
|---|---|---|
| Active (prize deposited, selling tickets) | Monthly | Extend instance TTL by 6 months; batch-bump `TicketCount/Address` + `Ticket(1..tickets_sold)`. |
| Drawing (awaiting randomness / oracle) | Weekly | Same, especially `RandomnessRequested` and the RequestLedger/Id. |
| Finalized (winners recorded, claims open) | Monthly until 100% claimed | Instance (Raffle, AccumulatedFees) and winning tickets. |
| Cancelled / Failed | Monthly until refund window closes | All `Ticket(u32)` and `TicketRefunded(u32)` keys; the instance tier can be allowed to lapse once all refunds are processed and `wipe_storage` invoked. |
| Claimed / Wiped | None | Ready for garbage collection via `clean_old_raffle` on the factory. |

---

## Adding a new key? Checklist

1. ✅ Pick the correct tier by cross-referencing:
   - Is it accessed on every hot path? → Prefer Instance so it bundles with the instance TTL.
   - Does it outlive the raffle/factory active phase? → Persistent.
   - Per-address / per-item and sparse? → Persistent.
2. ✅ Append a variant to the `DataKey` enum in the relevant crate's `lib.rs`.
3. ✅ Add a row to **this document** (`docs/STORAGE.md`) with all five columns.
4. ✅ If the risk is **Catastrophic**, file a companion issue/PR against operator tooling (scripts, docs/DEPLOYMENT.md) to include it in the default TTL bump loop.
5. ✅ If the key is Persistent and dense (e.g., `Ticket(u32)`), note the batch-bump strategy in the phase table above.
