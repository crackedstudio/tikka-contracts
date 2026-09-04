# Raffle Contract Events

This document is **auto-generated** from the `#[contractevent]` struct
definitions in `contracts/*/src/events.rs`. **Do not edit by hand.**
Regenerate it whenever event structs or their field docs change:

```bash
python scripts/generate_event_docs.py
```

## Event Topic Scheme

All events use a two-symbol Soroban event topic:

```text
("tikka", "<event_topic>")
```

- First symbol: `"tikka"` (constant namespace).
- Second symbol: the event struct name in **snake_case** (e.g. `ticket_purchased`,
  `raffle_created`).

Fields marked `topic` in the tables below are part of the event topic rather
than the event body.

## Index-vs-ID convention

To avoid the drift that silently breaks indexers:

- `ticket_id` / `ticket_ids` / `ticket_number` are **1-based ticket IDs**.
- `winning_ticket_ids` are **0-based positions** within the ticket pool (the
  corresponding 1-based ticket ID is `winning_ticket_ids[i] + 1`).
- `*_index` fields are **0-based positions** into the array referenced by the
  field name.
- `*_id` / `*_count` / `round` fields state their base explicitly in the field
  docs.

---

# Factory Contract Events

Defined in `contracts\raffle-factory\src\events.rs`.

## AdminOpCancelled

Emitted when a proposed timelocked admin operation is cancelled.

Topic: `tikka:admin_op_cancelled`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `op_id` | `u32` |  | Sequential op ID (1-based) of the cancelled operation. |
| `cancelled_by` | `Address` |  | Admin that cancelled the op. |
| `cancelled_at` | `u64` |  | Ledger timestamp of the cancellation. |

**Emitted by:** `cancel_config_change`

---

## AdminOpExecuted

Emitted when a proposed admin operation is executed after its timelock elapsed.

Topic: `tikka:admin_op_executed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `op_id` | `u32` |  | Sequential op ID (1-based) of the executed operation. |
| `op` | `AdminOp` |  | Operation payload that was executed. |
| `executed_by` | `Address` |  | Admin that executed the op. |
| `executed_at` | `u64` |  | Ledger timestamp of execution. |

**Emitted by:** `execute_config_change`

---

## AdminOpProposed

Emitted when a new admin operation is proposed through the timelock mechanism.

Topic: `tikka:admin_op_proposed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `op_id` | `u32` |  | Sequential op ID (1-based) that identifies the proposed operation. |
| `op` | `AdminOp` |  | Operation payload being proposed. |
| `effective_timestamp` | `u64` |  | Ledger timestamp at which the op becomes executable. |
| `proposed_by` | `Address` |  | Admin that proposed the op. |

**Emitted by:** `propose_fee_change`, `propose_wasm_upgrade`, `set_config`

---

## AdminTransferAccepted

Emitted when the proposed admin accepts the admin transfer.

Topic: `tikka:admin_transfer_accepted`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_admin` | `Address` |  | Admin before the transfer. |
| `new_admin` | `Address` |  | Admin after the transfer. |
| `timestamp` | `u64` |  | Ledger timestamp of the acceptance. |

**Emitted by:** `accept_factory_admin`

---

## AdminTransferFailed

Emitted when an admin transfer proposal fails.  Note: this event is `#[allow(dead_code)]` in the current implementation.

Topic: `tikka:admin_transfer_failed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `current_admin` | `Address` |  | Current admin at the time of the failed transfer. |
| `proposed_admin` | `Address` |  | Admin that was proposed. |
| `reason_code` | `u32` |  | Numeric failure reason code. |
| `timestamp` | `u64` |  | Ledger timestamp of the failure. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## AdminTransferProposed

Emitted when an admin proposes transferring factory admin to a new address.

Topic: `tikka:admin_transfer_proposed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `current_admin` | `Address` |  | Current admin. |
| `proposed_admin` | `Address` |  | Admin proposed as replacement. |
| `timestamp` | `u64` |  | Ledger timestamp of the proposal. |

