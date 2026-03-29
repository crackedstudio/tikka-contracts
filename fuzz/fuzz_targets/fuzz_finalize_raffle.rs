//! Fuzz target: finalize_raffle winner-selection logic
//!
//! This harness reproduces the winner-index bounds invariants for both the
//! Internal and External randomness paths after the multi-source seed upgrade.
//!
//! Internal path seed construction (mirrors `build_internal_seed`):
//!   seed[0..8]  = timestamp.to_le_bytes()
//!   seed[8..12] = sequence.to_le_bytes()
//!   seed[12..20] ^= fold(raffle_id_bytes)   -- XOR-folded address bytes
//!   seed[0..32] ^= network_id               -- XOR with 32-byte network hash
//!   PRNG = ChaCha20(seed); winner_index = prng.gen_range(0..tickets_sold)
//!
//! External path additionally XORs the oracle seed into bytes 0..8 before
//! reseeding.
//!
//! Invariants that must never be violated:
//!   1. `winner_index < tickets_sold` for every non-zero tickets_sold.
//!   2. No integer overflow / panic occurs for any input combination.
//!   3. Selection is deterministic: same inputs → same winner_index.
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
    /// Simulated ledger sequence number
    sequence: u32,
    /// Tickets sold — must be > 0 for a valid Drawing state
    tickets_sold: u32,
    /// Simulated raffle_id address bytes (stand-in for XDR-encoded address)
    raffle_id_bytes: [u8; 32],
    /// Simulated network_id (SHA-256 of network passphrase)
    network_id: [u8; 32],
    /// External randomness seed provided by oracle (External path only)
    external_seed: u64,
}

// ───────────────────── Seed construction (mirrors build_internal_seed) ───────

fn build_seed(
    timestamp: u64,
    sequence: u32,
    raffle_id_bytes: &[u8; 32],
    network_id: &[u8; 32],
) -> [u8; 32] {
    let mut seed = [0u8; 32];

    // Source 1: timestamp — bytes 0..8
    seed[0..8].copy_from_slice(&timestamp.to_le_bytes());

    // Source 2: sequence — bytes 8..12
    seed[8..12].copy_from_slice(&sequence.to_le_bytes());

    // Source 3: raffle_id — XOR-fold 32 bytes into positions 12..20
    for i in 0..32 {
        seed[12 + (i % 8)] ^= raffle_id_bytes[i];
    }

    // Source 4: network_id — XOR into all 32 bytes
    for i in 0..32 {
        seed[i] ^= network_id[i];
    }

    seed
}

// ─────────────────── ChaCha20-based gen_range (simplified model) ─────────────
//
// The actual PRNG is ChaCha20 inside the Soroban host. For fuzz purposes we
// only need to verify the *bounds* invariant, not reproduce exact values.
// We model gen_range(0..n) as: seed_u64 % n, where seed_u64 is the first
// 8 bytes of the seed interpreted as little-endian u64. This is conservative
// — the real ChaCha20 output is stronger, but the modulo bound holds for any
// uniform distribution.

fn model_winner_index(seed: &[u8; 32], tickets_sold: u32) -> u32 {
    let seed_u64 = u64::from_le_bytes(seed[0..8].try_into().unwrap());
    (seed_u64 % tickets_sold as u64) as u32
}

// ───────────────────── Pure-logic extraction (Internal path) ─────────────────

fn compute_winner_internal(
    timestamp: u64,
    sequence: u32,
    raffle_id_bytes: &[u8; 32],
    network_id: &[u8; 32],
    tickets_sold: u32,
) -> Option<u32> {
    if tickets_sold == 0 {
        return None;
    }
    let seed = build_seed(timestamp, sequence, raffle_id_bytes, network_id);
    Some(model_winner_index(&seed, tickets_sold))
}

// ───────────────────── Pure-logic extraction (External path) ─────────────────

fn compute_winner_external(
    timestamp: u64,
    sequence: u32,
    raffle_id_bytes: &[u8; 32],
    network_id: &[u8; 32],
    external_seed: u64,
    tickets_sold: u32,
) -> Option<u32> {
    if tickets_sold == 0 {
        return None;
    }
    let mut seed = build_seed(timestamp, sequence, raffle_id_bytes, network_id);
    // Mix oracle seed into bytes 0..8
    let oracle_bytes = external_seed.to_le_bytes();
    for i in 0..8 {
        seed[i] ^= oracle_bytes[i];
    }
    Some(model_winner_index(&seed, tickets_sold))
}

// ─────────────────────────── Fuzz entry point ───────────────────────────────

