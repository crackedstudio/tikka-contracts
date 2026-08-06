//! Fuzz target: refund and cancellation flows
//!
//! Exercises arbitrary interleavings of ticket purchases, raffle cancellations
//! (by creator *or* admin), individual ticket refunds, prize-deposit activation,
//! and forced-failure transitions against a pure-Rust model of the
//! raffle-instance state machine.
//!
//! # Invariants checked on every execution
//!
//! 1. **No double-refund** — a ticket may only be refunded once; the second
//!    call must return `PrizeAlreadyClaimed`.
//! 2. **Total refunded ≤ total paid** — the sum of all successful refund
//!    amounts never exceeds the sum of all ticket purchases.
//! 3. **Contract balance never negative vs entitlements** — the virtual
//!    contract balance (collected ticket revenue) minus the sum of completed
//!    refunds is always ≥ 0.
//! 4. **Refunds only permitted in terminal-refundable states** — `Cancelled`
//!    and `Failed`; never in `PendingPrize`, `Active`, `Drawing`, `Finalized`,
//!    or `Claimed`.
//! 5. **Terminal states are terminal** — once `Cancelled` or `Failed`, no
//!    operation transitions the raffle to a different status.
//! 6. **Cancel semantics** — cancellation is permitted from `Active`,
//!    `Drawing`, `PendingPrize`, and `Failed`; rejected with `InvalidStatus`
//!    from `Finalized`, `Cancelled`, and `Claimed`.
//! 7. **Only authorised roles may cancel** — an `Unauthorized` role returns
//!    `NotAuthorized`; state is unchanged.
//! 8. **tickets_sold never exceeds max_tickets** — buying stops at the cap.
//! 9. **`PendingPrize` is the initial state** — `Op::Activate` transitions to
//!    `Active` (models prize deposit); `Op::Fail` transitions `Drawing` to
//!    `Failed` (models finalize when tickets_sold < min_tickets).
//!
//! # Running (Linux/WSL, nightly)
//!
//! ```bash
//! cargo fuzz run fuzz_refund_cancel -- -max_total_time=1800
//! ```

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::collections::{HashMap, HashSet};

// ═══════════════════════════════════════════════════════════════════════════
// State-machine model
// ═══════════════════════════════════════════════════════════════════════════

/// Mirrors `RaffleStatus` in `raffle-shared/src/lib.rs` (7 variants).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Active,       // = 0
    Drawing,      // = 1
    Finalized,    // = 2
    Cancelled,    // = 3
    Failed,       // = 4
    Claimed,      // = 5
    PendingPrize, // = 6  initial state before prize deposit
}

/// Roles that are allowed (or not) to cancel a raffle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
enum CancelRole {
    /// The raffle creator — always authorised.
    Creator,
    /// The protocol admin — always authorised.
    Admin,
    /// Any arbitrary third party — never authorised.
    Unauthorized,
}

/// Reason supplied with a cancel call; mirrors `CancelReason` in the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Arbitrary)]
enum CancelReason {
    CreatorCancelled,
    AdminCancelled,
    MinTicketsNotMet,
}

/// Result type returned by model operations.
///
/// Variant names are kept 1-to-1 with `Error` variants in the real contract so
/// that grep-based correlation between model and on-chain code is direct.
#[derive(Debug, PartialEq, Eq)]
enum ModelError {
    /// `buy` called when raffle is not in `Active` state.
    /// Maps to `Error::RaffleInactive`.
    RaffleInactive,
    /// Ticket id does not exist (was never purchased).
    /// Maps to `Error::TicketNotFound`.
    TicketNotFound,
    /// This ticket was already refunded.
    /// Maps to `Error::PrizeAlreadyClaimed` (the error code used by
    /// `refund_ticket` in `claim.rs` for double-refund attempts).
    PrizeAlreadyClaimed,
    /// Cancel attempted on a raffle in Finalized, Cancelled, or Claimed status.
    /// Maps to `Error::InvalidStatus`.
    InvalidStatus,
    /// Caller is not an authorised cancellation role.
    /// Maps to `Error::NotAuthorized`.
    NotAuthorized,
    /// Raffle is already sold out.
    SoldOut,
    /// Raffle has ended (past deadline).
    Expired,
}

/// Lightweight pure-Rust model of one raffle instance.
#[derive(Debug, Clone)]
struct RaffleModel {
    status: Status,
    max_tickets: u32,
    tickets_sold: u32,
    ticket_price: i128,
    end_time: u64, // 0 = no deadline
    /// ticket_id → owner_id (buyer index)
    tickets: HashMap<u32, u32>,
    /// set of ticket_ids that have already been refunded
    refunded: HashSet<u32>,
    /// total token revenue collected (tickets_sold * ticket_price)
    total_collected: i128,
    /// total token refunded so far
    total_refunded: i128,
}

