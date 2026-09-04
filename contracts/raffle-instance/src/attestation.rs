//! Draw attestation for third-party verification.
//!
//! This module provides a single-call interface for external auditors to verify
//! that a raffle draw was performed correctly. Rather than requiring multiple
//! queries (raffle config, winners, fairness data, metadata), auditors can call
//! [`get_draw_attestation`] and receive everything needed to independently
//! reproduce the winner selection in one response.

use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

use raffle_shared::{FairnessData, RandomnessSource};

use crate::{read_raffle, DataKey, Error, FairnessMetadata, RaffleStatus};

/// Complete attestation package for independent draw verification.
///
/// Contains all data required to verify a raffle draw without additional
/// contract queries or trust in off-chain indexers. A verifier can:
///
/// 1. Hash the effective config to confirm it matches `config_hash`.
/// 2. Use `fairness_data.seed` and `fairness_data.ticket_ids` to reproduce
///    the winner selection algorithm.
/// 3. Compare the reproduced winners with `winner_addresses` and
///    `winning_ticket_ids`.
/// 4. Verify the `metadata_hash` matches the advertised off-chain content.
///
/// This structure is only available after a raffle has been finalized.
#[derive(Clone)]
#[contracttype]
pub struct DrawAttestation {
    /// Complete fairness audit data (seed, source, ticket IDs, winning indices).
    pub fairness_data: FairnessData,
    /// SHA-256 hash of the raffle's off-chain metadata.
    pub metadata_hash: BytesN<32>,
    /// Resolved winner addresses in prize-tier order.
    pub winner_addresses: Vec<Address>,
    /// Winning ticket IDs (1-indexed) in prize-tier order.
    pub winning_ticket_ids: Vec<u32>,
    /// Randomness source used for this draw.
    pub randomness_source: RandomnessSource,
    /// SHA-256 hash of the effective raffle configuration at draw time.
    pub config_hash: BytesN<32>,
    /// Total number of tickets sold.
    pub total_tickets_sold: u32,
    /// Prize distribution in basis points.
    pub prize_distribution_bp: Vec<u32>,
    /// Total prize amount.
    pub prize_amount: i128,
    /// Ticket price.
    pub ticket_price: i128,
}

/// Return a complete attestation package for third-party draw verification.
///
/// This function combines:
/// - [`FairnessData`] from persistent storage (seed, ticket IDs, winning indices).
/// - Winner addresses and ticket IDs resolved from the raffle state.
/// - Metadata hash from the original raffle configuration.
/// - A hash of the effective configuration (max_tickets, ticket_price, etc.)
///   so verifiers can confirm the draw parameters.
///
/// # Availability
///
/// Only callable when the raffle is in [`RaffleStatus::Finalized`] or
/// [`RaffleStatus::Claimed`]. Returns [`Error::InvalidStatus`] otherwise.
///
/// # Errors
///
/// - [`Error::NotInitialized`] — raffle not initialized.
/// - [`Error::InvalidStatus`] — raffle has not been finalized yet.
///
/// # Usage
///
/// ```rust,ignore
/// let attestation = contract.get_draw_attestation(&env)?;
/// // Verifier independently reproduces winner selection using attestation.fairness_data
/// // and compares results with attestation.winner_addresses
/// ```
///
/// See also: [`docs/RANDOMNESS.md`](../../../../docs/RANDOMNESS.md) — verification
/// procedure.
pub(crate) fn get_draw_attestation(env: &Env) -> Result<DrawAttestation, Error> {
    let raffle = read_raffle(env)?;

    // Only allow attestation retrieval after finalization
    if raffle.status != RaffleStatus::Finalized && raffle.status != RaffleStatus::Claimed {
        return Err(Error::InvalidStatus);
    }

    // Load fairness metadata from persistent storage
    let fairness_meta: FairnessMetadata = env
        .storage()
        .persistent()
        .get(&DataKey::RandomnessSeed)
        .ok_or(Error::InvalidStatus)?;

    // Reconstruct full FairnessData
    let mut ticket_ids = Vec::new(env);
    for i in 1..=raffle.tickets_sold {
        ticket_ids.push_back(i);
    }

    let fairness_data = FairnessData {
        seed: fairness_meta.seed,
        randomness_source: fairness_meta.randomness_source.clone(),
        ticket_ids,
        winning_ticket_indices: fairness_meta.winning_ticket_indices.clone(),
        draw_timestamp: fairness_meta.draw_timestamp,
        draw_sequence: fairness_meta.draw_sequence,
        unique_winners: fairness_meta.unique_winners,
    };

    // Resolve winning ticket IDs from indices
    let mut winning_ticket_ids = Vec::new(env);
    for i in 0..fairness_meta.winning_ticket_indices.len() {
        if let Some(idx) = fairness_meta.winning_ticket_indices.get(i) {
            winning_ticket_ids.push_back(idx + 1); // Convert 0-based index to 1-based ticket ID
        }
    }

    // Compute configuration hash for verification
    let config_hash = compute_config_hash(env, &raffle);

    // Retrieve metadata hash from the raffle configuration
    let metadata_hash = raffle.metadata_hash.clone();

    let mut winner_addresses = Vec::new(env);
    for winner in raffle.winners.iter() {
        winner_addresses.push_back(winner.address);
    }

    Ok(DrawAttestation {
        fairness_data,
        metadata_hash,
        winner_addresses,
        winning_ticket_ids,
        randomness_source: raffle.randomness_source,
        config_hash,
        total_tickets_sold: raffle.tickets_sold,
        prize_distribution_bp: raffle.prizes.clone(),
        prize_amount: raffle.prize_amount,
        ticket_price: raffle.ticket_price,
    })
}

/// Compute a deterministic hash of the raffle's effective configuration.
///
/// This hash allows verifiers to confirm that the configuration used for the
/// draw matches what they expect. The hash includes all parameters that affect
/// winner selection or prize distribution.
fn compute_config_hash(env: &Env, raffle: &crate::Raffle) -> BytesN<32> {
    use soroban_sdk::xdr::ToXdr;

    // Pack configuration fields that affect draw outcome
    let config_xdr = (
        raffle.max_tickets,
        raffle.ticket_price,
        raffle.prize_amount,
        raffle.prizes.clone().to_xdr(env),
        raffle.randomness_source.clone().to_xdr(env),
        raffle.payment_token.clone().to_xdr(env),
        raffle.creator.clone().to_xdr(env),
    )
        .to_xdr(env);

    env.crypto().sha256(&config_xdr).into()
}

