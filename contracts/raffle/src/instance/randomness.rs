use soroban_sdk::{xdr::ToXdr, Address, Env, Vec};

/// Common winner-selection interface used by both PRNG and oracle paths.
pub trait WinnerSelectionStrategy {
    fn select_winner_indices(&self, env: &Env, total_tickets: u32, winner_count: u32) -> Vec<u32>;
}

/// Internal PRNG selection strategy.
///
/// Seed material includes timestamp, sequence, raffle_id, and tickets_sold to
/// avoid relying on a single ledger field.
pub struct PrngWinnerSelection {
    timestamp: u64,
    sequence: u32,
    raffle_id: Address,
    tickets_sold: u32,
}

impl PrngWinnerSelection {
    pub fn new(timestamp: u64, sequence: u32, raffle_id: Address, tickets_sold: u32) -> Self {
        Self {
            timestamp,
            sequence,
            raffle_id,
            tickets_sold,
        }
    }

    pub fn seed_fingerprint(&self, env: &Env) -> u64 {
        let mut fingerprint = self
            .timestamp
            .wrapping_add((self.sequence as u64) << 32)
            .wrapping_add(self.tickets_sold as u64);

        for byte in self.raffle_id.clone().to_xdr(env).iter() {
            fingerprint = fingerprint.wrapping_mul(16777619).wrapping_add(byte as u64);
        }

        fingerprint
    }

    fn seed_bytes(&self, env: &Env) -> soroban_sdk::Bytes {
        let xdr = (
            self.timestamp,
            self.sequence,
            self.raffle_id.clone(),
            self.tickets_sold,
        )
            .to_xdr(env);
        env.crypto().sha256(&xdr).into()
    }
}

impl WinnerSelectionStrategy for PrngWinnerSelection {
    fn select_winner_indices(&self, env: &Env, total_tickets: u32, winner_count: u32) -> Vec<u32> {
        let mut indices = Vec::new(env);
        if total_tickets == 0 || winner_count == 0 {
            return indices;
        }

        env.prng().seed(self.seed_bytes(env));

        for _ in 0..winner_count {
            let idx = env.prng().u64_in_range(0..(total_tickets as u64)) as u32;
            indices.push_back(idx);
        }

        indices
    }
}

/// Oracle-backed strategy using externally provided seed.
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

        let mut current_seed = self.seed;
        for _ in 0..winner_count {
            let idx = (current_seed % (total_tickets as u64)) as u32;
            indices.push_back(idx);
            current_seed = current_seed.wrapping_add(1);
        }

        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn prng_selection_is_in_ticket_range() {
        let env = Env::default();
        let raffle_id = Address::generate(&env);
        let strategy = PrngWinnerSelection::new(1_700_000_000, 99_001, raffle_id, 17);

        let contract_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
        let indices = env.as_contract(&contract_id, || {
            strategy.select_winner_indices(&env, 17, 25)
        });
        assert_eq!(indices.len(), 25);
        for idx in indices.iter() {
            assert!(idx < 17);
        }
    }

    #[test]
    fn prng_selection_is_deterministic_for_same_inputs() {
        let env = Env::default();
        let raffle_id = Address::generate(&env);

        let contract_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address(); // just to have a contract
        let first = env.as_contract(&contract_id, || {
            PrngWinnerSelection::new(1_700_000_000, 99_001, raffle_id.clone(), 17)
                .select_winner_indices(&env, 17, 8)
        });
        let second = env.as_contract(&contract_id, || {
            PrngWinnerSelection::new(1_700_000_000, 99_001, raffle_id, 17)
                .select_winner_indices(&env, 17, 8)
        });

        assert_eq!(first, second);
    }
}
