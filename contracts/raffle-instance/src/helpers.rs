use soroban_sdk::{auth::InvokerContractAuthEntry, Address, BytesN, Env, IntoVal, Symbol, Val, Vec, token};

use crate::events::{RaffleFinalized, RaffleStatusChanged, WinnerDrawn};
use crate::randomness::{OracleSeedWinnerSelection, WinnerSelectionStrategy};
use crate::{DataKey, Error, FairnessMetadata, Raffle, RaffleStatus, RandomnessType, Ticket};

pub(crate) fn read_raffle(env: &Env) -> Result<Raffle, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Raffle)
        .ok_or(Error::NotInitialized)
}

pub(crate) fn write_raffle(env: &Env, raffle: &Raffle) {
    env.storage().instance().set(&DataKey::Raffle, raffle);
}

/// Checked lifecycle transition. All status writes must go through this helper
/// (or [`revert_status`] for internal draw rollbacks).
pub(crate) fn transition_status(
    env: &Env,
    raffle: &mut Raffle,
    new_status: RaffleStatus,
    timestamp: u64,
) -> Result<(), Error> {
    if !raffle.status.can_transition_to(new_status) {
        return Err(Error::InvalidStateTransition);
    }
    let old_status = raffle.status.clone();
    raffle.status = new_status.clone();
    write_raffle(env, raffle);
    RaffleStatusChanged {
        old_status,
        new_status,
        timestamp,
    }
    .publish(env);
    Ok(())
}

/// Internal rollback when a draw step fails after transitioning to `Drawing`.
pub(crate) fn revert_status(env: &Env, raffle: &mut Raffle, target: RaffleStatus) -> Result<(), Error> {
    if !raffle.status.can_internal_revert_to(target) {
        return Err(Error::InvalidStateTransition);
    }
    raffle.status = target;
    write_raffle(env, raffle);
    Ok(())
}

pub(crate) fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotAuthorized)?;
    admin.require_auth();
    Ok(admin)
}

pub(crate) fn get_ticket_owner(env: &Env, ticket_id: u32) -> Option<Address> {
    env.storage()
        .persistent()
        .get::<_, Ticket>(&DataKey::Ticket(ticket_id))
        .map(|t| t.owner)
}

pub(crate) fn acquire_guard(env: &Env) -> Result<(), Error> {
    if env.storage().instance().has(&DataKey::ReentrancyGuard) {
        return Err(Error::Reentrancy);
    }
    env.storage()
        .instance()
        .set(&DataKey::ReentrancyGuard, &true);
    Ok(())
}

pub(crate) fn release_guard(env: &Env) {
    env.storage().instance().remove(&DataKey::ReentrancyGuard);
}

pub(crate) struct Guard<'a> {
    env: &'a Env,
}

impl<'a> Guard<'a> {
    pub(crate) fn new(env: &'a Env) -> Result<Self, Error> {
        acquire_guard(env)?;
        Ok(Guard { env })
    }
}

impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        release_guard(self.env);
    }
}

#[allow(dead_code)]
pub(crate) fn enforce_swap_guard(
    env: &Env,
    raffle: &Raffle,
    amount_out: i128,
    min_amount_out: i128,
) -> Result<(), Error> {
    let deadline = env.ledger().timestamp() + raffle.swap_deadline_seconds;
    if env.ledger().timestamp() > deadline {
        return Err(Error::DeadlinePassed);
    }
    if amount_out < min_amount_out {
        return Err(Error::SlippageExceeded);
    }
    Ok(())
}

pub(crate) fn request_randomness(env: &Env) -> Result<u64, Error> {
    let already: bool = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequested)
        .unwrap_or(false);
    if already {
        return Err(Error::RandomnessAlreadyRequested);
    }

    use soroban_sdk::xdr::ToXdr;
    let request_id_xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.current_contract_address().to_xdr(env),
    )
        .to_xdr(env);
    let request_id_hash: BytesN<32> = env.crypto().sha256(&request_id_xdr).into();
    let arr = request_id_hash.to_array();
    let mut id_bytes = [0u8; 8];
    id_bytes.copy_from_slice(&arr[..8]);
    let request_id = u64::from_be_bytes(id_bytes);

    env.storage()
        .instance()
        .set(&DataKey::RandomnessRequested, &true);
    env.storage()
        .instance()
        .set(&DataKey::RandomnessRequestLedger, &env.ledger().sequence());
    env.storage()
        .instance()
        .set(&DataKey::RandomnessRequestId, &request_id);
    Ok(request_id)
}

pub(crate) fn transition_to_drawing(
    env: &Env,
    raffle: &mut Raffle,
    timestamp: u64,
) -> Result<(), Error> {
    let drawing_lock: bool = env
        .storage()
        .instance()
        .get(&DataKey::DrawingLock)
        .unwrap_or(false);
    if drawing_lock {
        return Err(Error::DrawingAlreadyInProgress);
    }

    if raffle.status == RaffleStatus::Drawing {
        return Err(Error::DrawingAlreadyInProgress);
    }

    transition_status(env, raffle, RaffleStatus::Drawing, timestamp)?;
    env.storage().instance().set(&DataKey::DrawingLock, &true);
    Ok(())
}

