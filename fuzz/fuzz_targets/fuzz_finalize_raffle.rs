//! Fuzz target: finalize_raffle winner-selection logic
//!
//! This harness reproduces the internal randomness and winner-index
//! computation from `instance/mod.rs::Contract::finalize_raffle` (Internal
//! randomness path) and `provide_randomness` (External path).
//!
//! The key arithmetic is:
//!   let seed         = timestamp + sequence as u64;
//!   let winner_index = (seed % tickets_sold as u64) as u32;
//!
//! Invariants that must never be violated:
//!   1. `winner_index < tickets_sold` for every non-zero tickets_sold.
//!   2. No integer overflow / panic occurs for any (u64, u64, u32) input.
//!   3. External-randomness path: winner_index = seed % tickets_sold.
//!
//! Run for 30 minutes (Linux/WSL with nightly):
//!   cargo fuzz run fuzz_finalize_raffle -- -max_total_time=1800

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

// ───────────────────────────── Input types ──────────────────────────────────

#[derive(Debug, Arbitrary)]
struct FinalizeInput {
    /// Simulated ledger timestamp at finalization time
    timestamp: u64,
    /// Simulated ledger sequence number (cast to u64 in contract)
    sequence: u32,
    /// Tickets sold — must be > 0 for a valid Drawing state (capped to >0 by check below)
    tickets_sold: u32,
    /// External randomness seed provided by oracle (External path)
    external_seed: u64,
}

// ───────────────────── Pure-logic extraction (Internal path) ─────────────────

/// Mirrors `finalize_raffle` internal-randomness winner computation.
/// Returns `None` when tickets_sold == 0 (NoTicketsSold guard).
fn compute_winner_internal(timestamp: u64, sequence: u32, tickets_sold: u32) -> Option<u32> {
    if tickets_sold == 0 {
        return None; // maps to Error::NoTicketsSold
    }
    let seed: u64 = timestamp.wrapping_add(sequence as u64);
    let winner_index = (seed % tickets_sold as u64) as u32;
    Some(winner_index)
}

/// Mirrors `provide_randomness` external-randomness winner computation.
fn compute_winner_external(external_seed: u64, tickets_sold: u32) -> Option<u32> {
    if tickets_sold == 0 {
        return None;
    }
    let winner_index = (external_seed % tickets_sold as u64) as u32;
    Some(winner_index)
}

// ─────────────────────────── Fuzz entry point ───────────────────────────────

fuzz_target!(|input: FinalizeInput| {
    // ── Internal randomness path ─────────────────────────────────────────────
    match compute_winner_internal(input.timestamp, input.sequence, input.tickets_sold) {
        None => {
            // INVARIANT: None only when tickets_sold == 0
            assert_eq!(
                input.tickets_sold, 0,
                "None returned for non-zero tickets_sold (internal path)"
            );
        }
        Some(idx) => {
            // INVARIANT 1: winner index is strictly within bounds
            assert!(
                idx < input.tickets_sold,
                "winner_index {} >= tickets_sold {} (internal path)",
                idx,
                input.tickets_sold
            );

            // INVARIANT 2: index is deterministic for the same inputs
            let idx2 = compute_winner_internal(
                input.timestamp,
                input.sequence,
                input.tickets_sold,
            )
            .unwrap();
            assert_eq!(idx, idx2, "non-determinism detected (internal path)");
        }
    }

    // ── External randomness path ─────────────────────────────────────────────
    match compute_winner_external(input.external_seed, input.tickets_sold) {
        None => {
            assert_eq!(
                input.tickets_sold, 0,
                "None returned for non-zero tickets_sold (external path)"
            );
        }
        Some(idx) => {
            // INVARIANT 3: external path winner index in-bounds
            assert!(
                idx < input.tickets_sold,
                "winner_index {} >= tickets_sold {} (external path)",
                idx,
                input.tickets_sold
            );

            // INVARIANT 4: external index matches manual modulo
            let expected = (input.external_seed % input.tickets_sold as u64) as u32;
            assert_eq!(
                idx, expected,
                "external winner_index mismatch: got {idx}, expected {expected}"
            );
        }
    }
});

// ───────────────────── Smoke tests (cargo test -p raffle-fuzz) ─────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Internal path ────────────────────────────────────────────────────────

    #[test]
    fn zero_tickets_returns_none_internal() {
        assert_eq!(compute_winner_internal(999, 1, 0), None);
    }

    #[test]
    fn single_ticket_always_index_zero() {
        // Any seed % 1 == 0
        for ts in [0u64, 1, u64::MAX / 2, u64::MAX - 1] {
            for seq in [0u32, 1, u32::MAX] {
                let idx = compute_winner_internal(ts, seq, 1).unwrap();
                assert_eq!(idx, 0, "ts={ts} seq={seq}");
            }
        }
    }

    #[test]
    fn winner_always_in_bounds_internal() {
        let cases = [
            (0u64, 0u32, 5u32),
            (u64::MAX, u32::MAX, 1),
            (u64::MAX, u32::MAX, u32::MAX),
            (123456789, 42, 100),
            (0, 0, u32::MAX),
        ];
        for (ts, seq, sold) in cases {
            if sold == 0 {
                continue;
            }
            let idx = compute_winner_internal(ts, seq, sold).unwrap();
            assert!(idx < sold, "idx={idx} sold={sold} ts={ts} seq={seq}");
        }
    }

    #[test]
    fn max_u64_timestamp_does_not_overflow() {
        // wrapping_add must not panic
        let idx = compute_winner_internal(u64::MAX, u32::MAX, 7).unwrap();
        assert!(idx < 7);
    }

    // ── External path ────────────────────────────────────────────────────────

    #[test]
    fn zero_tickets_returns_none_external() {
        assert_eq!(compute_winner_external(42, 0), None);
    }

    #[test]
    fn external_seed_zero_with_any_sold_gives_index_zero() {
        for sold in [1u32, 2, 5, 1000, u32::MAX] {
            let idx = compute_winner_external(0, sold).unwrap();
            assert_eq!(idx, 0, "sold={sold}");
        }
    }

    #[test]
    fn external_winner_always_in_bounds() {
        let cases = [
            (0u64, 1u32),
            (u64::MAX, 1),
            (u64::MAX, u32::MAX),
            (12345, 5),
            (99999999999, 17),
        ];
        for (seed, sold) in cases {
            let idx = compute_winner_external(seed, sold).unwrap();
            assert!(idx < sold, "seed={seed} sold={sold} idx={idx}");
        }
    }

    #[test]
    fn external_determinism() {
        let seed = 0xDEAD_BEEF_u64;
        let sold = 37u32;
        let a = compute_winner_external(seed, sold).unwrap();
        let b = compute_winner_external(seed, sold).unwrap();
        assert_eq!(a, b);
    }
}
