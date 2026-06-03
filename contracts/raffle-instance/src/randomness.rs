use soroban_sdk::{xdr::ToXdr, Address, Bytes, BytesN, Env, Vec};

// ============================================================================
// build_internal_seed
// ============================================================================
//
// ⚠️  LOW-STAKES RAFFLES ONLY
//
// This seed is deterministic and visible on-chain.  Any participant who knows
// the ledger state at the time `finalize_raffle` is called can reproduce the
// exact output.  Miners / validators can also influence the ledger timestamp and
// sequence to bias the result.
//
// For high-stakes or high-value raffles, use `RandomnessSource::External` so
// that a VRF oracle provides a verifiably-unbiased seed that cannot be
// predicted or manipulated before `provide_randomness` is called.
//
// Entropy sources mixed into the seed:
//   1. `ledger_timestamp`  – wall-clock time in seconds
//   2. `ledger_sequence`   – monotonically-increasing ledger counter
//   3. `network_id`        – SHA-256 of the network passphrase (32 bytes),
//                            ensuring seeds are network-partitioned (mainnet ≠
//                            testnet ≠ futurenet)
//   4. `raffle_id`         – the raffle contract address in XDR encoding,
//                            making every raffle's draw independent even when
//                            finalized in the same ledger
//
// All four inputs are packed together and passed through `env.crypto().sha256`
// to produce a uniformly-distributed 32-byte value that is used as the PRNG
// seed via `env.prng().seed()`.

/// Builds a 32-byte internal PRNG seed by hashing four ledger entropy sources.
///
/// # Arguments
///
/// * `env`       – the contract execution environment
/// * `raffle_id` – the current contract's address (distinguishes concurrent raffles)
///
/// # Returns
///
/// A `BytesN<32>` suitable for passing directly to `env.prng().seed()`.
///
/// # Security note
///
/// **For low-stakes raffles only.**  See the module-level comment for a full
/// explanation of the limitations and the recommended alternative for
/// high-value draws.
#[allow(dead_code)]
pub fn build_internal_seed(env: &Env, raffle_id: &Address) -> BytesN<32> {
    let timestamp = env.ledger().timestamp();
    let sequence = env.ledger().sequence();
    let network_id: BytesN<32> = env.ledger().network_id();

    // Pack all four sources into a single byte buffer, then SHA-256 hash it.
    // Using XDR serialisation guarantees an unambiguous, length-delimited
    // encoding so there are no collisions between differently-typed fields.
    let raw: Bytes = (timestamp, sequence, network_id, raffle_id.clone()).to_xdr(env);
    hash_bytes32(env, &raw)
}

/// Hashes the input with SHA-256 and validates the result.
///
/// On congested ledgers, the crypto operation may fail due to resource limits.
/// In that case we expect the returned hash to be invalid rather than silently
/// falling back to a zeroed seed, which would make winner selection
/// deterministic and insecure.
#[allow(dead_code)]
fn hash_bytes32(env: &Env, input: &Bytes) -> BytesN<32> {
    let hash: BytesN<32> = env.crypto().sha256(input).into();
    if hash.to_array() == [0u8; 32] {
        panic!("crypto.sha256() failed: invalid hash output");
    }
    hash
}

/// Common winner-selection interface used by both PRNG and oracle paths.
pub trait WinnerSelectionStrategy {
    fn select_winner_indices(&self, env: &Env, total_tickets: u32, winner_count: u32) -> Vec<u32>;
}

/// Internal PRNG-based winner selection.
///
/// Uses [`build_internal_seed`] to construct a multi-source seed that is then
/// fed into `env.prng()`.  The same inputs always produce the same winners,
/// which allows off-chain verification of draws.
///
/// **For low-stakes raffles only** — see [`build_internal_seed`] for the full
/// security caveat.
#[allow(dead_code)]
pub struct PrngWinnerSelection {
    pub timestamp: u64,
    pub sequence: u32,
    pub raffle_id: Address,
    pub tickets_sold: u32,
}