pub(crate) fn require_not_paused(env: &Env) -> Result<(), Error> {
    if env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(Error::ContractPaused);
    }
    Ok(())
}

pub(crate) fn require_global_not_paused(env: &Env) -> Result<(), Error> {
    let factory: Address = env
        .storage()
        .instance()
        .get(&DataKey::Factory)
        .ok_or(Error::NotInitialized)?;
    let paused: bool = env.invoke_contract(
        &factory,
        &Symbol::new(env, "is_global_paused"),
        ().into_val(env),
    );
    if paused {
        return Err(Error::ContractPaused);
    }
    Ok(())
}

pub(crate) fn validate_token_address(env: &Env, token_address: &Address) -> Result<(), Error> {
    let token_client = token::Client::new(env, token_address);
    let _ = token_client
        .try_decimals()
        .map_err(|_| Error::InvalidTokenAddress)?;
    Ok(())
}

pub(crate) fn build_internal_seed_u64(env: &Env) -> u64 {
    use soroban_sdk::xdr::ToXdr;
    let xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.current_contract_address(),
    )
        .to_xdr(env);
    let hash: BytesN<32> = env.crypto().sha256(&xdr).into();
    let arr = hash.to_array();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&arr[..8]);
    u64::from_be_bytes(bytes)
}

pub(crate) fn calculate_tier_prize(raffle: &Raffle, tier_index: u32) -> Result<i128, Error> {
    let last_tier_index = raffle.prizes.len() - 1;
    if tier_index == last_tier_index {
        let mut allocated = 0i128;
        for i in 0..last_tier_index {
            let bp = raffle.prizes.get(i).ok_or(Error::InvalidIndex)?;
            let amt = raffle
                .prize_amount
                .checked_mul(bp as i128)
                .ok_or(Error::ArithmeticOverflow)?
                / 10000;
            allocated = allocated
                .checked_add(amt)
                .ok_or(Error::ArithmeticOverflow)?;
        }
        return raffle
            .prize_amount
            .checked_sub(allocated)
            .ok_or(Error::ArithmeticOverflow);
    }
    let bp = raffle.prizes.get(tier_index).ok_or(Error::InvalidIndex)?;
    raffle
        .prize_amount
        .checked_mul(bp as i128)
        .ok_or(Error::ArithmeticOverflow)
        .map(|a| a / 10000)
}


