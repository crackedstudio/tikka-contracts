# Raffle Contract Events

This document describes all events emitted by the Tikka raffle system, covering both the **Factory** contract (`contracts/raffle/src/events.rs`) and the **Raffle Instance** contract (`contracts/raffle-instance/src/events.rs`). Off-chain indexers, event listeners, and frontend developers use these events to reconstruct complete raffle state without querying contract storage.

## Event Topic Scheme

All events use a consistent two-symbol topic structure:

```
("tikka", "event_name")
```

Where:
- First symbol: `"tikka"` (constant namespace identifier)
- Second symbol: Event name in snake_case matching the struct name (e.g. `ticket_purchased`, `raffle_created`)

---

# Factory Events

## FactoryInitialized

Emitted when the factory contract is initialized for the first time.

| Field | Type | Description |
|-------|------|-------------|
| `admin` | `Address` | Initial admin address with privileged access |
| `protocol_fee_bp` | `u32` | Protocol fee in basis points (100 = 1%) |
| `treasury` | `Address` | Treasury address that receives protocol fee payouts |
| `timestamp` | `u64` | Ledger timestamp of initialization |

**Emitted by:** `init_factory`
**When:** Factory is successfully initialized for the first time (admin, wasm_hash, protocol_fee, treasury are set).

---

## RaffleInstanceDeployed

Emitted when the factory deploys a new raffle instance contract.

| Field | Type | Description |
|-------|------|-------------|
| `instance` | `Address` | Address of the deployed raffle instance contract |
| `wasm_hash` | `BytesN<32>` | Hash of the WASM bytecode deployed for the instance |
| `creator` | `Address` | Address that requested the raffle creation |
| `timestamp` | `u64` | Ledger timestamp of deployment |

**Emitted by:** `create_raffle`
**When:** Immediately after the instance contract is deployed via Soroban's `create_contract`.
**Note:** This event is marked `#[allow(dead_code)]` — the factory currently uses `RaffleCreated` from the instance directly for lifecycle tracking.

---

## CheckpointCreated

Emitted periodically to create a verifiable state checkpoint of all tracked raffles.

| Field | Type | Description |
|-------|------|-------------|
| `index` | `u32` | Sequential checkpoint index (increments each interval) |
| `raffle_count` | `u32` | Number of raffle instances tracked at this checkpoint |
| `ledger_timestamp` | `u64` | Ledger timestamp when checkpoint was created |
| `aggregate_hash` | `BytesN<32>` | Aggregate hash of raffle state (enables off-chain verification) |

**Emitted by:** `maybe_create_checkpoint` (helper, called from `create_raffle`)
**When:** Every `CHECKPOINT_INTERVAL` (1000) raffles created, providing cumulative state snapshots.

---

## SupportedSacUpdated

Emitted when a Stellar Asset Contract (SAC) token's support status is updated.

| Field | Type | Description |
|-------|------|-------------|
| `token` | `Address` | Address of the token contract |
| `supported` | `bool` | Whether the token is now accepted for raffle creation (`true`) or not (`false`) |
| `updated_by` | `Address` | Address that performed the update |
| `timestamp` | `u64` | Ledger timestamp of the update |

**Emitted by:** (dead code — defined but never emitted in current implementation)
**When:** Admin toggles a SAC token's supported status in the factory.

---

## RaffleCleanedUp

Emitted when a finished raffle instance's storage is cleaned up from the factory.

| Field | Type | Description |
|-------|------|-------------|
| `raffle_address` | `Address` | Address of the cleaned-up raffle instance |
| `cleaned_by` | `Address` | Address that performed the cleanup |
| `finish_time` | `u64` | Timestamp when the raffle originally finished |
| `cleaned_at` | `u64` | Ledger timestamp of the cleanup action |

**Emitted by:** `clean_old_raffle`
**When:** Admin wipes storage of a completed/cancelled/failed raffle instance and removes it from the tracked instances list.

---

## CreationRateLimited

Emitted when a creator is rate-limited from creating new raffles.

| Field | Type | Description |
|-------|------|-------------|
| `creator` | `Address` | Address of the rate-limited creator |
| `unlock_timestamp` | `u64` | Timestamp when the rate limit expires and creation is allowed again |
| `timestamp` | `u64` | Ledger timestamp of the rate-limit event |