#[allow(dead_code)]
impl PrngWinnerSelection {
    pub fn new(timestamp: u64, sequence: u32, raffle_id: Address, tickets_sold: u32) -> Self {
        Self {
            timestamp,
            sequence,
            raffle_id,
            tickets_sold,
        }
    }

    /// Returns a compact u64 fingerprint of the seed for inclusion in the
    /// on-chain `FairnessMetadata` event.  This is derived from the same
    /// inputs as the actual seed so it can be used to spot-check draws.
    pub fn seed_fingerprint(&self, env: &Env) -> u64 {
        // Mix the build_internal_seed output down to a u64 for the fairness proof.
        let seed_bytes: BytesN<32> = build_internal_seed(env, &self.raffle_id);
        let arr = seed_bytes.to_array();
        // Take the first 8 bytes as big-endian u64.
        u64::from_be_bytes([
            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
        ])
    }

    /// Returns the raw 32-byte seed as `Bytes` for `env.prng().seed()`.
    ///
    /// Wraps [`build_internal_seed`] and additionally mixes in `tickets_sold`
    /// so that two raffles with the same address finalized in the same ledger
    /// still differ if they have different ticket counts.
    fn seed_bytes(&self, env: &Env) -> Bytes {
        let base: BytesN<32> = build_internal_seed(env, &self.raffle_id);
        // XDR-pack the base seed + tickets_sold and re-hash to include the
        // extra entropy source without truncating the network_id contribution.
        let combined: Bytes = (base, self.tickets_sold).to_xdr(env);
        hash_bytes32(env, &combined).into()
    }
}

impl WinnerSelectionStrategy for PrngWinnerSelection {
    fn select_winner_indices(&self, env: &Env, total_tickets: u32, winner_count: u32) -> Vec<u32> {
        let mut indices = Vec::new(env);
        if total_tickets == 0 || winner_count == 0 {
            return indices;
        }

        // Seed the PRNG with the multi-source hash — see build_internal_seed
        // for details on the entropy inputs.
        env.prng().seed(self.seed_bytes(env));

        for _ in 0..winner_count {
            #[allow(deprecated)]
            let idx = env.prng().u64_in_range(0..(total_tickets as u64)) as u32;
            indices.push_back(idx);
        }

        indices
    }
}

/// Oracle-backed strategy using an externally provided VRF seed.
///
/// Used by [`provide_randomness`] after the oracle has delivered a
/// cryptographically-verified random value.  Not subject to the
/// manipulability concerns of the PRNG path.
pub struct OracleSeedWinnerSelection {
    seed: u64,
}

impl OracleSeedWinnerSelection {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }
}