**Emitted by:** `transfer_factory_admin`

---

## CheckpointCreated

Emitted periodically to create a verifiable state checkpoint of all tracked raffles.

Topic: `tikka:checkpoint_created`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `index` | `u32` |  | 1-based checkpoint sequence number (`raffle_count / CHECKPOINT_INTERVAL`). |
| `raffle_count` | `u32` |  | Number of raffles recorded when the checkpoint was taken. |
| `ledger_timestamp` | `u64` |  | Ledger timestamp of the checkpoint. |
| `aggregate_hash` | `BytesN<32>` |  | SHA-256 over the checkpoint inputs (raffle count, ledger sequence, timestamp). |

**Emitted by:** `maybe_create_checkpoint`

---

## CreationPaused

Emitted when raffle creation is paused for the whole factory.

Topic: `tikka:creation_paused`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `paused_by` | `Address` |  | Address that paused raffle creation. |
| `timestamp` | `u64` |  | Ledger timestamp of the pause. |

**Emitted by:** `set_creation_paused`

---

## CreationRateLimited

Emitted when a creator is rate-limited from creating new raffles.

Topic: `tikka:creation_rate_limited`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `creator` | `Address` |  | Creator that was rate-limited. |
| `unlock_timestamp` | `u64` |  | Ledger timestamp at which creation is allowed again. |
| `timestamp` | `u64` |  | Ledger timestamp of the rate-limit event. |

**Emitted by:** `create_raffle`

---

## CreationUnpaused

Emitted when raffle creation is resumed after being paused.

Topic: `tikka:creation_unpaused`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `unpaused_by` | `Address` |  | Address that unpaused raffle creation. |
| `timestamp` | `u64` |  | Ledger timestamp of the resume. |

**Emitted by:** `set_creation_paused`

---

## FactoryInitialized

Emitted when the factory contract is initialized for the first time.

Topic: `tikka:factory_initialized`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `admin` | `Address` |  | Admin of the factory. |
| `protocol_fee_bp` | `u32` |  | Protocol fee (basis points) applied to prizes. |
| `treasury` | `Address` |  | Treasury address that receives protocol fees. |
| `timestamp` | `u64` |  | Ledger timestamp of initialization. |

**Emitted by:** `init_factory`

---

## FactoryTokensRescued

Emitted when tokens are rescued out of the factory contract.

Topic: `tikka:factory_tokens_rescued`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `rescued_by` | `Address` |  | Address that rescued the tokens. |
| `token` | `Address` |  | Token that was rescued. |
| `recipient` | `Address` |  | Address the rescued funds were sent to. |
| `amount` | `i128` |  | Amount rescued. |
| `timestamp` | `u64` |  | Ledger timestamp of the rescue. |

**Emitted by:** `rescue_tokens`

---

## FactoryUpgraded

Emitted when the factory contract is upgraded to new wasm code.

Topic: `tikka:factory_upgraded`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `admin` | `Address` |  | Admin that triggered the upgrade. |
| `new_wasm_hash` | `BytesN<32>` |  | SHA-256 wasm hash of the new contract code. |
| `timestamp` | `u64` |  | Ledger timestamp of the upgrade. |

**Emitted by:** `upgrade`

---

## GlobalEmergencyPaused

Emitted when the entire factory (creation, pauses, and every live raffle) is put into emergency pause.

Topic: `tikka:global_emergency_paused`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `paused_by` | `Address` |  | Address that paused the whole factory. |
| `reason` | `soroban_sdk::String` |  | Free-text reason for the emergency pause. |
| `timestamp` | `u64` |  | Ledger timestamp of the pause. |

**Emitted by:** `emergency_pause_all`

---

## GlobalEmergencyUnpaused

Emitted when the factory-wide emergency pause is lifted.

Topic: `tikka:global_emergency_unpaused`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `unpaused_by` | `Address` |  | Address that lifted the emergency pause. |
| `timestamp` | `u64` |  | Ledger timestamp of the resume. |

**Emitted by:** `emergency_unpause_all`