**Emitted by:** `create_raffle`
**When:** A non-whitelisted creator attempts to create a raffle before the per-creator cooldown period has elapsed. Emitted on the error path.

---

## AdminOpProposed

Emitted when a new admin operation is proposed through the timelock mechanism.

| Field | Type | Description |
|-------|------|-------------|
| `op_id` | `u32` | Unique operation identifier (auto-incremented) |
| `op` | `AdminOp` | The proposed admin operation: `SetConfig(u32, Address)` (fee_bp + treasury) or `UpdateWasmHash(BytesN<32>)` |
| `effective_timestamp` | `u64` | Timestamp when the operation becomes executable (after timelock delay) |
| `proposed_by` | `Address` | Address that proposed the operation |

**Emitted by:** `set_config`
**When:** Admin proposes a config change. The operation is stored with a timelock before it can be executed.

---

## AdminOpExecuted

Emitted when a previously proposed admin operation is executed.

| Field | Type | Description |
|-------|------|-------------|
| `op_id` | `u32` | Operation identifier matching the proposed operation |
| `op` | `AdminOp` | The admin operation that was executed |
| `executed_by` | `Address` | Address that executed the operation |
| `executed_at` | `u64` | Ledger timestamp of execution |

**Emitted by:** `execute_config_change`
**When:** A pending admin operation is executed after the timelock delay has elapsed.

---

## AdminOpCancelled

Emitted when a proposed admin operation is cancelled before execution.

| Field | Type | Description |
|-------|------|-------------|
| `op_id` | `u32` | Operation identifier of the cancelled operation |
| `cancelled_by` | `Address` | Address that cancelled the operation |
| `cancelled_at` | `u64` | Ledger timestamp of cancellation |

**Emitted by:** `cancel_config_change`
**When:** Admin cancels a pending config change before it takes effect.

---

## TreasuryChanged

Emitted when the factory-level treasury address is changed.

| Field | Type | Description |
|-------|------|-------------|
| `old_treasury` | `Address` | Previous treasury address |
| `new_treasury` | `Address` | New treasury address |
| `changed_by` | `Address` | Address that authorized the change (indexed topic) |
| `timestamp` | `u64` | Ledger timestamp of the change |

**Emitted by:** (dead code — `TreasuryChanged` is defined but the treasury update path uses `AdminOpExecuted` + `SetConfig` instead)
**When:** Treasury address is changed via an executed admin operation. The factory's actual behavior emits `AdminOpProposed` → `AdminOpExecuted` with a `SetConfig` payload.

---

## AdminTransferProposed

Emitted when a factory admin transfer is proposed to a new address.

| Field | Type | Description |
|-------|------|-------------|
| `current_admin` | `Address` | Current admin address |
| `proposed_admin` | `Address` | Address proposed to become the new admin |
| `timestamp` | `u64` | Ledger timestamp of the proposal |

**Emitted by:** `transfer_factory_admin`
**When:** Admin proposes a new admin address (single-pending, no timelock).

---

## AdminTransferAccepted

Emitted when a proposed admin accepts the admin transfer.

| Field | Type | Description |
|-------|------|-------------|
| `old_admin` | `Address` | Previous admin address |
| `new_admin` | `Address` | New admin address that accepted the role |
| `timestamp` | `u64` | Ledger timestamp of acceptance |

**Emitted by:** `accept_factory_admin`
**When:** The proposed admin address calls this function to accept the admin role.

---

## AdminTransferFailed

Emitted when an admin transfer proposal fails.

| Field | Type | Description |
|-------|------|-------------|
| `current_admin` | `Address` | Current admin address at time of failure |
| `proposed_admin` | `Address` | Address that was proposed as new admin |
| `reason_code` | `u32` | Numeric reason code for the failure |
| `timestamp` | `u64` | Ledger timestamp of the failure |

**Emitted by:** (dead code — defined but never emitted in current implementation)
**When:** An admin transfer proposal cannot be completed.

---

## ContractPaused (Factory)

Emitted when the factory contract is paused.

| Field | Type | Description |
|-------|------|-------------|
| `paused_by` | `Address` | Address that paused the contract |
| `timestamp` | `u64` | Ledger timestamp of the pause |

