//! Fuzz target: buy_ticket logic
//!
//! This harness reproduces the pure numeric and state-machine guards from
//! `instance/mod.rs::Contract::buy_ticket` without any Soroban host calls.
//! The fuzzer throws arbitrary inputs at the logic and asserts invariants
//! that must hold unconditionally.
//!
//! Run for 30 minutes (Linux/WSL with nightly):
//!   cargo fuzz run fuzz_buy_ticket -- -max_total_time=1800

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

// ───────────────────────────── Input types ──────────────────────────────────

/// Every configurable piece of state the buy_ticket function reads.
#[derive(Debug, Arbitrary)]
struct BuyTicketInput {
    /// Raffle's max ticket cap (clamped to 1..=u16::MAX to keep runs fast)
    max_tickets: u16,
    /// Tickets already sold before this call
    tickets_sold: u16,
    /// Whether the raffle allows one address to hold multiple tickets
    allow_multiple: bool,
    /// How many tickets this buyer already holds (0 = first purchase)
    buyer_existing_count: u32,
    /// Ticket price (must be > 0 in a valid raffle; fuzzer may feed 0/neg)
    ticket_price: i64,
    /// Raffle end_time in ledger seconds (0 = no deadline)
    end_time: u64,
    /// Simulated current ledger timestamp
    now: u64,
}

// ───────────────────────────── Error mirror ─────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
enum BuyError {
    RaffleInactive,       // status != Active  (not tested here; pre-condition)
    RaffleEnded,          // past end_time
    TicketsSoldOut,       // tickets_sold >= max_tickets
    MultipleTicketsNotAllowed,
}

// ─────────────────────── Pure-logic extraction ──────────────────────────────

/// Mirror of the guard logic inside `buy_ticket`.
fn check_buy_ticket(input: &BuyTicketInput) -> Result<u16, BuyError> {
    let max = input.max_tickets as u32;
    let sold = input.tickets_sold as u32;

    // Guard: raffle deadline
    if input.end_time != 0 && input.now > input.end_time {
        return Err(BuyError::RaffleEnded);
    }

    // Guard: sold out
    if sold >= max {
        return Err(BuyError::TicketsSoldOut);
    }

    // Guard: single-ticket-per-address policy
    if !input.allow_multiple && input.buyer_existing_count > 0 {
        return Err(BuyError::MultipleTicketsNotAllowed);
    }

    // Happy path: tickets_sold increments by exactly 1
    let new_sold = sold
        .checked_add(1)
        .expect("tickets_sold overflow — impossible if max_tickets fits in u32");

    Ok(new_sold as u16)
}

// ─────────────────────────── Fuzz entry point ───────────────────────────────

fuzz_target!(|input: BuyTicketInput| {
    let result = check_buy_ticket(&input);

    let max = input.max_tickets as u32;
    let sold = input.tickets_sold as u32;

    match &result {
        Ok(new_sold) => {
            // INVARIANT 1: success only when no guard fired
            let end_ok = input.end_time == 0 || input.now <= input.end_time;
            assert!(end_ok, "succeeded past end_time");

            assert!(sold < max, "succeeded when sold-out");

            let multi_ok = input.allow_multiple || input.buyer_existing_count == 0;
            assert!(multi_ok, "succeeded despite multiple-ticket violation");

            // INVARIANT 2: tickets_sold increments by exactly 1
            assert_eq!(*new_sold as u32, sold + 1, "tickets_sold did not increment by 1");
        }
        Err(BuyError::RaffleEnded) => {
            // Must only fire when past deadline
            assert!(
                input.end_time != 0 && input.now > input.end_time,
                "RaffleEnded fired but deadline not passed"
            );
        }
        Err(BuyError::TicketsSoldOut) => {
            // Must only fire when cap reached
            assert!(
                sold >= max,
                "TicketsSoldOut fired but capacity not reached"
            );
        }
        Err(BuyError::MultipleTicketsNotAllowed) => {
            // Must only fire when policy violated
            assert!(
                !input.allow_multiple && input.buyer_existing_count > 0,
                "MultipleTicketsNotAllowed fired spuriously"
            );
        }
        Err(BuyError::RaffleInactive) => {
            // This variant is listed for completeness; the harness never returns it.
            unreachable!("RaffleInactive is not reachable in this harness");
        }
    }
});

// ────────────────────── Smoke tests (cargo test -p raffle-fuzz) ─────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: BuyTicketInput) -> Result<u16, BuyError> {
        check_buy_ticket(&input)
    }

    #[test]
    fn sold_out_returns_error() {
        let r = run(BuyTicketInput {
            max_tickets: 5,
            tickets_sold: 5,
            allow_multiple: true,
            buyer_existing_count: 0,
            ticket_price: 10,
            end_time: 0,
            now: 100,
        });
        assert_eq!(r, Err(BuyError::TicketsSoldOut));
    }

    #[test]
    fn past_deadline_returns_error() {
        let r = run(BuyTicketInput {
            max_tickets: 10,
            tickets_sold: 0,
            allow_multiple: true,
            buyer_existing_count: 0,
            ticket_price: 1,
            end_time: 50,
            now: 51,
        });
        assert_eq!(r, Err(BuyError::RaffleEnded));
    }

    #[test]
    fn no_deadline_never_expires() {
        let r = run(BuyTicketInput {
            max_tickets: 10,
            tickets_sold: 0,
            allow_multiple: true,
            buyer_existing_count: 0,
            ticket_price: 1,
            end_time: 0,
            now: u64::MAX,
        });
        assert_eq!(r, Ok(1));
    }

    #[test]
    fn multiple_tickets_blocked() {
        let r = run(BuyTicketInput {
            max_tickets: 10,
            tickets_sold: 1,
            allow_multiple: false,
            buyer_existing_count: 1,
            ticket_price: 10,
            end_time: 0,
            now: 0,
        });
        assert_eq!(r, Err(BuyError::MultipleTicketsNotAllowed));
    }

    #[test]
    fn multiple_tickets_allowed_when_flag_set() {
        let r = run(BuyTicketInput {
            max_tickets: 10,
            tickets_sold: 1,
            allow_multiple: true,
            buyer_existing_count: 3,
            ticket_price: 10,
            end_time: 0,
            now: 0,
        });
        assert_eq!(r, Ok(2));
    }

    #[test]
    fn first_ticket_always_succeeds() {
        let r = run(BuyTicketInput {
            max_tickets: u16::MAX,
            tickets_sold: 0,
            allow_multiple: false,
            buyer_existing_count: 0,
            ticket_price: i64::MAX,
            end_time: 0,
            now: 0,
        });
        assert_eq!(r, Ok(1));
    }

    #[test]
    fn sold_increments_by_exactly_one() {
        for sold in [0u16, 1, 10, 100, 999] {
            let r = run(BuyTicketInput {
                max_tickets: u16::MAX,
                tickets_sold: sold,
                allow_multiple: true,
                buyer_existing_count: 0,
                ticket_price: 5,
                end_time: 0,
                now: 0,
            });
            assert_eq!(r, Ok(sold + 1), "sold={sold}");
        }
    }
}