---

## RaffleCleanedUp

Emitted when a finished raffle instance's storage is cleaned up from the factory.

Topic: `tikka:raffle_cleaned_up`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `raffle_address` | `Address` |  | Address of the raffle instance that was cleaned up. |
| `cleaned_by` | `Address` |  | Address that performed the cleanup. |
| `finish_time` | `u64` |  | Timestamp at which the raffle was finalized. |
| `cleaned_at` | `u64` |  | Ledger timestamp of the cleanup. |

**Emitted by:** `clean_old_raffle`

---

## RaffleInstanceDeployed

Emitted when the factory deploys a new raffle instance contract.  Note: this event is `#[allow(dead_code)]` in the current implementation.

Topic: `tikka:raffle_instance_deployed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `instance` | `Address` |  | Address of the deployed raffle instance. |
| `wasm_hash` | `BytesN<32>` |  | SHA-256 wasm hash of the deployed instance code. |
| `creator` | `Address` |  | Address that deployed the instance. |
| `timestamp` | `u64` |  | Ledger timestamp of the deployment. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## RecurringRaffleCancelled

Emitted when a recurring raffle schedule is cancelled.

Topic: `tikka:recurring_raffle_cancelled`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `recurring_id` | `u32` |  | Unique ID of the recurring raffle schedule. |
| `cancelled_by` | `Address` |  | Address that cancelled the schedule. |
| `rounds_completed` | `u32` |  | Total number of rounds completed before cancellation. |
| `timestamp` | `u64` |  | Ledger timestamp of the cancellation. |

**Emitted by:** `cancel_recurring_raffle`

---

## RecurringRaffleCreated

Emitted when a recurring raffle schedule is created.

Topic: `tikka:recurring_raffle_created`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `recurring_id` | `u32` |  | Unique ID (1-based) of the recurring raffle schedule. |
| `creator` | `Address` |  | Creator that owns the recurring schedule. |
| `interval_seconds` | `u64` |  | Seconds between consecutive rounds. |
| `max_rounds` | `u32` |  | Maximum number of rounds; `0` means unlimited. |
| `auto_fund` | `bool` |  | Whether prize funding for each round happens automatically. |
| `next_due` | `u64` |  | Ledger timestamp of the next scheduled round. |
| `timestamp` | `u64` |  | Ledger timestamp of the schedule creation. |

**Emitted by:** `create_recurring_raffle`

---

## RecurringRoundTriggered

Emitted each time a recurring raffle schedule fires a new round.

Topic: `tikka:recurring_round_triggered`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `recurring_id` | `u32` |  | Unique ID of the recurring raffle schedule. |
| `round` | `u32` |  | 1-based round number that just fired. |
| `raffle_address` | `Address` |  | Address of the raffle instance created for this round. |
| `next_due` | `u64` |  | Ledger timestamp of the next scheduled round. |
| `timestamp` | `u64` |  | Ledger timestamp of the trigger. |

**Emitted by:** `trigger_next_round`

---

## SupportedSacUpdated

Emitted when a Stellar Asset Contract (SAC) token's support status is updated.  Note: this event is `#[allow(dead_code)]` in the current implementation.

Topic: `tikka:supported_sac_updated`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `token` | `Address` |  | Token whose SAC support flag changed. |
| `supported` | `bool` |  | Whether the token is supported for SAC-assisted settlement. |
| `updated_by` | `Address` |  | Address that updated the flag. |
| `timestamp` | `u64` |  | Ledger timestamp of the update. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## TreasuryChanged

Emitted when the factory treasury address is changed.  Note: this event is `#[allow(dead_code)]` in the current implementation.

Topic: `tikka:treasury_changed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_treasury` | `Address` |  | Treasury before the change. |
| `new_treasury` | `Address` |  | Treasury after the change. |
| `changed_by` | `Address` | topic | Topic: address that changed the treasury. |
| `timestamp` | `u64` |  | Ledger timestamp of the change. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

# Instance Contract Events

Defined in `contracts\raffle-instance\src\events.rs`.