**Emitted by:** `pause_factory`
**When:** Admin pauses the factory contract, preventing new raffle creation.

---

## ContractUnpaused (Factory)

Emitted when the factory contract is unpaused.

| Field | Type | Description |
|-------|------|-------------|
| `unpaused_by` | `Address` | Address that unpaused the contract |
| `timestamp` | `u64` | Ledger timestamp of the unpause |

**Emitted by:** `unpause_factory`
**When:** Admin unpauses the factory contract, restoring raffle creation capability.

---

## FactoryTokensRescued

Emitted when accidentally-sent tokens are rescued from the factory.

| Field | Type | Description |
|-------|------|-------------|
| `rescued_by` | `Address` | Address that rescued the tokens |
| `token` | `Address` | Address of the rescued token contract |
| `recipient` | `Address` | Address receiving the rescued tokens |
| `amount` | `i128` | Amount of tokens rescued |
| `timestamp` | `u64` | Ledger timestamp of the rescue |

**Emitted by:** `rescue_tokens`
**When:** Admin rescues tokens that were accidentally sent to the factory contract (cannot sweep the tracked prize/escrow tokens).

---

## FactoryUpgraded

Emitted when the factory contract's WASM code is upgraded.

| Field | Type | Description |
|-------|------|-------------|
| `admin` | `Address` | Admin address that performed the upgrade |
| `new_wasm_hash` | `BytesN<32>` | Hash of the new WASM contract code |
| `timestamp` | `u64` | Ledger timestamp of the upgrade |

**Emitted by:** `upgrade`
**When:** Admin upgrades the factory contract to a new WASM implementation.

---

# Raffle Instance Events

## RaffleCreated

Emitted when a new raffle instance is initialized with its configuration.

| Field | Type | Description |
|-------|------|-------------|
| `raffle_id` | `Address` | Address of the raffle instance contract |
| `creator` | `Address` | Address of the raffle creator |
| `end_time` | `u64` | Timestamp when ticket sales close (0 for no time limit) |
| `max_tickets` | `u32` | Maximum number of tickets available for sale |
| `ticket_price` | `i128` | Price per ticket in stroops of `payment_token` |
| `payment_token` | `Address` | Address of the token contract used for payments |
| `prize_amount` | `i128` | Total amount the creator must deposit as the prize pool |
| `prizes` | `Vec<u32>` | Prize tier distribution — each element is the number of winning positions for that tier |
| `description` | `String` | Human-readable raffle description |
| `randomness_source` | `RandomnessSource` | Randomness source: `Internal = 0`, `External = 1`, `CommitReveal = 2` |
| `metadata_hash` | `BytesN<32>` | Hash of off-chain metadata (indexed topic) |

**Emitted by:** `init`
**When:** A new raffle instance is initialized with the given `RaffleConfig`. Raffle status becomes `PendingPrize`.

---

## RaffleStatusChanged

Emitted whenever the raffle status transitions between lifecycle states.

| Field | Type | Description |
|-------|------|-------------|
| `old_status` | `RaffleStatus` | Previous raffle status |
| `new_status` | `RaffleStatus` | New raffle status |
| `timestamp` | `u64` | Ledger timestamp of the transition |

**RaffleStatus values:**
| Value | Name | Description |
|-------|------|-------------|
| `6` | `PendingPrize` | Raffle created, awaiting prize deposit |
| `0` | `Active` | Prize deposited, accepting ticket purchases |
| `1` | `Drawing` | Ticket sales ended, winner selection in progress |
| `2` | `Finalized` | Winners determined, awaiting claims |
| `5` | `Claimed` | All prizes claimed |
| `3` | `Cancelled` | Raffle cancelled before finalization |
| `4` | `Failed` | Raffle failed (e.g. zero tickets sold) |

**State machine:** `Drawing` transitions atomically to `Finalized` once winner selection completes; there is no intermediate `Finalizing` state.

```mermaid
stateDiagram-v2
    [*] --> PendingPrize
    PendingPrize --> Active: deposit_prize
    Active --> Drawing: sales end or sell-out
    Drawing --> Finalized: finalize_raffle
    Active --> Failed: end with zero tickets
    PendingPrize --> Cancelled: cancel_raffle
    Active --> Cancelled: cancel_raffle
    Drawing --> Cancelled: cancel_raffle
    Finalized --> Claimed: claim_prize
```