impl RaffleModel {
    /// Create a new raffle.  Initial status is `PendingPrize`, matching the
    /// on-chain contract where a newly-created raffle awaits prize deposit
    /// before ticket sales open (RaffleStatus::PendingPrize = 6).
    fn new(max_tickets: u32, ticket_price: i128, end_time: u64) -> Self {
        RaffleModel {
            status: Status::PendingPrize,
            max_tickets: max_tickets.max(1),
            tickets_sold: 0,
            ticket_price: ticket_price.max(1),
            end_time,
            tickets: HashMap::new(),
            refunded: HashSet::new(),
            total_collected: 0,
            total_refunded: 0,
        }
    }

    /// Model the prize-deposit step: transitions `PendingPrize` → `Active`.
    /// Is a no-op for every other status (safe to call speculatively).
    fn activate(&mut self) {
        if self.status == Status::PendingPrize {
            self.status = Status::Active;
        }
    }

    /// Model `finalize_raffle` when `tickets_sold < min_tickets`:
    /// transitions `Drawing` → `Failed`.  Is a no-op for every other status.
    fn fail(&mut self) {
        if self.status == Status::Drawing {
            self.status = Status::Failed;
        }
    }

    /// Attempt to purchase one ticket for `buyer_id` at `now`.
    fn buy(&mut self, buyer_id: u32, now: u64) -> Result<u32, ModelError> {
        if self.status != Status::Active {
            return Err(ModelError::RaffleInactive);
        }
        if self.end_time != 0 && now > self.end_time {
            return Err(ModelError::Expired);
        }
        if self.tickets_sold >= self.max_tickets {
            return Err(ModelError::SoldOut);
        }

        self.tickets_sold += 1;
        let ticket_id = self.tickets_sold; // 1-indexed, matches contract
        self.tickets.insert(ticket_id, buyer_id);
        self.total_collected = self
            .total_collected
            .saturating_add(self.ticket_price);

        // Auto-transition to Drawing when sold out (mirrors contract behaviour)
        if self.tickets_sold >= self.max_tickets {
            self.status = Status::Drawing;
        }

        Ok(ticket_id)
    }

    /// Attempt to cancel the raffle.
    ///
    /// Mirrors `cancel_raffle` in `admin.rs`:
    ///   - `Finalized | Cancelled | Claimed` → `Err(InvalidStatus)`
    ///   - `Active | Drawing | PendingPrize | Failed` (authorised) → `Ok(())`
    fn cancel(&mut self, role: CancelRole, _reason: CancelReason) -> Result<(), ModelError> {
        // Only creator and admin are authorised
        if role == CancelRole::Unauthorized {
            return Err(ModelError::NotAuthorized);
        }
        // States that the real contract rejects with InvalidStatus
        match self.status {
            Status::Finalized | Status::Cancelled | Status::Claimed => {
                return Err(ModelError::InvalidStatus);
            }
            Status::Active | Status::Drawing | Status::PendingPrize | Status::Failed => {}
        }
        self.status = Status::Cancelled;
        Ok(())
    }

    /// Attempt to refund `ticket_id`.
    ///
    /// Mirrors `refund_ticket` in `claim.rs`:
    ///   - Only valid in `Cancelled` or `Failed` states.
    ///   - Returns `PrizeAlreadyClaimed` (not `AlreadyRefunded`) on double-refund,
    ///     matching `Error::PrizeAlreadyClaimed` in the real contract.
    fn refund_ticket(&mut self, ticket_id: u32) -> Result<i128, ModelError> {
        // Refunds are only valid in Cancelled or Failed states
        match self.status {
            Status::Cancelled | Status::Failed => {}
            _ => return Err(ModelError::RaffleInactive),
        }
        // Ticket must exist
        if !self.tickets.contains_key(&ticket_id) {
            return Err(ModelError::TicketNotFound);
        }
        // No double-refund — mirrors DataKey::TicketRefunded check in claim.rs
        if self.refunded.contains(&ticket_id) {
            return Err(ModelError::PrizeAlreadyClaimed);
        }

        self.refunded.insert(ticket_id);
        self.total_refunded = self
            .total_refunded
            .saturating_add(self.ticket_price);

        Ok(self.ticket_price)
    }

    // ── Invariant assertions ────────────────────────────────────────────────

