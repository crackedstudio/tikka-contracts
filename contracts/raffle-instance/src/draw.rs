use soroban_sdk::{xdr::ToXdr, Address, Bytes, BytesN, Env, Vec};

use raffle_shared::{
    CancelReason, FailureReason, QuorumConfig, RandomnessSource, RandomnessType,
};

use crate::events::{
    DrawTriggered, RaffleCancelled, RaffleFailed, RandomnessFallbackTriggered, RandomnessReceived,
    RandomnessRequested,
};
use crate::helpers::{
    build_internal_seed_u64, do_finalize_with_seed, read_raffle, request_randomness,
    revert_status, transition_status, transition_to_drawing, write_raffle,
};
use crate::randomness::build_vrf_proof_message;
use crate::{
    CommitRevealEntry, DataKey, Error, RaffleStatus, ORACLE_TIMEOUT_LEDGERS,
    RANDOMNESS_MIN_DELAY_LEDGERS,
};

pub(crate) fn finalize_raffle(env: Env) -> Result<(), Error> {
    let drawing_lock: bool = env
        .storage()
        .instance()
        .get(&DataKey::DrawingLock)
        .unwrap_or(false);
    if drawing_lock {
        return Err(Error::DrawingAlreadyInProgress);
    }
    let mut raffle = read_raffle(&env)?;
    raffle.creator.require_auth();

    if raffle.status != RaffleStatus::Active && raffle.status != RaffleStatus::Drawing {
        return Err(Error::InvalidStatus);
    }

    let now = env.ledger().timestamp();
    let time_ended = !raffle.no_deadline && now >= raffle.end_time;
    let tickets_full = raffle.tickets_sold >= raffle.max_tickets;

    if raffle.status == RaffleStatus::Active && !time_ended && !tickets_full {
        return Err(Error::InvalidStateTransition);
    }

    if raffle.tickets_sold == 0 || raffle.tickets_sold < raffle.min_tickets {
        let failure_reason = if raffle.tickets_sold == 0 {
            FailureReason::ZeroTicketsSold
        } else {
            FailureReason::MinTicketsNotMet
        };
        transition_status(&env, &mut raffle, RaffleStatus::Failed, now)?;
        RaffleFailed {
            creator: raffle.creator.clone(),
            reason: failure_reason,
            tickets_sold: raffle.tickets_sold,
            timestamp: now,
        }
        .publish(&env);
        return Ok(());
    }

    let caller = raffle.creator.clone();
    let pre_status = raffle.status.clone();
    transition_to_drawing(&env, &mut raffle, now)?;

    // === Quorum fan-out ===
    if let RandomnessSource::Quorum(QuorumConfig { oracles, .. }) = &raffle.randomness_source {
        match request_randomness(&env) {
            Ok(request_id) => {
                DrawTriggered {
                    caller: caller.clone(),
                    total_tickets_sold: raffle.tickets_sold,
                    timestamp: now,
                }
                .publish(&env);

                // Emit RandomnessRequested for each oracle (fan-out)
                for i in 0..oracles.len() {
                    if let Some(addr) = oracles.get(i) {
                        RandomnessRequested {
                            oracle: addr,
                            request_id,
                            timestamp: now,
                        }
                        .publish(&env);
                    }
                }

                // Initialise empty quorum submission tracker
                env.storage()
                    .instance()
                    .set(&DataKey::QuorumSubmittedOracles, &Vec::<Address>::new(&env));

                return Ok(());
            }
            Err(err) => {
                revert_status(&env, &mut raffle, pre_status)?;
                env.storage()
                    .instance()
                    .set(&DataKey::DrawingLock, &false);
                return Err(err);
            }
        }
    }

    // === External (single oracle) ===
    if raffle.randomness_source == RandomnessSource::External {
        match request_randomness(&env) {
            Ok(request_id) => {
                DrawTriggered {
                    caller: caller.clone(),
                    total_tickets_sold: raffle.tickets_sold,
                    timestamp: now,
                }
                .publish(&env);
                RandomnessRequested {
                    oracle: raffle
                        .oracle_address
                        .clone()
                        .unwrap_or(env.current_contract_address()),
                    request_id,
                    timestamp: now,
                }
                .publish(&env);
                return Ok(());
            }
            Err(err) => {
                revert_status(&env, &mut raffle, pre_status)?;
                env.storage()
                    .instance()
                    .set(&DataKey::DrawingLock, &false);
                return Err(err);
            }
        }
    }

    DrawTriggered {
        caller: caller.clone(),
        total_tickets_sold: raffle.tickets_sold,
        timestamp: now,
    }
    .publish(&env);

    if raffle.randomness_source == RandomnessSource::CommitReveal {
        let mut combined = Bytes::new(&env);
        let mut commits_found: u32 = 0;
        for ticket_id in 1..=raffle.tickets_sold {
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<_, CommitRevealEntry>(&DataKey::CommitEntry(ticket_id))
            {
                combined.extend_from_array(&entry.hash.to_array());
                commits_found += 1;
            }
        }
        if commits_found > 0 {
            let hash: BytesN<32> = env.crypto().sha256(&combined).into();
            let arr = hash.to_array();
            let mut seed_bytes = [0u8; 8];
            seed_bytes.copy_from_slice(&arr[..8]);
            let seed = u64::from_be_bytes(seed_bytes);
            return do_finalize_with_seed(&env, raffle, seed, RandomnessType::Prng);
        }
    }

    let seed = build_internal_seed_u64(&env);
    do_finalize_with_seed(&env, raffle, seed, RandomnessType::Prng)
}