**Emitted by:** `deposit_prize`, `buy_tickets` (via `transition_to_drawing`), `finalize_raffle`, `claim_prize`
**When:** Status changes between lifecycle states (e.g. `PendingPrize` → `Active`, `Active` → `Drawing`, `Drawing` → `Finalized`, `Finalized` → `Claimed`).

---

## PrizeDeposited

Emitted when the creator deposits the prize pool into the raffle contract.

| Field | Type | Description |
|-------|------|-------------|
| `creator` | `Address` | Address that deposited the prize |
| `amount` | `i128` | Amount of tokens deposited |
| `token` | `Address` | Address of the deposited token contract |
| `timestamp` | `u64` | Ledger timestamp of the deposit |

**Emitted by:** `deposit_prize`
**When:** The creator transfers the prize amount into the contract. Raffle transitions from `PendingPrize` to `Active`.

---

## PrizeRefunded

Emitted when the deposited prize is refunded back to the creator.

| Field | Type | Description |
|-------|------|-------------|
| `creator` | `Address` | Address receiving the refund |
| `amount` | `i128` | Amount of tokens refunded |
| `token` | `Address` | Address of the refunded token contract |
| `timestamp` | `u64` | Ledger timestamp of the refund |

**Emitted by:** `refund_prize`
**When:** After a raffle is cancelled or failed, the creator withdraws the deposited prize back. Requires raffle to be in `Cancelled` or `Failed` status.

---

## TicketPurchased

Emitted when a buyer successfully purchases one or more tickets.

| Field | Type | Description |
|-------|------|-------------|
| `buyer` | `Address` | Address that purchased the tickets |
| `ticket_ids` | `Vec<u32>` | List of ticket IDs assigned (1-indexed, sequential within this purchase) |
| `quantity` | `u32` | Number of tickets purchased in this transaction |
| `ticket_price` | `i128` | Price per ticket in stroops of `payment_token` |
| `total_paid` | `i128` | Total amount transferred from buyer (`ticket_price × quantity`) |
| `protocol_fee` | `i128` | Amount immediately sent to treasury as protocol fee |
| `timestamp` | `u64` | Ledger timestamp of the purchase |

**Emitted by:** `buy_tickets`
**When:** After successful token transfer from buyer to contract, ticket records written, and state committed. Raffle must be in `Active` status with ticket sales not paused.

---

## TicketTransferred

Emitted when a ticket is transferred from one address to another.

| Field | Type | Description |
|-------|------|-------------|
| `ticket_id` | `u32` | ID of the transferred ticket |
| `from` | `Address` | Previous owner address |
| `to` | `Address` | New owner address |
| `timestamp` | `u64` | Ledger timestamp of the transfer |

**Emitted by:** (dead code — defined but never emitted in current implementation)
**When:** A ticket holder transfers their ticket to another address.

---

## DrawTriggered

Emitted when the draw process is initiated for a raffle.

| Field | Type | Description |
|-------|------|-------------|
| `caller` | `Address` | Address that initiated the draw |
| `total_tickets_sold` | `u32` | Total number of tickets sold at the time of draw |
| `timestamp` | `u64` | Ledger timestamp when the draw was triggered |

**Emitted by:** `buy_tickets`, `finalize_raffle`
**When:** The raffle enters the drawing phase — either because the last ticket was sold (via `buy_tickets`) or because `finalize_raffle` is called explicitly for `Internal`/`External`/`CommitReveal` randomness.

---

## RandomnessRequested

Emitted when external randomness is requested from an oracle.

| Field | Type | Description |
|-------|------|-------------|
| `oracle` | `Address` | Address of the oracle contract providing randomness |
| `request_id` | `u64` | Oracle-specific request identifier for correlating the response |
| `timestamp` | `u64` | Ledger timestamp of the request |

**Emitted by:** `buy_tickets`, `finalize_raffle`
**When:** The last ticket is sold (external randomness mode) and an oracle request is dispatched, or when `finalize_raffle` is called with `External` randomness and the oracle request is dispatched.

---

## RandomnessReceived

Emitted when randomness is successfully received from the oracle.

