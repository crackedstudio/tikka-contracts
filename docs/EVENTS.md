# Raffle Contract Events

This document describes all events emitted by the Tikka raffle contract. The indexer uses these events to reconstruct complete raffle state without querying contract storage.

## Event Topic Scheme

All events use a consistent two-symbol topic structure:
```
("tikka", "event_name")
```

Where:
- First symbol: `"tikka"` (constant namespace identifier)
- Second symbol: Event name in snake_case matching the struct name

## Lifecycle Events

### raffle_created

Emitted when a new raffle instance is initialized.

**Topic:** `("tikka", "raffle_created")`

**Fields:**
- `creator: Address` - Address of the raffle creator
- `end_time: u64` - Unix timestamp when raffle ends (0 for no time limit)
- `max_tickets: u32` - Maximum number of tickets that can be sold
- `ticket_price: i128` - Price per ticket in payment token units
- `payment_token: Address` - Address of the token used for payments
- `prize_amount: i128` - Total prize pool amount
- `description: String` - Human-readable raffle description
- `randomness_source: RandomnessSource` - Enum: Internal (0) or External (1)

---

### prize_deposited

Emitted when the creator deposits the prize pool into the contract.

**Topic:** `("tikka", "prize_deposited")`

**Fields:**
- `creator: Address` - Address that deposited the prize
- `amount: i128` - Amount deposited
- `token: Address` - Token contract address
- `timestamp: u64` - Unix timestamp of deposit

---

### ticket_purchased

Emitted when a user purchases one or more tickets.

**Topic:** `("tikka", "ticket_purchased")`

**Fields:**
- `buyer: Address` - Address of the ticket purchaser
- `ticket_ids: Vec<u32>` - List of ticket IDs purchased (supports multi-ticket purchases)
- `quantity: u32` - Number of tickets purchased in this transaction
- `total_paid: i128` - Total amount paid for all tickets
- `timestamp: u64` - Unix timestamp of purchase

---

### draw_triggered

Emitted when the draw process is initiated.

**Topic:** `("tikka", "draw_triggered")`

**Fields:**
- `triggered_by: Address` - Address that triggered the draw
- `total_tickets_sold: u32` - Total number of tickets sold at draw time
- `timestamp: u64` - Unix timestamp when draw was triggered

---

### randomness_requested

Emitted when external randomness is requested from an oracle.

**Topic:** `("tikka", "randomness_requested")`

**Fields:**
- `oracle: Address` - Address of the oracle contract
- `timestamp: u64` - Unix timestamp of request

---

### randomness_received

Emitted when external randomness is received from an oracle.

**Topic:** `("tikka", "randomness_received")`

**Fields:**
- `oracle: Address` - Address of the oracle that provided randomness
- `seed: u64` - Random seed value provided by oracle
- `timestamp: u64` - Unix timestamp when randomness was received

---

### raffle_finalized

Emitted when the raffle winner is determined.

**Topic:** `("tikka", "raffle_finalized")`

**Fields:**
- `winner: Address` - Address of the winning participant
- `winning_ticket_id: u32` - ID of the winning ticket
- `total_tickets_sold: u32` - Total tickets sold in this raffle
- `randomness_source: RandomnessSource` - Source used for winner selection
- `finalized_at: u64` - Unix timestamp when raffle was finalized

---

### raffle_cancelled

Emitted when a raffle is cancelled by the creator.

**Topic:** `("tikka", "raffle_cancelled")`

**Fields:**
- `creator: Address` - Address of the creator who cancelled
- `reason: String` - Human-readable cancellation reason
- `tickets_sold: u32` - Number of tickets sold before cancellation
- `timestamp: u64` - Unix timestamp of cancellation

---

### ticket_refunded

Emitted when a ticket holder receives a refund (e.g., after cancellation).

**Topic:** `("tikka", "ticket_refunded")`

**Fields:**
- `buyer: Address` - Address receiving the refund
- `ticket_id: u32` - ID of the refunded ticket
- `amount: i128` - Refund amount
- `timestamp: u64` - Unix timestamp of refund

---

### prize_claimed

Emitted when the winner claims their prize.

**Topic:** `("tikka", "prize_claimed")`

**Fields:**
- `winner: Address` - Address of the winner claiming the prize
- `gross_amount: i128` - Total prize amount before fees
- `net_amount: i128` - Amount transferred to winner after fees
- `platform_fee: i128` - Fee amount retained by platform
- `claimed_at: u64` - Unix timestamp of claim