## AdminChanged

Emitted when the raffle admin is changed.  Note: this event is `#[allow(dead_code)]` in the current implementation.

Topic: `tikka:admin_changed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_admin` | `Address` |  | Admin before the change. |
| `new_admin` | `Address` |  | Admin after the change. |
| `changed_by` | `Address` | topic | Topic: address that changed the admin. |
| `timestamp` | `u64` |  | Ledger timestamp of the change. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## CancelScheduled

Emitted when an admin schedules a cancellation of a raffle that has already sold tickets. The actual cancel only executes via `execute_admin_cancel` once `cancel_at` has passed. Ticket holders may refund immediately as soon as this event is emitted (#406).

Topic: `tikka:cancel_scheduled`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `creator` | `Address` |  | Address that created the raffle. |
| `scheduled_by` | `Address` |  | Admin address that scheduled the cancellation. |
| `tickets_sold` | `u32` |  | Number of tickets sold when the cancel was scheduled. |
| `cancel_at` | `u64` |  | Ledger timestamp at which the cancel becomes executable. |
| `timestamp` | `u64` |  | Ledger timestamp of the schedule. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## DrawTriggered

Emitted when the draw is triggered (usually by the last ticket purchase).

Topic: `tikka:draw_triggered`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `caller` | `Address` |  | Address that triggered the draw (usually the last buyer). |
| `total_tickets_sold` | `u32` |  | Number of tickets sold when the draw was triggered. |
| `timestamp` | `u64` |  | Ledger timestamp of the trigger. |

**Emitted by:** `buy_tickets`, `buy_tickets_for`, `finalize_raffle`

---

## DustSwept

Emitted when residual dust balances are swept to the treasury.

Topic: `tikka:dust_swept`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `swept_by` | `Address` |  | Address that triggered the sweep. |
| `token` | `Address` |  | Token whose dust balance was swept. |
| `treasury` | `Address` |  | Address the dust was swept to. |
| `amount` | `i128` |  | Amount swept. |
| `timestamp` | `u64` |  | Ledger timestamp of the sweep. |

**Emitted by:** `sweep_dust`

---

## EmergencyWithdrawn

Emitted when an emergency withdrawal is executed after the delay elapses.

Topic: `tikka:emergency_withdrawn`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `withdrawn_by` | `Address` |  | Address that issued the emergency withdrawal. |
| `to` | `Address` |  | Address the funds were withdrawn to. |
| `amount` | `i128` |  | Amount withdrawn. |
| `token` | `Address` |  | Token withdrawn. |
| `timestamp` | `u64` |  | Ledger timestamp of the withdrawal. |

**Emitted by:** `emergency_withdraw`

---

## EndTimeExtended

Emitted when the raffle end time is extended.

Topic: `tikka:end_time_extended`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_end_time` | `u64` |  | End time before the extension. |
| `new_end_time` | `u64` |  | End time after the extension. |
| `extended_by` | `Address` |  | Address that extended the end time. |
| `timestamp` | `u64` |  | Ledger timestamp of the extension. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## FeesWithdrawn

Emitted when accumulated protocol fees are withdrawn to the treasury.

Topic: `tikka:fees_withdrawn`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `recipient` | `Address` |  | Address the accumulated fees were sent to. |
| `amount` | `i128` |  | Amount withdrawn. |
| `token` | `Address` |  | Token the fees were held in. |
| `timestamp` | `u64` |  | Ledger timestamp of the withdrawal. |

**Emitted by:** `withdraw_fees`

---

## MetadataHashUpdated

Emitted when the metadata hash backing a raffle's description is updated.

Topic: `tikka:metadata_hash_updated`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_hash` | `BytesN<32>` |  | Metadata hash before the update. |
| `new_hash` | `BytesN<32>` |  | Metadata hash after the update. |
| `updated_by` | `Address` |  | Address that performed the update. |
| `timestamp` | `u64` |  | Ledger timestamp of the update. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## OracleAddressUpdated

Emitted when the configured randomness oracle address is updated.

Topic: `tikka:oracle_address_updated`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_oracle` | `Option<Address>` |  | Previous oracle address, if one was configured. |
| `new_oracle` | `Address` |  | New oracle address. |
| `updated_by` | `Address` |  | Address that performed the update. |
| `timestamp` | `u64` |  | Ledger timestamp of the update. |

**Emitted by:** `update_oracle_address`

---

## OracleSeedDelivered

Emitted each time an oracle in the quorum submits its random seed.

Topic: `tikka:oracle_seed_delivered`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `oracle` | `Address` |  | Oracle address that submitted its random seed for quorum aggregation. |
| `seed` | `u64` |  | Seed value delivered by this oracle. |
| `request_id` | `u64` |  | Correlation ID of the original quorum request. |
| `current_count` | `u32` |  | Number of distinct seeds collected so far (1-based count). |
| `threshold` | `u32` |  | Quorum threshold (`k`) required before aggregation happens. |
| `timestamp` | `u64` |  | Ledger timestamp of the delivery. |

**Emitted by:** `provide_quorum_randomness`

---

## PrizeClaimed

Emitted when a winner claims their prize.

Topic: `tikka:prize_claimed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `winner` | `Address` |  | Winner address claiming the prize. |
| `tier_index` | `u32` |  | 0-based prize tier index being claimed (`prizes[i]`). |
| `payment_token` | `Address` |  | Token the prize is paid in. |
| `gross_amount` | `i128` |  | Prize amount before platform fee deduction. |
| `net_amount` | `i128` |  | Amount actually transferred to the winner. |
| `platform_fee` | `i128` |  | Platform fee withheld from the prize. |
| `claimed_at` | `u64` |  | Ledger timestamp of the claim. |

**Emitted by:** `claim_prize`

---

## PrizeDeposited

Emitted when the prize pool is deposited into the raffle.

Topic: `tikka:prize_deposited`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `creator` | `Address` |  | Address that deposited the prize (usually the creator). |
| `amount` | `i128` |  | Amount deposited. |
| `token` | `Address` |  | Token the prize is held in. |
| `timestamp` | `u64` |  | Ledger timestamp of the deposit. |

**Emitted by:** `deposit_prize`

---

## PrizeRefunded

Emitted when the prize pool is refunded back to the creator.

Topic: `tikka:prize_refunded`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `creator` | `Address` |  | Address that received the refund (usually the creator). |
| `amount` | `i128` |  | Amount refunded. |
| `token` | `Address` |  | Token the refund was paid in. |
| `timestamp` | `u64` |  | Ledger timestamp of the refund. |

**Emitted by:** `refund_prize`

---

## PrizeSwept

Emitted once per unclaimed winner when `sweep_unclaimed` runs after `claim_expiry_seconds` has elapsed since finalization.  The prize share is transferred to the raffle's `treasury_address`.

Topic: `tikka:prize_swept`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `winner` | `Address` |  | Original winner address whose unclaimed prize was swept. |
| `tier_index` | `u32` |  | Prize tier index (0-based, matches `prizes` array). |
| `treasury` | `Address` |  | Treasury address that received the swept prize. |
| `amount` | `i128` |  | Amount transferred to treasury. |
| `swept_at` | `u64` |  | Ledger timestamp of the sweep. |

**Emitted by:** `sweep_unclaimed`

---

## ProtocolFeeUpdated

Emitted when the protocol fee (basis points) is updated.

Topic: `tikka:protocol_fee_updated`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_fee_bp` | `u32` |  | Protocol fee (basis points) before the update. |
| `new_fee_bp` | `u32` |  | Protocol fee (basis points) after the update. |
| `updated_by` | `Address` |  | Address that performed the update. |
| `timestamp` | `u64` |  | Ledger timestamp of the update. |

**Emitted by:** `set_protocol_fee_bp`

---

## RaffleCancelled

Emitted when a raffle is cancelled and (if applicable) prizes refunded.

Topic: `tikka:raffle_cancelled`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `creator` | `Address` |  | Address that created the raffle. |
| `reason` | `CancelReason` |  | Machine-readable cancel reason. |
| `tickets_sold` | `u32` |  | Number of tickets sold before cancellation. |
| `prize_refunded` | `bool` |  | Whether the deposited prize was returned to the creator. |
| `timestamp` | `u64` |  | Ledger timestamp of the cancellation. |

**Emitted by:** `cancel_raffle`, `trigger_randomness_fallback`

---

## RaffleCreated

Emitted when a new raffle instance is created with its initial configuration.

Topic: `tikka:raffle_created`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `raffle_id` | `Address` |  | Instance contract address of the new raffle. |
| `creator` | `Address` |  | Address that created the raffle. |
| `end_time` | `u64` |  | Ledger timestamp at which the raffle is scheduled to close. |
| `max_tickets` | `u32` |  | Total number of tickets that can ever be sold (1-based ticket IDs run `1..=max_tickets`). |
| `ticket_price` | `i128` |  | Nominal price (in `payment_token`) of a single ticket. |
| `payment_token` | `Address` |  | Token in which tickets are paid for. |
| `prize_amount` | `i128` |  | Total prize pool deposited into the raffle. |
| `prizes` | `Vec<u32>` |  | Prize tier weights (basis points), 0-based supported levels. |
| `description` | `String` |  | Free-text description set by the creator. |
| `randomness_source` | `RandomnessSource` |  | Entry point that will drive the draw randomness. |
| `metadata_hash` | `BytesN<32>` | topic | Topic: SHA-256 of the metadata the description resolves to. |
| `unique_winners` | `bool` |  | Whether each address may win at most once. |

**Emitted by:** `init`

---

## RaffleFailed

Emitted when the raffle fails during its lifecycle and is wound down.

Topic: `tikka:raffle_failed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `creator` | `Address` |  | Address that created the raffle. |
| `reason` | `FailureReason` |  | Machine-readable failure reason. |
| `tickets_sold` | `u32` |  | Number of tickets sold before the failure. |
| `timestamp` | `u64` |  | Ledger timestamp of the failure. |

**Emitted by:** `finalize_raffle`

---

## RaffleFinalized

Emitted when the raffle draw completes and winners are recorded.

Topic: `tikka:raffle_finalized`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `raffle_id` | `Address` |  | Instance contract address of the finalized raffle. |
| `winners` | `Vec<Address>` |  | Winner addresses; parallel to `winning_ticket_ids`. |
| `winning_ticket_ids` | `Vec<u32>` |  | 0-based winning positions within the ticket pool (1-based ticket IDs are `winning_ticket_ids[i] + 1`); parallel to `winners`.  These are **positions**, not ticket IDs. |
| `total_tickets_sold` | `u32` |  | Number of tickets that were sold for the raffle. |
| `randomness_source` | `RandomnessSource` |  | Randomness source that produced `randomness_type`. |
| `randomness_type` | `RandomnessType` |  | Concrete randomness mode used for this draw. |
| `finalized_at` | `u64` |  | Ledger timestamp of finalization. |
| `unique_winners` | `bool` |  | Whether unique-winner selection was applied. |

**Emitted by:** `do_finalize_with_seed`

---

## RaffleStatusChanged

Emitted on every raffle status transition.

Topic: `tikka:raffle_status_changed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_status` | `raffle_shared::RaffleStatus` |  | Status before the transition. |
| `new_status` | `raffle_shared::RaffleStatus` |  | Status after the transition. |
| `timestamp` | `u64` |  | Ledger timestamp of the transition. |

**Emitted by:** `transition_status`

---

## RandomnessFallbackTriggered

Emitted when the randomness fallback path is used to finalize a draw.

Topic: `tikka:randomness_fallback_triggered`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `triggered_by` | `Address` |  | Address that triggered the fallback. |
| `seed_used` | `u64` |  | Seed value used to finalize the draw. |
| `request_ledger` | `u32` |  | Ledger sequence at which randomness was originally requested. |
| `fallback_ledger` | `u32` |  | Ledger sequence at which the fallback fired. |
| `timestamp` | `u64` |  | Ledger timestamp of the fallback. |

**Emitted by:** `trigger_randomness_fallback`

---

## RandomnessReceived

Emitted when the oracle delivers a seed for the draw.

Topic: `tikka:randomness_received`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `oracle` | `Address` |  | Oracle address that delivered the seed. |
| `seed` | `u64` |  | Raw seed value returned by the oracle. |
| `request_id` | `u64` |  | Correlation ID of the original request. |
| `timestamp` | `u64` |  | Ledger timestamp of the delivery. |

**Emitted by:** `provide_randomness`

---

## RandomnessRequested

Emitted when a randomness request is sent to the configured oracle.

Topic: `tikka:randomness_requested`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `oracle` | `Address` |  | Oracle address the request was sent to. |
| `request_id` | `u64` |  | Correlation ID used to match the later delivery. |
| `timestamp` | `u64` |  | Ledger timestamp of the request. |

**Emitted by:** `buy_tickets`, `buy_tickets_for`, `finalize_raffle`

---

## StorageWiped

Emitted when all contract storage for the raffle is wiped.

Topic: `tikka:storage_wiped`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `wiped_by` | `Address` |  | Address that wiped the storage. |
| `timestamp` | `u64` |  | Ledger timestamp of the wipe. |

**Emitted by:** `wipe_storage`

---

## SwapDeadlineUpdated

Emitted when the swap deadline (seconds) is updated.

Topic: `tikka:swap_deadline_updated`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `old_deadline_seconds` | `u64` |  | Swap deadline (seconds) before the update. |
| `new_deadline_seconds` | `u64` |  | Swap deadline (seconds) after the update. |
| `updated_by` | `Address` |  | Address that performed the update. |
| `timestamp` | `u64` |  | Ledger timestamp of the update. |

**Emitted by:** `set_swap_deadline`

---

## TicketGifted

Emitted when tickets are bought for another address (a gift).

Topic: `tikka:ticket_gifted`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `buyer` | `Address` |  | Address that paid for the tickets. |
| `recipient` | `Address` |  | Address that received the tickets (owner of record). |
| `ticket_ids` | `Vec<u32>` |  | 1-based ticket IDs minted; length equals `quantity`. |
| `quantity` | `u32` |  | Number of tickets gifted in this transaction. |
| `ticket_price` | `i128` |  | Nominal unit price used for reserved seats. |
| `effective_ticket_price` | `i128` |  | Effective unit price actually charged (equals `ticket_price` when no discount applies). |
| `total_paid` | `i128` |  | Total paid by `buyer`. |
| `protocol_fee` | `i128` |  | Protocol fee (basis points of `total_paid`). |
| `timestamp` | `u64` |  | Ledger timestamp of the gift. |

**Emitted by:** `buy_tickets_for`

---

## TicketNftMinted

Emitted once per NFT receipt is successfully minted by the configured `nft_contract`.

Topic: `tikka:ticket_nft_minted`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `recipient` | `Address` |  | The address that received the NFT (the ticket buyer). |
| `ticket_id` | `u32` |  | The ticket ID within this raffle (1-indexed). |
| `raffle_id` | `Address` |  | The raffle instance contract address (NFT namespace). |
| `nft_contract` | `Address` |  | The NFT contract that performed the mint. |
| `timestamp` | `u64` |  | Ledger timestamp of the mint. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## TicketPurchased

Emitted when a ticket purchase succeeds and tickets are minted.

Topic: `tikka:ticket_purchased`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `buyer` | `Address` |  | Address whose account(s) paid for the tickets. |
| `ticket_ids` | `Vec<u32>` |  | 1-based ticket IDs minted; length equals `quantity`. |
| `quantity` | `u32` |  | Number of tickets purchased in this transaction. |
| `ticket_price` | `i128` |  | Nominal unit price used for reserved seats. |
| `effective_ticket_price` | `i128` |  | Effective unit price actually charged (equals `ticket_price` when no discount applies). |
| `total_paid` | `i128` |  | Total paid: `effective_ticket_price * quantity` after any discounts. |
| `protocol_fee` | `i128` |  | Protocol fee (basis points of `total_paid`) withheld and forwarded to the treasury. |
| `timestamp` | `u64` |  | Ledger timestamp of the purchase. |

**Emitted by:** `buy_tickets`

---

## TicketRefunded

Emitted when a ticket is refunded (only applies to refundable ticket states).

Topic: `tikka:ticket_refunded`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `buyer` | `Address` |  | Address that was refunded (the ticket owner at refund time). |
| `ticket_number` | `u32` |  | 1-based ticket ID that was refunded. |
| `amount` | `i128` |  | Amount refunded, denominated in the payment token. |
| `timestamp` | `u64` |  | Ledger timestamp of the refund. |

**Emitted by:** `refund_ticket`

---

## TicketSalesPaused

Emitted when ticket sales are paused for the raffle.

Topic: `tikka:ticket_sales_paused`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `paused_by` | `Address` |  | Address that paused sales. |
| `timestamp` | `u64` |  | Ledger timestamp of the pause. |

**Emitted by:** `pause_ticket_sales`

---

## TicketSalesResumed

Emitted when ticket sales are resumed for the raffle.

Topic: `tikka:ticket_sales_resumed`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `resumed_by` | `Address` |  | Address that resumed sales. |
| `timestamp` | `u64` |  | Ledger timestamp of the resume. |

**Emitted by:** `resume_ticket_sales`

---

## TicketTransferred

Emitted when a ticket changes ownership.  Note: this event is `#[allow(dead_code)]` in the current implementation.

Topic: `tikka:ticket_transferred`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `ticket_id` | `u32` |  | 1-based ticket ID transferred. |
| `from` | `Address` |  | Previous owner. |
| `to` | `Address` |  | New owner. |
| `timestamp` | `u64` |  | Ledger timestamp of the transfer. |

**Emitted by:** *(no live call sites — defined but not currently published)*

---

## TokensRescued

Emitted when tokens are rescued out of the raffle contract.

Topic: `tikka:tokens_rescued`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `rescued_by` | `Address` |  | Address that rescued the tokens. |
| `token` | `Address` |  | Token that was rescued. |
| `recipient` | `Address` |  | Address the rescued funds were sent to. |
| `amount` | `i128` |  | Amount rescued. |
| `timestamp` | `u64` |  | Ledger timestamp of the rescue. |

**Emitted by:** `rescue_tokens`

---

## WinnerDrawn

Emitted once per winner selected during the draw.

Topic: `tikka:winner_drawn`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `winner` | `Address` |  | Winner address for this prize tier. |
| `ticket_id` | `u32` |  | 1-based ticket ID of the winning ticket (pool position + 1). |
| `tier_index` | `u32` |  | 0-based prize tier index this win corresponds to (`prizes[i]`). |
| `timestamp` | `u64` |  | Ledger timestamp of the draw. |

**Emitted by:** `do_finalize_with_seed`

---

# Shared Events

These events are defined once in `contracts/raffle-shared/src/events.rs` and re-exported by both the factory and the instance contracts. They are emitted with identical payloads from either contract.

## ContractPaused

Emitted when either the factory or an instance contract is paused.

Topic: `tikka:contract_paused`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `paused_by` | `Address` |  | Address that paused the contract. |
| `timestamp` | `u64` |  | Ledger timestamp of the pause. |

**Emitted by:** `pause`, `pause_factory`

---

## ContractUnpaused

Emitted when either the factory or an instance contract is unpaused.

Topic: `tikka:contract_unpaused`

| Field | Type | Flags | Description |
|-------|------|-------|-------------|
| `unpaused_by` | `Address` |  | Address that unpaused the contract. |
| `timestamp` | `u64` |  | Ledger timestamp of the resume. |

**Emitted by:** `unpause`, `unpause_factory`

---