| Field | Type | Description |
|-------|------|-------------|
| `oracle` | `Address` | Address of the oracle contract that provided randomness |
| `seed` | `u64` | Random seed value provided by the oracle |
| `request_id` | `u64` | Oracle request identifier matching the original request |
| `timestamp` | `u64` | Ledger timestamp when randomness was received |

**Emitted by:** `provide_randomness`
**When:** The oracle provides a valid VRF seed with correct proof and matching `request_id`.

---

## RandomnessFallbackTriggered

Emitted when the fallback randomness path is used due to oracle timeout.

| Field | Type | Description |
|-------|------|-------------|
| `triggered_by` | `Address` | Address that triggered the fallback |
| `seed_used` | `u64` | Fallback seed value used for winner selection (derived internally) |
| `request_ledger` | `u32` | Ledger sequence when randomness was originally requested |
| `fallback_ledger` | `u32` | Ledger sequence when the fallback was triggered |
| `timestamp` | `u64` | Ledger timestamp of the fallback |

**Emitted by:** `trigger_randomness_fallback`
**When:** The oracle timeout has elapsed and the fallback path is taken (with `do_refund = false`); an internal seed is used to finalize the raffle.

---

## RaffleFinalized

Emitted when the raffle is finalized with winners selected.

| Field | Type | Description |
|-------|------|-------------|
| `raffle_id` | `Address` | Address of the raffle instance |
| `winners` | `Vec<Address>` | Addresses of the winners, in order of prize tiers |
| `winning_ticket_ids` | `Vec<u32>` | Ticket IDs selected for each prize tier (parallel to `winners`) |
| `total_tickets_sold` | `u32` | Total tickets sold in this raffle |
| `randomness_source` | `RandomnessSource` | Randomness channel used: `Internal = 0`, `External = 1`, `CommitReveal = 2` |
| `randomness_type` | `RandomnessType` | Exact draw method: `Prng = 0`, `Vrf = 1`, `Fallback = 2` |
| `finalized_at` | `u64` | Ledger timestamp of finalization |

**Emitted by:** `do_finalize_with_seed` (helper)
**When:** After winners are selected, state is committed, and the drawing lock is released. Raffle status becomes `Finalized`.

---

## WinnerDrawn

Emitted for each winner drawn during finalization (one event per tier).

| Field | Type | Description |
|-------|------|-------------|
| `winner` | `Address` | Address of the winning participant |
| `ticket_id` | `u32` | ID of the winning ticket |
| `tier_index` | `u32` | Prize tier index (0-based, in order of the `prizes` array from `RaffleCreated`) |
| `timestamp` | `u64` | Ledger timestamp of the draw |

**Emitted by:** `do_finalize_with_seed` (helper)
**When:** For each winner tier during finalization — emitted once per winner selected.

---

## RaffleCancelled

Emitted when a raffle is cancelled.

| Field | Type | Description |
|-------|------|-------------|
| `creator` | `Address` | Address that cancelled the raffle (creator or admin) |
| `reason` | `CancelReason` | Reason for cancellation: `CreatorCancelled = 0`, `AdminCancelled = 1`, `OracleTimeout = 2`, `MinTicketsNotMet = 3` |
| `tickets_sold` | `u32` | Number of tickets sold before cancellation |
| `prize_refunded` | `bool` | Whether the deposited prize was already refunded |
| `timestamp` | `u64` | Ledger timestamp of cancellation |

**Emitted by:** `cancel_raffle`, `trigger_randomness_fallback`
**When:** The creator or admin cancels the raffle, or when fallback triggers with `do_refund = true` (OracleTimeout). Raffle status becomes `Cancelled`.

---

## RaffleFailed

Emitted when a raffle fails (e.g. insufficient participation).

| Field | Type | Description |
|-------|------|-------------|
| `creator` | `Address` | Address of the raffle creator |
| `reason` | `FailureReason` | Reason for failure: `ZeroTicketsSold = 0`, `MinTicketsNotMet = 1` |
| `tickets_sold` | `u32` | Number of tickets sold before failure |
| `timestamp` | `u64` | Ledger timestamp of failure |

**Emitted by:** `finalize_raffle`
**When:** Zero tickets were sold (`ZeroTicketsSold`) or tickets sold < minimum required (`MinTicketsNotMet`). Raffle status becomes `Failed`.