/// Finalize the raffle using a pre-computed `u64` seed.
///
/// This is the common finalization path shared by all three randomness modes
/// (`Internal`, `External`/VRF, and `Fallback`).  The caller selects the
/// appropriate seed and [`RandomnessType`] label before calling this function.
///
/// ## What this function does
///
/// 1. Validates `tickets_sold > 0` and `prizes.len() ≤ tickets_sold`.
/// 2. Uses [`OracleSeedWinnerSelection`] to pick `prizes.len()` distinct
///    winning ticket indices using rejection sampling (no modulo bias).
/// 3. Resolves each winning index to a ticket owner via
///    [`get_ticket_owner`] and emits a [`events::WinnerDrawn`] event per
///    winner.
/// 4. Writes [`FairnessMetadata`] to **persistent** storage under
///    [`DataKey::RandomnessSeed`] so it survives ledger-entry expiry and can
///    be queried by [`get_fairness_data`](crate::views::get_fairness_data).
/// 5. Sets `raffle.status = Finalized`, records `winners`,
///    `claimed_winners`, and `finalized_at`.
/// 6. Clears `RandomnessRequested`, `RandomnessRequestId`,
///    `RandomnessRequestLedger`, and sets `DrawingLock = false`.
/// 7. Emits [`events::RaffleFinalized`].
///
/// # Parameters
///
/// - `seed` — The 64-bit random seed to use for winner selection.
/// - `randomness_type` — Label for the audit trail (PRNG, VRF, or Fallback).
///
/// # Errors
///
/// - [`Error::NoTicketsSold`] — `tickets_sold == 0`.
/// - [`Error::MorePrizesThanTickets`] — more prize tiers than tickets sold.
/// - [`Error::NoActiveTickets`] — zero tickets found (should not happen in
///   practice if the above checks pass).
/// - [`Error::InvalidIndex`] — winner index out of range.
/// - [`Error::TicketNotFound`] — ticket record missing for a winning index.
/// - [`Error::ArithmeticOverflow`] — overflow in prize calculation.
///
/// # Events
///
/// - [`events::WinnerDrawn`] — emitted once per winner.
/// - [`events::RaffleFinalized`] — emitted after all winners are resolved.
///
/// See also: [`docs/EVENTS.md`](../../../../docs/EVENTS.md) — `WinnerDrawn`,
/// `RaffleFinalized`.
pub(crate) fn do_finalize_with_seed(
    env: &Env,
    mut raffle: Raffle,
    seed: u64,
    randomness_type: RandomnessType,
) -> Result<(), Error> {
    let total_tickets = raffle.tickets_sold;
    if total_tickets == 0 {
        return Err(Error::NoTicketsSold);
    }
    if raffle.prizes.len() > total_tickets {
        return Err(Error::MorePrizesThanTickets);
    }
    if raffle.tickets_sold == 0 {
        return Err(Error::NoActiveTickets);
    }

    let selector = OracleSeedWinnerSelection::new(seed);
    let winning_ticket_ids =
        selector.select_winner_indices(env, total_tickets, raffle.prizes.len());
    let mut winners = Vec::new(env);

    for i in 0..winning_ticket_ids.len() {
        let mut idx = winning_ticket_ids.get(i).ok_or(Error::InvalidIndex)?;
        if raffle.unique_winners {
            idx = resolve_unique_winner(env, seed, i as u32, total_tickets, &winners, idx);
            winning_ticket_ids.set(i, idx);
        }
        let winner = get_ticket_owner(env, idx + 1).ok_or(Error::TicketNotFound)?;
        winners.push_back(winner.clone());
        WinnerDrawn {
            winner,
            ticket_id: idx,
            tier_index: i,
            timestamp: env.ledger().timestamp(),
        }
        .publish(env);
    }

    let mut quorum_contributions: Option<Vec<(Address, u64)>> = None;
    if let Some(submitted) = env.storage().persistent().get::<_, Vec<Address>>(&DataKey::QuorumSubmittedOracles) {
        let mut contributions = Vec::new(env);
        for i in 0..submitted.len() {
            if let Some(addr) = submitted.get(i) {
                if let Some(s) = env.storage().persistent().get::<_, u64>(&DataKey::QuorumSeed(addr.clone())) {
                    contributions.push_back((addr.clone(), s));
                }
                env.storage().persistent().remove(&DataKey::QuorumSeed(addr));
            }
        }
        env.storage().persistent().remove(&DataKey::QuorumSubmittedOracles);
        
        // Only record the contributions if this was an actual Quorum draw,
        // not a fallback that bypassed quorum completion.
        if randomness_type == RandomnessType::Vrf {
            quorum_contributions = Some(contributions);
        }
    }

    env.storage().persistent().set(
        &DataKey::RandomnessSeed,
        &FairnessMetadata {
            seed,
            randomness_source: raffle.randomness_source.clone(),
            winning_ticket_indices: winning_ticket_ids.clone(),
            draw_timestamp: env.ledger().timestamp(),
            draw_sequence: env.ledger().sequence(),
            unique_winners: raffle.unique_winners,
            quorum_contributions,
        },
    );

    raffle.winners = winners.clone();
    raffle.finalized_at = Some(env.ledger().timestamp());
    transition_status(
        env,
        &mut raffle,
        RaffleStatus::Finalized,
        env.ledger().timestamp(),
    )?;
    raffle.finalized_at = Some(env.ledger().timestamp());
    write_raffle(env, &raffle);

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

    RaffleFinalized {
        raffle_id: env.current_contract_address(),
        winners,
        winning_ticket_ids,
        total_tickets_sold: raffle.tickets_sold,
        randomness_source: raffle.randomness_source.clone(),
        randomness_type,
        finalized_at: env.ledger().timestamp(),
    }
    .publish(env);

    bump_raffle_ttl(env, total_tickets);
    record_leaderboard(env, &raffle);
    Ok(())
}

fn record_leaderboard(env: &Env, raffle: &Raffle) {
    let factory: Address = match env.storage().instance().get(&DataKey::Factory) {
        Some(f) => f,
        None => return,
    };
    let raffle_id = env.current_contract_address();
    let tickets = raffle.tickets_sold as i128;
    
    let max_tickets = raffle.max_tickets as i128;
    let early_bird_limit = max_tickets * (raffle.early_bird_ticket_percentage as i128) / 100;
    let discount_price = raffle.ticket_price - (raffle.ticket_price * (raffle.early_bird_discount_bp as i128) / 10000);
    
    let volume = if tickets <= early_bird_limit {
        discount_price.saturating_mul(tickets)
    } else {
        let early_volume = discount_price.saturating_mul(early_bird_limit);
        let regular_tickets = tickets - early_bird_limit;
        let regular_volume = raffle.ticket_price.saturating_mul(regular_tickets);
        early_volume.saturating_add(regular_volume)
    };
    let args: Vec<Val> = (
        raffle_id.clone(),
        tickets,
        raffle.prize_amount,
        volume,
    )
        .into_val(env);

    use soroban_sdk::auth::{ContractContext, SubContractInvocation};
    env.authorize_as_current_contract(soroban_sdk::vec![
        env,
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: factory.clone(),
                fn_name: Symbol::new(env, "record_leaderboard_entry"),
                args: args.clone(),
            },
            sub_invocations: Vec::new(env),
        }),
    ]);
    let _ = env.invoke_contract::<()>(
        &factory,
        &Symbol::new(env, "record_leaderboard_entry"),
        args,
    );
}