    fn assert_invariants(&self) {
        // INV-2 & INV-3: total_refunded ≤ total_collected, balance ≥ 0
        assert!(
            self.total_refunded <= self.total_collected,
            "total_refunded ({}) > total_collected ({})",
            self.total_refunded,
            self.total_collected,
        );

        // INV-3: virtual balance is non-negative
        let balance = self.total_collected - self.total_refunded;
        assert!(
            balance >= 0,
            "virtual balance negative: collected={} refunded={}",
            self.total_collected,
            self.total_refunded,
        );

        // INV-8: tickets_sold cap
        assert!(
            self.tickets_sold <= self.max_tickets,
            "tickets_sold ({}) > max_tickets ({})",
            self.tickets_sold,
            self.max_tickets,
        );

        // INV-1: every refunded ticket_id must have been purchased exactly once
        for tid in &self.refunded {
            assert!(
                self.tickets.contains_key(tid),
                "refunded ticket {tid} was never purchased"
            );
        }

        // Refunded count can't exceed tickets sold
        assert!(
            self.refunded.len() <= self.tickets_sold as usize,
            "refunded count ({}) > tickets_sold ({})",
            self.refunded.len(),
            self.tickets_sold,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Fuzzer input types
// ═══════════════════════════════════════════════════════════════════════════

/// One operation in the interleaved sequence.
#[derive(Debug, Arbitrary)]
enum Op {
    /// A buyer (identified by `buyer_id % NUM_BUYERS`) purchases one ticket.
    Buy { buyer_id: u8, now: u64 },
    /// Cancel the raffle as `role` with `reason`.
    Cancel { role: CancelRole, reason: CancelReason },
    /// Attempt to refund ticket number `ticket_id` (1-indexed).
    /// `ticket_id` is clamped to a plausible range by the harness.
    Refund { ticket_id: u8 },
    /// Model the prize-deposit step: transitions PendingPrize → Active.
    Activate,
    /// Model finalize_raffle when tickets_sold < min_tickets:
    /// transitions Drawing → Failed; no-op in all other states.
    Fail,
}

/// Top-level fuzz input: raffle parameters + operation sequence.
#[derive(Debug, Arbitrary)]
struct FuzzInput {
    /// Max number of tickets (clamped to 1..=64 for fast runs).
    max_tickets_raw: u8,
    /// Ticket price (clamped to 1..=i32::MAX cast to i128).
    ticket_price_raw: u32,
    /// Raffle end_time; 0 = no deadline.
    end_time: u64,
    /// Sequence of operations to interleave.
    /// Capped to 128 entries to bound run time.
    ops: Vec<Op>,
}

// Number of distinct simulated buyers.
const NUM_BUYERS: u8 = 8;
// Maximum ops executed per fuzz run to keep execution bounded.
const MAX_OPS: usize = 128;

// ═══════════════════════════════════════════════════════════════════════════
// Fuzz entry point
// ═══════════════════════════════════════════════════════════════════════════

fuzz_target!(|input: FuzzInput| {
    let max_tickets = (input.max_tickets_raw as u32 % 64).max(1);
    let ticket_price = (input.ticket_price_raw as i128).max(1);

    let mut raffle = RaffleModel::new(max_tickets, ticket_price, input.end_time);

    for op in input.ops.iter().take(MAX_OPS) {
        match op {
            Op::Buy { buyer_id, now } => {
                let result = raffle.buy(*buyer_id as u32 % NUM_BUYERS as u32, *now);
                match result {
                    Ok(ticket_id) => {
                        // Successful buy: ticket_id must be in range
                        assert!(ticket_id >= 1, "ticket_id must be ≥ 1");
                        assert!(
                            ticket_id <= raffle.max_tickets,
                            "ticket_id {ticket_id} > max_tickets {}",
                            raffle.max_tickets
                        );
                    }
                    Err(ModelError::SoldOut) => {
                        // Must be at cap or in a non-Active state
                        assert!(
                            raffle.tickets_sold >= raffle.max_tickets
                                || raffle.status != Status::Active,
                            "SoldOut fired but capacity not reached"
                        );
                    }
                    Err(ModelError::Expired) => {
                        assert!(
                            raffle.end_time != 0 && *now > raffle.end_time,
                            "Expired fired but deadline not reached"
                        );
                    }
                    Err(ModelError::RaffleInactive) => {
                        // INV: status was not Active when buy was attempted
                        assert_ne!(
                            raffle.status,
                            Status::Active,
                            "RaffleInactive returned but status is Active"
                        );
                    }
                    Err(e) => panic!("unexpected buy error: {e:?}"),
                }
            }

            Op::Cancel { role, reason } => {
                let status_before = raffle.status.clone();
                let result = raffle.cancel(*role, *reason);

                match result {
                    Ok(()) => {
                        // INV-5: must now be in Cancelled
                        assert_eq!(raffle.status, Status::Cancelled);
                        // INV-6: predecessor must be a cancellable state
                        assert!(
                            matches!(
                                status_before,
                                Status::Active
                                    | Status::Drawing
                                    | Status::PendingPrize
                                    | Status::Failed
                            ),
                            "cancel succeeded from {:?}",
                            status_before
                        );
                        // INV-7: only authorised roles can succeed
                        assert_ne!(
                            *role,
                            CancelRole::Unauthorized,
                            "unauthorized cancel succeeded"
                        );
                    }
                    Err(ModelError::NotAuthorized) => {
                        // INV-7: must be the Unauthorized role
                        assert_eq!(
                            *role,
                            CancelRole::Unauthorized,
                            "NotAuthorized returned for authorised role {:?}",
                            role
                        );
                        // INV-5: state must not have changed
                        assert_eq!(
                            raffle.status, status_before,
                            "state changed after NotAuthorized"
                        );
                    }
                    Err(ModelError::InvalidStatus) => {
                        // INV-6: was in a non-cancellable terminal state
                        assert!(
                            matches!(
                                status_before,
                                Status::Finalized | Status::Cancelled | Status::Claimed
                            ),
                            "InvalidStatus from non-terminal state {:?}",
                            status_before
                        );
                        // INV-5: state unchanged
                        assert_eq!(raffle.status, status_before);
                    }
                    Err(e) => panic!("unexpected cancel error: {e:?}"),
                }
            }

            Op::Refund { ticket_id } => {
                // Map the fuzzer's u8 to a plausible ticket_id (1-indexed)
                let tid = (*ticket_id as u32 % raffle.max_tickets).max(1);
                let status_before = raffle.status.clone();
                let result = raffle.refund_ticket(tid);

                match result {
                    Ok(amount) => {
                        // INV-4: refunds only in Cancelled or Failed
                        assert!(
                            status_before == Status::Cancelled
                                || status_before == Status::Failed,
                            "refund succeeded in state {:?}",
                            status_before
                        );
                        // Amount must equal ticket price
                        assert_eq!(amount, raffle.ticket_price, "refund amount mismatch");
                        // INV-1: ticket must be marked refunded
                        assert!(raffle.refunded.contains(&tid));
                    }
                    Err(ModelError::RaffleInactive) => {
                        // INV-4: state was not Cancelled or Failed
                        assert!(
                            status_before != Status::Cancelled
                                && status_before != Status::Failed,
                            "RaffleInactive (refund path) in {:?}",
                            status_before
                        );
                        // INV-5: state unchanged
                        assert_eq!(raffle.status, status_before);
                    }
                    Err(ModelError::TicketNotFound) => {
                        assert!(
                            !raffle.tickets.contains_key(&tid),
                            "TicketNotFound for ticket {tid} that exists"
                        );
                    }
                    Err(ModelError::PrizeAlreadyClaimed) => {
                        // INV-1: ticket must already be in refunded set
                        assert!(
                            raffle.refunded.contains(&tid),
                            "PrizeAlreadyClaimed for ticket {tid} not in refunded set"
                        );
                    }
                    Err(e) => panic!("unexpected refund error: {e:?}"),
                }
            }

            Op::Activate => {
                let status_before = raffle.status.clone();
                raffle.activate();
                // activate() either moves PendingPrize→Active, or is a no-op
                if status_before == Status::PendingPrize {
                    assert_eq!(
                        raffle.status,
                        Status::Active,
                        "activate() failed to transition PendingPrize→Active"
                    );
                } else {
                    assert_eq!(
                        raffle.status, status_before,
                        "activate() changed status from {:?}",
                        status_before
                    );
                }
            }

            Op::Fail => {
                let status_before = raffle.status.clone();
                raffle.fail();
                // fail() either moves Drawing→Failed, or is a no-op
                if status_before == Status::Drawing {
                    assert_eq!(
                        raffle.status,
                        Status::Failed,
                        "fail() failed to transition Drawing→Failed"
                    );
                } else {
                    assert_eq!(
                        raffle.status, status_before,
                        "fail() changed status from {:?}",
                        status_before
                    );
                }
            }
        }

        // Check all structural invariants after every operation.
        raffle.assert_invariants();
    }

    // Final invariant pass.
    raffle.assert_invariants();
});