---

## PrizeClaimed

Emitted when a verified winner claims their prize.

| Field | Type | Description |
|-------|------|-------------|
| `winner` | `Address` | Address of the winner claiming the prize |
| `tier_index` | `u32` | Prize tier index being claimed (0-based) |
| `payment_token` | `Address` | Token contract used for the payout |
| `gross_amount` | `i128` | Total prize amount before any deductions |
| `net_amount` | `i128` | Amount transferred to the winner (equals gross; fees are charged at ticket purchase) |
| `platform_fee` | `i128` | Always 0 — protocol fees are collected on ticket sales, not prize claims |
| `claimed_at` | `u64` | Ledger timestamp of the claim |

**Emitted by:** `claim_prize`
**When:** A verified winner claims their prize after any claim lockup period has elapsed. If this is the last unclaimed prize tier, raffle status becomes `Claimed`.

---

## TicketRefunded

Emitted when a ticket holder receives a refund after cancellation or failure.

| Field | Type | Description |
|-------|------|-------------|
| `buyer` | `Address` | Address receiving the refund |
| `ticket_number` | `u32` | Number of the refunded ticket |
| `amount` | `i128` | Refund amount (original ticket price) |
| `timestamp` | `u64` | Ledger timestamp of the refund |

**Emitted by:** `refund_ticket`
**When:** After a raffle is cancelled or failed, a ticket holder gets their ticket price refunded.

---

## FeesWithdrawn

Emitted when accumulated protocol fees are withdrawn from the raffle instance.

| Field | Type | Description |
|-------|------|-------------|
| `recipient` | `Address` | Address receiving the withdrawn fees |
| `amount` | `i128` | Amount of fees withdrawn |
| `token` | `Address` | Token contract address of the withdrawn fees |
| `timestamp` | `u64` | Ledger timestamp of the withdrawal |

**Emitted by:** `withdraw_fees`
**When:** Admin withdraws accumulated protocol fees from a finalized or claimed raffle instance.

---

## EmergencyWithdrawn

Emitted when an emergency withdrawal of the prize is performed for a stuck raffle.

| Field | Type | Description |
|-------|------|-------------|
| `withdrawn_by` | `Address` | Address that initiated the emergency withdrawal |
| `to` | `Address` | Recipient address receiving the withdrawn tokens |
| `amount` | `i128` | Amount of tokens withdrawn |
| `token` | `Address` | Token contract address of the withdrawn tokens |
| `timestamp` | `u64` | Ledger timestamp of the withdrawal |

**Emitted by:** `emergency_withdraw`
**When:** After the `EMERGENCY_WITHDRAW_DELAY_SECONDS` (90-day) timeout has elapsed for a raffle stuck in `Finalized` or `Drawing` status. Creator or admin forcibly withdraws the prize pool.

---

## OracleAddressUpdated

Emitted when the oracle address is changed for an external-randomness raffle.

| Field | Type | Description |
|-------|------|-------------|
| `old_oracle` | `Option<Address>` | Previous oracle address (`None` if being set for the first time) |
| `new_oracle` | `Address` | New oracle address |
| `updated_by` | `Address` | Address that made the change |
| `timestamp` | `u64` | Ledger timestamp of the update |

**Emitted by:** `update_oracle_address`
**When:** Admin updates the oracle address for an `External`-randomness raffle whose tickets have not sold out yet.

---

## ProtocolFeeUpdated

Emitted when the per-raffle protocol fee basis points are changed.

| Field | Type | Description |
|-------|------|-------------|
| `old_fee_bp` | `u32` | Previous fee in basis points |
| `new_fee_bp` | `u32` | New fee in basis points (100 = 1%) |
| `updated_by` | `Address` | Address that made the change |
| `timestamp` | `u64` | Ledger timestamp of the update |

**Emitted by:** `set_protocol_fee_bp`
**When:** Admin changes the per-raffle protocol fee percentage before any tickets are sold.

---

## SwapDeadlineUpdated

Emitted when the swap deadline window is changed.

