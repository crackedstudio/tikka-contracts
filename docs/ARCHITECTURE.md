# Tikka Architecture

This document explains how the factory, raffle instances, oracle, and clients interact.

## Factory -> Instance -> Oracle Flow

```mermaid
graph TB
    UI[Frontend / DApp]
    Factory[RaffleFactory Contract]
    Instance[RaffleInstance Contract]
    Oracle[Oracle Service]
    Stellar[Stellar Network]
    IPFS[IPFS / Metadata]

    UI -->|create_raffle| Factory
    Factory -->|deploys| Instance
    UI -->|buy_tickets| Instance
    UI -->|finalize_raffle| Instance
    Instance -->|RandomnessRequested event| Stellar
    Oracle -->|polls events| Stellar
    Oracle -->|provide_randomness| Instance
    Instance -->|RaffleFinalized event| Stellar
    UI -->|claim_prize| Instance
    UI -->|metadata_hash| IPFS
```

### Flow explanation

1. A creator calls `create_raffle` on the factory with `RaffleConfig`.
1. The factory deploys a new raffle instance and returns the new instance address.
1. Users buy tickets directly on the raffle instance contract.
1. When finalization starts, the instance emits randomness request events to the network.
1. The oracle service polls those events and calls `provide_randomness` back on the instance.
1. The instance finalizes winners, emits finalization events, and winners claim prizes.

## RaffleStatus State Machine

```mermaid
stateDiagram-v2
    [*] --> PendingPrize: create_raffle
    PendingPrize --> Active: deposit_prize
    Active --> Drawing: finalize_raffle / tickets_full
    Active --> Cancelled: cancel_raffle
    Active --> Failed: finalize_raffle (min_tickets not met)
    Drawing --> Finalized: provide_randomness / finalize (internal)
    Drawing --> Cancelled: cancel_raffle / fallback(refund)
    Finalized --> Claimed: all winners claim
    Drawing --> Cancelled: emergency_withdraw (after timeout)
```

### State notes

- `PendingPrize`: created but not funded yet.
- `Active`: funded and selling tickets.
- `Drawing`: draw execution in progress.
- `Finalized`: winners are locked and can claim.
- `Claimed`: terminal state when all claims are complete.
- `Cancelled` / `Failed`: terminal non-success states.

### Token egress and escrow solvency

The instance has four intended token-moving paths:

- `claim_prize` pays each unclaimed winner and records protocol fees.
- `sweep_unclaimed` pays unclaimed prizes to the treasury after the claim
    expiry period and marks those prizes claimed.
- `refund_prize` returns the deposited prize after `Cancelled` or `Failed`.
- `refund_ticket` returns each ticket payment after `Cancelled` or `Failed`.
- `withdraw_fees` pays only recorded accumulated fees after finalization.

Administrative escape paths are constrained by the same invariant:

- `emergency_withdraw` is only available for a timed-out `Drawing` raffle.
    Its delay starts at `end_time`, or at the randomness request ledger for a
    no-deadline raffle. It transfers only the deposited prize token and leaves
    all remaining obligations covered.
- `rescue_tokens` can transfer unrelated-token surplus, but for either
    configured raffle token it must leave unpaid ticket refunds, accumulated
    fees, and outstanding prize claims fully covered.
- `sweep_dust` is available only after settlement and transfers payment-token
    surplus above all remaining entitlements; accumulated fees are preserved.

Escrow solvency is a protocol guarantee. After every successful state-changing
entrypoint, configured-token balances must cover all stored entitlements:

```text
balance(prize_token)   >= unclaimed_prize_total
balance(payment_token) >= unrefunded_ticket_total + accumulated_fees_owed
```

When `payment_token == prize_token`, these are enforced as one combined
inequality over the shared token balance. `unclaimed_prize_total`,
`unrefunded_ticket_total`, and `accumulated_fees_owed` are derived from
contract storage, not off-chain indexer state or test bookkeeping.

No token-moving path may reduce a token balance below its outstanding
entitlement. `emergency_withdraw` cannot operate on `Finalized`, because
unclaimed winners remain entitled to their prizes.

### Entrypoint Lifecycle Transition Matrix