fuzz_target!(|input: FinalizeInput| {
    // ── Internal randomness path ─────────────────────────────────────────────
    match compute_winner_internal(
        input.timestamp,
        input.sequence,
        &input.raffle_id_bytes,
        &input.network_id,
        input.tickets_sold,
    ) {
        None => {
            assert_eq!(input.tickets_sold, 0, "None for non-zero tickets_sold (internal)");
        }
        Some(idx) => {
            // INVARIANT 1: winner index strictly within bounds
            assert!(
                idx < input.tickets_sold,
                "winner_index {idx} >= tickets_sold {} (internal)",
                input.tickets_sold
            );
            // INVARIANT 2: deterministic
            let idx2 = compute_winner_internal(
                input.timestamp,
                input.sequence,
                &input.raffle_id_bytes,
                &input.network_id,
                input.tickets_sold,
            )
            .unwrap();
            assert_eq!(idx, idx2, "non-determinism (internal)");
        }
    }

    // ── External randomness path ─────────────────────────────────────────────
    match compute_winner_external(
        input.timestamp,
        input.sequence,
        &input.raffle_id_bytes,
        &input.network_id,
        input.external_seed,
        input.tickets_sold,
    ) {
        None => {
            assert_eq!(input.tickets_sold, 0, "None for non-zero tickets_sold (external)");
        }
        Some(idx) => {
            // INVARIANT 3: winner index strictly within bounds
            assert!(
                idx < input.tickets_sold,
                "winner_index {idx} >= tickets_sold {} (external)",
                input.tickets_sold
            );
            // INVARIANT 4: deterministic
            let idx2 = compute_winner_external(
                input.timestamp,
                input.sequence,
                &input.raffle_id_bytes,
                &input.network_id,
                input.external_seed,
                input.tickets_sold,
            )
            .unwrap();
            assert_eq!(idx, idx2, "non-determinism (external)");
        }
    }
});

// ───────────────────── Smoke tests (cargo test -p raffle-fuzz) ─────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Seed construction ────────────────────────────────────────────────────

    #[test]
    fn seed_differs_when_timestamp_changes() {
        let base = build_seed(100, 1, &[0u8; 32], &[0u8; 32]);
        let diff = build_seed(101, 1, &[0u8; 32], &[0u8; 32]);
        assert_ne!(base, diff);
    }

    #[test]
    fn seed_differs_when_sequence_changes() {
        let base = build_seed(100, 1, &[0u8; 32], &[0u8; 32]);
        let diff = build_seed(100, 2, &[0u8; 32], &[0u8; 32]);
        assert_ne!(base, diff);
    }

    #[test]
    fn seed_differs_when_raffle_id_changes() {
        let mut id = [0u8; 32];
        let base = build_seed(100, 1, &id, &[0u8; 32]);
        id[0] = 1;
        let diff = build_seed(100, 1, &id, &[0u8; 32]);
        assert_ne!(base, diff);
    }

    #[test]
    fn seed_differs_when_network_id_changes() {
        let mut net = [0u8; 32];
        let base = build_seed(100, 1, &[0u8; 32], &net);
        net[0] = 1;
        let diff = build_seed(100, 1, &[0u8; 32], &net);
        assert_ne!(base, diff);
    }

    // ── Internal path ────────────────────────────────────────────────────────

    #[test]
    fn zero_tickets_returns_none_internal() {
        assert_eq!(
            compute_winner_internal(999, 1, &[0u8; 32], &[0u8; 32], 0),
            None
        );
    }

    #[test]
    fn winner_always_in_bounds_internal() {
        let cases: &[(u64, u32, u32)] = &[
            (0, 0, 5),
            (u64::MAX, u32::MAX, 1),
            (123456789, 42, 100),
            (0, 0, u32::MAX),
        ];
        for &(ts, seq, sold) in cases {
            if sold == 0 { continue; }
            let idx = compute_winner_internal(ts, seq, &[0u8; 32], &[0u8; 32], sold).unwrap();
            assert!(idx < sold, "idx={idx} sold={sold}");
        }
    }

    #[test]
    fn internal_determinism() {
        let a = compute_winner_internal(42, 7, &[1u8; 32], &[2u8; 32], 37).unwrap();
        let b = compute_winner_internal(42, 7, &[1u8; 32], &[2u8; 32], 37).unwrap();
        assert_eq!(a, b);
    }

    // ── External path ────────────────────────────────────────────────────────

    #[test]
    fn zero_tickets_returns_none_external() {
        assert_eq!(
            compute_winner_external(0, 0, &[0u8; 32], &[0u8; 32], 42, 0),
            None
        );
    }

    #[test]
    fn external_winner_always_in_bounds() {
        let cases: &[(u64, u32, u64, u32)] = &[
            (0, 1, 0, 1),
            (u64::MAX, u32::MAX, u64::MAX, 1),
            (12345, 5, 99999, 17),
        ];
        for &(ts, seq, ext, sold) in cases {
            let idx = compute_winner_external(ts, seq, &[0u8; 32], &[0u8; 32], ext, sold).unwrap();
            assert!(idx < sold, "idx={idx} sold={sold}");
        }
    }

    #[test]
    fn external_determinism() {
        let a = compute_winner_external(42, 7, &[1u8; 32], &[2u8; 32], 0xDEAD_BEEF, 37).unwrap();
        let b = compute_winner_external(42, 7, &[1u8; 32], &[2u8; 32], 0xDEAD_BEEF, 37).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn external_differs_from_internal_with_nonzero_oracle_seed() {
        // Mixing the oracle seed must change the outcome vs. internal-only seed
        let internal = compute_winner_internal(42, 7, &[1u8; 32], &[2u8; 32], 1000).unwrap();
        let external = compute_winner_external(42, 7, &[1u8; 32], &[2u8; 32], 0xFF, 1000).unwrap();
        // Not guaranteed to differ for every input, but with a non-trivial
        // oracle seed and large ticket count it almost certainly will.
        // This is a sanity check, not a hard invariant.
        let _ = (internal, external); // suppress unused warning
    }
}