| Field | Type | Description |
|-------|------|-------------|
| `old_deadline_seconds` | `u64` | Previous deadline value in seconds |
| `new_deadline_seconds` | `u64` | New deadline value in seconds |
| `updated_by` | `Address` | Address that made the change |
| `timestamp` | `u64` | Ledger timestamp of the update |

**Emitted by:** `set_swap_deadline`
**When:** Admin changes the swap deadline window for time-sensitive swap operations before any tickets are sold.

---

## TicketSalesPaused

Emitted when ticket sales are paused for an active raffle.

| Field | Type | Description |
|-------|------|-------------|
| `paused_by` | `Address` | Address that paused ticket sales |
| `timestamp` | `u64` | Ledger timestamp of the pause |

**Emitted by:** `pause_ticket_sales`
**When:** Creator or admin pauses ticket purchases while the raffle is in `Active` status.

---

## TicketSalesResumed

Emitted when ticket sales are resumed after being paused.

| Field | Type | Description |
|-------|------|-------------|
| `resumed_by` | `Address` | Address that resumed ticket sales |
| `timestamp` | `u64` | Ledger timestamp of the resume |

**Emitted by:** `resume_ticket_sales`
**When:** Creator or admin resumes ticket purchases after a `TicketSalesPaused` event.

---

## ContractPaused (Instance)

Emitted when the raffle instance contract is paused.

| Field | Type | Description |
|-------|------|-------------|
| `paused_by` | `Address` | Address that paused the contract |
| `timestamp` | `u64` | Ledger timestamp of the pause |

**Emitted by:** `pause`
**When:** The factory pauses this raffle instance, preventing all state-changing operations.

---

## ContractUnpaused (Instance)

Emitted when the raffle instance contract is unpaused.

| Field | Type | Description |
|-------|------|-------------|
| `unpaused_by` | `Address` | Address that unpaused the contract |
| `timestamp` | `u64` | Ledger timestamp of the unpause |

**Emitted by:** `unpause`
**When:** The factory unpauses this raffle instance, restoring normal operation.

---

## TokensRescued

Emitted when accidentally-sent tokens are rescued from a raffle instance.

| Field | Type | Description |
|-------|------|-------------|
| `rescued_by` | `Address` | Address that rescued the tokens |
| `token` | `Address` | Address of the rescued token contract |
| `recipient` | `Address` | Address receiving the rescued tokens |
| `amount` | `i128` | Amount of tokens rescued |
| `timestamp` | `u64` | Ledger timestamp of the rescue |

**Emitted by:** `rescue_tokens`
**When:** Admin rescues tokens that were accidentally sent to the raffle instance (cannot sweep the raffle's own `payment_token` while prize is escrowed).

---

## AdminChanged

Emitted when the raffle instance admin is changed.

| Field | Type | Description |
|-------|------|-------------|
| `old_admin` | `Address` | Previous admin address |
| `new_admin` | `Address` | New admin address |
| `changed_by` | `Address` | Address that authorized the change (indexed topic) |
| `timestamp` | `u64` | Ledger timestamp of the change |

**Emitted by:** (dead code — defined but never emitted in current implementation)
**When:** The instance admin is changed via `set_admin` on a raffle instance.

---

# Indexer Implementation Notes

1. **Event Ordering**: Events are emitted in chronological order within each transaction.
2. **Multi-ticket Support**: `ticket_ids` in `TicketPurchased` is a vector supporting batch purchases.
3. **Optional Fields**: Fields typed as `Option<T>` may be `None` — indexer must handle both cases.
4. **Status Transitions**: `RaffleStatusChanged` events accompany most lifecycle events for redundancy.
5. **Timestamps**: All timestamps are Unix seconds from the ledger.
6. **Fee Calculation**: Platform fees are calculated as `(amount * fee_bp) / 10000`.
7. **Randomness Flow**: External randomness requires `RandomnessRequested` → `RandomnessReceived`; on timeout, `RandomnessFallbackTriggered` is emitted instead.
8. **Dead-Code Events**: Events marked as dead code (never emitted in the current implementation) may be activated in future versions — indexers should handle them gracefully.

## Event Emission Guarantees

- Events are only emitted on successful state changes.
- Failed transactions do not emit events.
- Each state-changing function emits exactly one primary event.
- Status changes emit both the primary event and `RaffleStatusChanged`.
- No events are emitted for read-only operations.