The following table summarizes the behavior of mutating contract entrypoints across all 7 `RaffleStatus` states (#623):

| Mutating Entrypoint | PendingPrize | Active | Drawing | Finalized | Cancelled | Failed | Claimed |
|---|---|---|---|---|---|---|---|
| `deposit_prize` | **Allowed** (-> Active) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) | Rejected (`PrizeAlreadyDeposited`) |
| `buy_tickets` | Rejected (`RaffleInactive`) | **Allowed** (-> Active / Drawing) | Rejected (`DrawingAlreadyInProgress` / `RaffleInactive`) | Rejected (`RaffleInactive`) | Rejected (`RaffleInactive`) | Rejected (`RaffleInactive`) | Rejected (`RaffleInactive`) |
| `finalize_raffle` | Rejected (`InvalidStateTransition`) | **Allowed** (if ended/full) | **Allowed** (if Drawing) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) |
| `provide_randomness` | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | **Allowed** (-> Finalized) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) |
| `claim_prize` | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | **Allowed** (-> Finalized / Claimed) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) |
| `cancel_raffle` | **Allowed** (-> Cancelled) | **Allowed** (-> Cancelled) | **Allowed** (-> Cancelled) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | **Allowed** (-> Cancelled) | Rejected (`InvalidStatus`) |
| `refund_ticket` | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | Rejected (`InvalidStatus`) | **Allowed** | **Allowed** | Rejected (`InvalidStatus`) |

## Security: Checks-Effects-Interactions Pattern

All contract entrypoints **MUST** follow this ordering to prevent reentrancy attacks and ensure atomicity.

### The Rule

| Step | Phase | Description |
|------|-------|-------------|
| 1 | **CHECK** | Validate all inputs, conditions, and permissions |
| 2 | **EFFECTS** | Perform all state mutations (storage writes) |
| 3 | **INTERACTIONS** | Make external calls (transfers, factory calls, etc.) |

### Applied to `buy_tickets` and `buy_tickets_for`

```rust
// 1. CHECK: Validate inputs
let _guard = Guard::new(&env)?;        // Reentrancy guard
require_not_paused(&env)?;              // Contract state check
if quantity == 0 { return Err(...); }   // Input validation
if raffle.status != Active { ... }      // State validation

// 2. EFFECTS: Charge payment FIRST (before any state mutation)
token_client.transfer(&buyer, &contract, &total_price)?;

// 3. EFFECTS: Mutate state
env.storage().persistent().set(&DataKey::Ticket(ticket_id), &ticket);
raffle.tickets_sold += quantity;
crate::write_raffle(&env, &raffle);

// 4. INTERACTIONS: External calls LAST
env.invoke_contract(&factory, "record_volume", args);
env.invoke_contract(&factory, "track_participant", args);

### Entrypoint Security Status
Entrypoint	Guard	Payment First	Factory Last	Status
buy_tickets	✅	✅	✅	✅ Fixed (Issue #763)
buy_tickets_for	✅	✅	✅	✅ Fixed (Issue #763)
claim_prize	✅	N/A	N/A	✅ Already has guard
refund_ticket	✅	N/A	N/A	✅ Already has guard
refund_prize	✅	N/A	N/A	✅ Already has guard

### Why This Matters
✅ Prevents reentrancy attacks - No external calls before state is final

✅ Prevents unpaid tickets - Payment must succeed before any state change

✅ Atomicity - If anything fails, the entire transaction reverts

✅ Checks-effects-interactions - Industry standard security pattern

### Historical Context
Issue #763 identified that buy_tickets was violating this pattern:

❌ Payment was happening LAST (after state mutations)

❌ Factory calls were happening BEFORE payment

❌ No reentrancy guard in purchase paths

### Fix applied (Issue #763):

✅ Payment moved to FIRST (before any state mutation)

✅ Reentrancy guard added to both purchase paths

✅ Factory notifications moved to the END (after all state is final)

✅ Event emission moved after state is final

### Testing
A malicious token contract that reenters buy_tickets during the transfer will now:

Find that Guard is already held → Error::Reentrancy

Or find that state is already final → no unpaid tickets can be minted

This closes the attack vector described in Issue #763.