impl WinnerSelectionStrategy for OracleSeedWinnerSelection {
    fn select_winner_indices(&self, env: &Env, total_tickets: u32, winner_count: u32) -> Vec<u32> {
        let mut indices = Vec::new(env);
        if total_tickets == 0 || winner_count == 0 {
            return indices;
        }

        // #257: Use rejection sampling to eliminate modulo bias.
        // We discard samples that fall in the biased tail so every ticket in
        // [0, total_tickets) is chosen with exactly equal probability.
        //
        // largest_multiple = floor(u64::MAX / total_tickets) * total_tickets
        // Any sample >= largest_multiple is rejected and the seed advanced.
        let n = total_tickets as u64;
        let largest_multiple = (u64::MAX / n) * n;

        let mut current_seed = self.seed;
        for _ in 0..winner_count {
            // Advance until the sample falls below the rejection threshold.
            let idx = loop {
                if current_seed < largest_multiple {
                    break (current_seed % n) as u32;
                }
                // Mix the seed to get a new candidate; wrapping_mul with a
                // large odd constant provides a fast, bias-free step.
                current_seed = current_seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
            };
            indices.push_back(idx);
            // Advance the seed for the next winner so picks are independent.
            current_seed = current_seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
        }

        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    /// build_internal_seed produces different values for different raffle IDs.
    #[test]
    fn build_internal_seed_differs_by_raffle_id() {
        let env = Env::default();
        let id_a = Address::generate(&env);
        let id_b = Address::generate(&env);
        let contract = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();

        let (seed_a, seed_b) = env.as_contract(&contract, || {
            (
                build_internal_seed(&env, &id_a),
                build_internal_seed(&env, &id_b),
            )
        });

        assert_ne!(
            seed_a, seed_b,
            "different raffle IDs must produce different seeds"
        );
    }

    /// build_internal_seed is deterministic: same inputs → same output.
    #[test]
    fn build_internal_seed_is_deterministic() {
        let env = Env::default();
        let raffle_id = Address::generate(&env);
        let contract = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();

        let (first, second) = env.as_contract(&contract, || {
            (
                build_internal_seed(&env, &raffle_id),
                build_internal_seed(&env, &raffle_id),
            )
        });

        assert_eq!(first, second, "same inputs must always yield the same seed");
    }

    /// build_internal_seed output is exactly 32 bytes.
    #[test]
    fn build_internal_seed_is_32_bytes() {
        let env = Env::default();
        let raffle_id = Address::generate(&env);
        let contract = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();

        let seed = env.as_contract(&contract, || build_internal_seed(&env, &raffle_id));
        // BytesN<32> is always 32 bytes by construction; this is a compile-time
        // guarantee, but we also verify the array conversion is loss-free.
        assert_eq!(seed.to_array().len(), 32);
    }

    /// build_internal_seed must not produce the all-zero hash.
    #[test]
    fn build_internal_seed_is_not_zero() {
        let env = Env::default();
        let raffle_id = Address::generate(&env);
        let contract = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();

        let seed = env.as_contract(&contract, || build_internal_seed(&env, &raffle_id));
        assert_ne!(
            seed.to_array(),
            [0u8; 32],
            "sha256 output must not be all zero"
        );
    }

    /// PRNG selections fall within [0, total_tickets).
    #[test]
    fn prng_selection_is_in_ticket_range() {
        let env = Env::default();
        let raffle_id = Address::generate(&env);
        let strategy = PrngWinnerSelection::new(1_700_000_000, 99_001, raffle_id, 17);

        let contract_id = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        let indices = env.as_contract(&contract_id, || {
            strategy.select_winner_indices(&env, 17, 25)
        });
        assert_eq!(indices.len(), 25);
        for idx in indices.iter() {
            assert!(idx < 17, "winner index {idx} must be < total_tickets 17");
        }
    }

    /// Same PRNG inputs always produce the same winner sequence.
    #[test]
    fn prng_selection_is_deterministic_for_same_inputs() {
        let env = Env::default();
        let raffle_id = Address::generate(&env);

        let contract_id = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        let first = env.as_contract(&contract_id, || {
            PrngWinnerSelection::new(1_700_000_000, 99_001, raffle_id.clone(), 17)
                .select_winner_indices(&env, 17, 8)
        });
        let second = env.as_contract(&contract_id, || {
            PrngWinnerSelection::new(1_700_000_000, 99_001, raffle_id, 17)
                .select_winner_indices(&env, 17, 8)
        });

        assert_eq!(
            first, second,
            "identical inputs must yield identical winners"
        );
    }

    /// Seed fingerprint changes when raffle_id changes.
    #[test]
    fn seed_fingerprint_differs_by_raffle_id() {
        let env = Env::default();
        let id_a = Address::generate(&env);
        let id_b = Address::generate(&env);
        let contract = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();

        let (fp_a, fp_b) = env.as_contract(&contract, || {
            let s_a = PrngWinnerSelection::new(0, 0, id_a, 10);
            let s_b = PrngWinnerSelection::new(0, 0, id_b, 10);
            (s_a.seed_fingerprint(&env), s_b.seed_fingerprint(&env))
        });

        assert_ne!(
            fp_a, fp_b,
            "fingerprints must differ for different raffle IDs"
        );
    }
}