/// Handle a single-oracle VRF randomness submission (existing External mode).
pub(crate) fn provide_randomness(
    env: Env,
    random_seed: u64,
    public_key: BytesN<32>,
    proof: BytesN<64>,
    request_id: u64,
) -> Result<Address, Error> {
    let drawing_lock: bool = env
        .storage()
        .instance()
        .get(&DataKey::DrawingLock)
        .unwrap_or(false);
    if !drawing_lock {
        return Err(Error::DrawingAlreadyComplete);
    }

    let raffle = read_raffle(&env)?;

    // Reject Quorum-mode submissions on this path
    if matches!(raffle.randomness_source, RandomnessSource::Quorum(_)) {
        return Err(Error::InvalidParameters);
    }

    let oracle = match &raffle.oracle_address {
        Some(addr) => {
            addr.require_auth();
            addr.clone()
        }
        None => return Err(Error::OracleNotSet),
    };

    if raffle.status != RaffleStatus::Drawing {
        return Err(Error::InvalidStateTransition);
    }
    let pending: bool = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequested)
        .unwrap_or(false);
    if !pending {
        return Err(Error::NoRandomnessRequest);
    }

    let req_ledger: u32 = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequestLedger)
        .unwrap_or(0);
    if env.ledger().sequence() < req_ledger + RANDOMNESS_MIN_DELAY_LEDGERS {
        return Err(Error::RandomnessTooEarly);
    }

    let stored: u64 = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequestId)
        .ok_or(Error::NoRandomnessRequest)?;
    if stored != request_id {
        return Err(Error::InvalidParameters);
    }

    let message = build_vrf_proof_message(&env, request_id, random_seed);
    env.crypto().ed25519_verify(&public_key, &message, &proof);

    RandomnessReceived {
        oracle,
        seed: random_seed,
        request_id,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    do_finalize_with_seed(&env, raffle, random_seed, RandomnessType::Vrf)?;
    Ok(env.current_contract_address())
}

pub(crate) fn trigger_randomness_fallback(
    env: Env,
    caller: Address,
    do_refund: bool,
) -> Result<(), Error> {
    let drawing_lock: bool = env
        .storage()
        .instance()
        .get(&DataKey::DrawingLock)
        .unwrap_or(false);
    if drawing_lock {
        return Err(Error::DrawingAlreadyInProgress);
    }

    caller.require_auth();
    let mut raffle = read_raffle(&env)?;

    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotAuthorized)?;
    if caller != raffle.creator && caller != admin {
        return Err(Error::NotAuthorized);
    }
    if raffle.status != RaffleStatus::Drawing {
        return Err(Error::InvalidStateTransition);
    }

    let pending: bool = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequested)
        .unwrap_or(false);
    if !pending {
        return Err(Error::NoRandomnessRequest);
    }

    let req_ledger: u32 = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequestLedger)
        .unwrap_or(0);
    if env.ledger().sequence() < req_ledger + ORACLE_TIMEOUT_LEDGERS {
        return Err(Error::FallbackTooEarly);
    }

    if do_refund {
        transition_status(
            &env,
            &mut raffle,
            RaffleStatus::Cancelled,
            env.ledger().timestamp(),
        )?;
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequested);
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequestId);
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequestLedger);
        env.storage().instance().set(&DataKey::DrawingLock, &false);
        RaffleCancelled {
            creator: raffle.creator.clone(),
            reason: CancelReason::OracleTimeout,
            tickets_sold: raffle.tickets_sold,
            prize_refunded: raffle.prize_deposited,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);
        return Ok(());
    }

    let seed = build_internal_seed_u64(&env);
    RandomnessFallbackTriggered {
        triggered_by: caller,
        seed_used: seed,
        request_ledger: req_ledger,
        fallback_ledger: env.ledger().sequence(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);

    do_finalize_with_seed(&env, raffle, seed, RandomnessType::Fallback)
}