---

## Admin Events

### oracle_address_updated

Emitted when the oracle address is changed.

**Topic:** `("tikka", "oracle_address_updated")`

**Fields:**
- `old_oracle: Option<Address>` - Previous oracle address (None if first time set)
- `new_oracle: Address` - New oracle address
- `updated_by: Address` - Admin address that made the change
- `timestamp: u64` - Unix timestamp of update

---

### fee_updated

Emitted when the protocol fee is changed.

**Topic:** `("tikka", "fee_updated")`

**Fields:**
- `old_fee_bp: u32` - Previous fee in basis points
- `new_fee_bp: u32` - New fee in basis points (100 = 1%)
- `updated_by: Address` - Admin address that made the change
- `timestamp: u64` - Unix timestamp of update

---

### treasury_updated

Emitted when the treasury address is changed.

**Topic:** `("tikka", "treasury_updated")`

**Fields:**
- `old_treasury: Option<Address>` - Previous treasury address (None if first time set)
- `new_treasury: Address` - New treasury address
- `updated_by: Address` - Admin address that made the change
- `timestamp: u64` - Unix timestamp of update

---

### fees_withdrawn

Emitted when accumulated fees are withdrawn from the contract.

**Topic:** `("tikka", "fees_withdrawn")`

**Fields:**
- `recipient: Address` - Address receiving the withdrawn fees
- `amount: i128` - Amount withdrawn
- `token: Address` - Token contract address
- `timestamp: u64` - Unix timestamp of withdrawal

---

### contract_paused

Emitted when the contract is paused by admin.

**Topic:** `("tikka", "contract_paused")`

**Fields:**
- `paused_by: Address` - Admin address that paused the contract
- `timestamp: u64` - Unix timestamp when paused

---

### contract_unpaused

Emitted when the contract is unpaused by admin.

**Topic:** `("tikka", "contract_unpaused")`

**Fields:**
- `unpaused_by: Address` - Admin address that unpaused the contract
- `timestamp: u64` - Unix timestamp when unpaused

---

### admin_transfer_proposed

Emitted when an admin transfer is proposed to a new address.

**Topic:** `("tikka", "admin_transfer_proposed")`

**Fields:**
- `current_admin: Address` - Current admin address
- `proposed_admin: Address` - Address proposed as new admin
- `timestamp: u64` - Unix timestamp of proposal

---

### admin_transfer_accepted

Emitted when a proposed admin accepts the transfer.

**Topic:** `("tikka", "admin_transfer_accepted")`

**Fields:**
- `old_admin: Address` - Previous admin address
- `new_admin: Address` - New admin address
- `timestamp: u64` - Unix timestamp when transfer was accepted

---

## Internal State Events

### status_changed

Emitted whenever the raffle status transitions.

**Topic:** `("tikka", "status_changed")`

**Fields:**
- `old_status: RaffleStatus` - Previous status enum value
- `new_status: RaffleStatus` - New status enum value
- `timestamp: u64` - Unix timestamp of status change

**RaffleStatus Enum Values:**
- `Proposed = 0` - Raffle created, awaiting prize deposit
- `Active = 1` - Prize deposited, accepting ticket purchases
- `Drawing = 2` - Ticket sales ended, determining winner
- `Finalized = 3` - Winner determined, awaiting claim
- `Claimed = 4` - Prize claimed by winner
- `Cancelled = 5` - Raffle cancelled by creator

---

## Indexer Implementation Notes

1. **Event Ordering**: Events are emitted in chronological order within each transaction
2. **Multi-ticket Support**: `ticket_ids` in `ticket_purchased` is a vector to support future batch purchases
3. **Optional Fields**: Fields typed as `Option<T>` may be `None` - indexer must handle both cases
4. **Status Transitions**: `status_changed` events accompany most lifecycle events for redundancy
5. **Timestamps**: All timestamps are Unix seconds from ledger
6. **Fee Calculation**: Platform fees are calculated as `(amount * fee_bp) / 10000`
7. **Randomness Flow**: External randomness requires two events: `randomness_requested` → `randomness_received`

## Event Emission Guarantees

- Events are only emitted on successful state changes
- Failed transactions do not emit events
- Each state-changing function emits exactly one primary event
- Status changes emit both the primary event and `status_changed`
- No events are emitted for read-only operations
