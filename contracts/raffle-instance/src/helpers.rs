use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    token,
    xdr::ToXdr,
    Address, BytesN, Env, IntoVal, Symbol, Val, Vec,
};

use crate::events::{RaffleFinalized, RaffleStatusChanged, WinnerDrawn};
use crate::randomness::OracleSeedWinnerSelection;
use crate::{DataKey, Error, FairnessMetadata, Raffle, RaffleStatus, RandomnessType, Ticket};
use raffle_shared::BuyQuote;

pub(crate) fn read_raffle(env: &Env) -> Result<Raffle, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Raffle)
        .ok_or(Error::NotInitialized)
}

pub(crate) fn write_raffle(env: &Env, raffle: &Raffle) {
    env.storage().instance().set(&DataKey::Raffle, raffle);
}

pub(crate) fn calculate_buy_quote(raffle: &Raffle, quantity: u32) -> Result<BuyQuote, Error> {
    if quantity == 0 {
        return Err(Error::InvalidQuantity);
    }
    let gross = raffle
        .ticket_price
        .checked_mul(quantity as i128)
        .ok_or(Error::ArithmeticOverflow)?;
    let discount = if raffle.early_bird_ticket_percentage > 0
        && raffle.tickets_sold < raffle.max_tickets * raffle.early_bird_ticket_percentage / 100
    {
        raffle_shared::apply_bp(gross, raffle.early_bird_discount_bp)
            .ok_or(Error::ArithmeticOverflow)?
    } else {
        0
    };
    let net = gross.checked_sub(discount).ok_or(Error::ArithmeticOverflow)?;
    let fee = raffle_shared::apply_bp(net, raffle.protocol_fee_bp)
        .ok_or(Error::ArithmeticOverflow)?;
    let net_to_pay = net.checked_add(fee).ok_or(Error::ArithmeticOverflow)?;
    let effective_ticket_price = net / quantity as i128;
    Ok(BuyQuote { gross, discount, fee, net_to_pay, effective_ticket_price })
}

/// Re-draw a winning ticket index so the tier is won by an address that has
/// not already won an earlier tier.
///
/// # Why not linear probing
///
/// Walking forward from the originally drawn index (`candidate + 1`,
/// `candidate + 2`, …) until an owner that has not won turns up made every
/// collision land on the ticket immediately after the colliding one. With a
/// small participant set and several tiers the holder just past a repeat
/// winner was therefore systematically over-represented, while ticket 0 was
/// the least likely of all (#991).
///
/// Instead this rejection-samples a fresh index from an LCG stream
/// domain-separated by `seed` and `tier_index`, so the accepted ticket is
/// uniform over the tickets whose owner has not yet won. `seed` and
/// `tier_index` are both load-bearing: no argument is accepted and ignored.
///
/// # Fallback
///
/// At most `OracleSeedWinnerSelection::MAX_REDRAW_ATTEMPTS` fresh samples are
/// taken. If they are exhausted — which requires that no acceptable ticket
/// exists, i.e. every ticket already belongs to an existing winner — the
/// originally drawn `candidate` is returned so the draw still terminates.
fn resolve_unique_winner(
    env: &Env,
    seed: u64,
    tier_index: u32,
    total_tickets: u32,
    winners: &Vec<Address>,
    candidate: u32,
) -> u32 {
    OracleSeedWinnerSelection::new(seed).resample_unique_index(
        tier_index,
        total_tickets,
        candidate,
        |index| match get_ticket_owner(env, index + 1) {
            // A ticket with no on-chain record cannot be paid out, so it is
            // never acceptable; the caller surfaces `Error::TicketNotFound`
            // against the index that is finally used.
            Some(owner) => !winners.iter().any(|winner| winner == owner),
            None => false,
        },
    )
}

/// Number of persistent ticket entries refreshed per hot-path call.
///
/// Bumping every ticket on every purchase would be O(n) in `tickets_sold`
/// (up to `MAX_TICKETS_LIMIT` = 100,000) and blow the Soroban resource
/// budget, so each call refreshes at most this many entries. The paginated
/// [`extend_ticket_ttls`] entrypoint covers the remainder for operators.
pub(crate) const TTL_BUMP_WINDOW: u32 = 100;

/// Extend instance TTL and keep persistent ticket records alive.
///
/// Every parameter is used: `total_tickets` bounds the rolling window so a
/// long-running raffle (in particular a `no_deadline` one) keeps its
/// `DataKey::Ticket(id)` entries alive alongside the instance entry.
/// Without this, the instance entry survives while ticket entries expire
/// underneath it, so `get_ticket_owner` returns `None` and finalization
/// fails with `TicketNotFound` against escrowed funds.
///
/// Cost is O(`TTL_BUMP_WINDOW`), independent of `total_tickets`.
pub(crate) fn bump_raffle_ttl(env: &Env, total_tickets: u32) {
    env.storage().instance().extend_ttl(
        raffle_shared::constants::INSTANCE_TTL_THRESHOLD_LEDGERS,
        raffle_shared::constants::INSTANCE_TTL_BUMP_LEDGERS,
    );
    if total_tickets == 0 {
        return;
    }
    // Rolling window: resume where the previous call left off so repeated
    // purchases cycle through all tickets over time.
    let start: u32 = env
        .storage()
        .instance()
        .get(&DataKey::LastBumpedIndex)
        .unwrap_or(0);
    let start = start.min(total_tickets);
    let end = start.saturating_add(TTL_BUMP_WINDOW).min(total_tickets);
    extend_ticket_ttls(env, start.saturating_add(1), end.saturating_sub(start));
    let next = if end >= total_tickets { 0 } else { end };
    env.storage()
        .instance()
        .set(&DataKey::LastBumpedIndex, &next);
}

/// Refresh the persistent TTL of the tickets touched by the current purchase
/// plus the buyer's index entries.
///
/// `first_ticket_id` is the 1-indexed ID of the first ticket minted in this
/// call and `quantity` the number minted. The buyer's `TicketCount` and
/// `OwnerTickets` entries are refreshed too, since per-address caps are read
/// from them on every purchase.
#[allow(dead_code)]
pub(crate) fn bump_touched_tickets(
    env: &Env,
    buyer: &Address,
    first_ticket_id: u32,
    quantity: u32,
) {
    for offset in 0..quantity {
        if let Some(ticket_id) = first_ticket_id.checked_add(offset) {
            env.storage().persistent().extend_ttl(
                &DataKey::Ticket(ticket_id),
                raffle_shared::constants::PERSISTENT_TTL_THRESHOLD_LEDGERS,
                raffle_shared::constants::PERSISTENT_TTL_BUMP_LEDGERS,
            );
            env.storage().persistent().extend_ttl(
                &DataKey::TicketRefunded(ticket_id),
                raffle_shared::constants::PERSISTENT_TTL_THRESHOLD_LEDGERS,
                raffle_shared::constants::PERSISTENT_TTL_BUMP_LEDGERS,
            );
        }
    }
    env.storage().persistent().extend_ttl(
        &DataKey::TicketCount(buyer.clone()),
        raffle_shared::constants::PERSISTENT_TTL_THRESHOLD_LEDGERS,
        raffle_shared::constants::PERSISTENT_TTL_BUMP_LEDGERS,
    );
    env.storage().persistent().extend_ttl(
        &DataKey::OwnerTickets(buyer.clone()),
        raffle_shared::constants::PERSISTENT_TTL_THRESHOLD_LEDGERS,
        raffle_shared::constants::PERSISTENT_TTL_BUMP_LEDGERS,
    );
}

/// Paginated operator helper: refresh `limit` ticket entries starting at
/// 1-indexed `start_ticket_id`.
///
/// Exposed to operators through the `extend_ttl` entrypoint so anyone can
/// keep a long-running raffle's ticket records alive without an O(n) call.
/// Returns the number of entries refreshed.
pub(crate) fn extend_ticket_ttls(env: &Env, start_ticket_id: u32, limit: u32) -> u32 {
    let mut refreshed = 0u32;
    for offset in 0..limit {
        if let Some(ticket_id) = start_ticket_id.checked_add(offset) {
            if !env
                .storage()
                .persistent()
                .has(&DataKey::Ticket(ticket_id))
            {
                break;
            }
            env.storage().persistent().extend_ttl(
                &DataKey::Ticket(ticket_id),
                raffle_shared::constants::PERSISTENT_TTL_THRESHOLD_LEDGERS,
                raffle_shared::constants::PERSISTENT_TTL_BUMP_LEDGERS,
            );
            refreshed = refreshed.saturating_add(1);
        }
    }
    refreshed
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

/// Shared lifecycle guard for both randomness callbacks (#987).
///
/// Requires that the raffle is in the drawing phase, that the drawing lock is
/// held, that a randomness request is pending for `request_id`, and that the
/// supplied `request_id` is the one on record.  Used by
/// [`provide_randomness`](crate::draw::provide_randomness) and
/// [`provide_quorum_randomness`](crate::draw::provide_quorum_randomness) so
/// the two paths cannot drift apart on lifecycle checks.
///
/// # Errors
///
/// - [`Error::InvalidStateTransition`] — the raffle is not drawing (for
///   example the draw already finalized, or a further seed is submitted after
///   finalization).
/// - [`Error::DrawingNotStarted`] — the drawing lock is not held.
/// - [`Error::NoRandomnessRequest`] — no request is pending.
/// - [`Error::InvalidParameters`] — `request_id` does not match the pending
///   request.
pub(crate) fn require_awaiting_randomness(
    env: &Env,
    raffle: &Raffle,
    request_id: u64,
) -> Result<(), Error> {
    if raffle.status != RaffleStatus::Drawing {
        return Err(Error::InvalidStateTransition);
    }

    let drawing_lock: bool = env
        .storage()
        .instance()
        .get(&DataKey::DrawingLock)
        .unwrap_or(false);
    if !drawing_lock {
        return Err(Error::DrawingNotStarted);
    }

    let pending: bool = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequested)
        .unwrap_or(false);
    if !pending {
        return Err(Error::NoRandomnessRequest);
    }

    let stored: u64 = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequestId)
        .ok_or(Error::NoRandomnessRequest)?;
    if stored != request_id {
        return Err(Error::InvalidParameters);
    }

    Ok(())
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

/// Blocks ticket purchases (and other guarded ops) while the protocol-wide
/// **global pause** is engaged.
///
/// See [`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) for the authoritative pause-flag precedence table and incident response.
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

/// Compute the full pricing breakdown for buying `quantity` tickets.
///
/// Applies the early-bird discount when `raffle.tickets_sold` is below the
/// configured threshold and returns the gross, discount, fee, net charge, and
/// effective per-ticket price.  Both `buy_tickets` and `preview_buy` route
/// through this function so quote and execution cannot diverge.
pub(crate) fn calculate_buy_quote(raffle: &Raffle, quantity: u32) -> Result<BuyQuote, Error> {
    if quantity == 0 {
        return Err(Error::InvalidQuantity);
    }

    let gross = raffle
        .ticket_price
        .checked_mul(quantity as i128)
        .ok_or(Error::ArithmeticOverflow)?;

    let discount = if raffle.early_bird_discount_bp > 0
        && raffle.early_bird_ticket_percentage > 0
    {
        let threshold = (raffle.max_tickets as u64)
            .checked_mul(raffle.early_bird_ticket_percentage as u64)
            .ok_or(Error::ArithmeticOverflow)?
            / 100;
        if (raffle.tickets_sold as u64) < threshold {
            raffle_shared::apply_bp(gross, raffle.early_bird_discount_bp)
                .ok_or(Error::ArithmeticOverflow)?
            apply_bp(gross, raffle.early_bird_discount_bp).map_err(|_| Error::ArithmeticOverflow)?
        } else {
            0
        }
    } else {
        0
    };

    let net_to_pay = gross
        .checked_sub(discount)
        .ok_or(Error::ArithmeticOverflow)?;

    let fee = raffle_shared::apply_bp(net_to_pay, raffle.protocol_fee_bp)
        .ok_or(Error::ArithmeticOverflow)?;
    let fee = apply_bp(net_to_pay, raffle.protocol_fee_bp).map_err(|_| Error::ArithmeticOverflow)?;

    let effective_ticket_price = net_to_pay / (quantity as i128);

    Ok(BuyQuote {
        gross,
        discount,
        fee,
        net_to_pay,
        effective_ticket_price,
    })
}

pub(crate) fn build_internal_seed_u64(env: &Env) -> u64 {
    let xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.ledger().network_id(),
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
    if raffle.prizes.is_empty() {
        return Err(Error::InvalidParameters);
    }
    if tier_index >= raffle.prizes.len() {
        return Err(Error::InvalidIndex);
    }
    let last_tier_index = raffle.prizes.len() - 1;
    if tier_index == last_tier_index {
        let mut allocated = 0i128;
        for i in 0..last_tier_index {
            let bp = raffle.prizes.get(i).ok_or(Error::InvalidIndex)?;
            let amt = raffle_shared::apply_bp(raffle.prize_amount, bp)
                .ok_or(Error::ArithmeticOverflow)?
                .checked_add(allocated)
                .ok_or(Error::ArithmeticOverflow)?;
            allocated = allocated
                .checked_add(amt)
                .ok_or(Error::ArithmeticOverflow)?;
        }
        raffle
            .prize_amount
            .checked_sub(allocated)
            .ok_or(Error::ArithmeticOverflow)
    } else {
        let bp = raffle.prizes.get(tier_index).ok_or(Error::InvalidIndex)?;
        raffle_shared::apply_bp(raffle.prize_amount, bp)
            .ok_or(Error::ArithmeticOverflow)
    }
    let bp = raffle.prizes.get(tier_index).ok_or(Error::InvalidIndex)?;
    raffle_shared::apply_bp(raffle.prize_amount, bp)
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
///    winner claim flags, and `finalized_at`.
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
    quorum_contributions: Option<Vec<(Address, u64)>>,
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
    // `winning_indices` holds zero-based positions into the ticket pool. They
    // are stored verbatim in FairnessMetadata and in the RaffleFinalized event
    // so off-chain verifiers can replay the draw; the public WinnerDrawn event
    // emits the 1-indexed ticket ID (`position + 1`).
    let mut winning_indices =
        selector.select_winner_indices(env, total_tickets, raffle.prizes.len());
    let mut winners: Vec<Address> = Vec::new(env);

    for i in 0..winning_indices.len() {
        let mut idx = winning_indices.get(i).ok_or(Error::InvalidIndex)?;
        if raffle.unique_winners {
            idx = resolve_unique_winner(env, seed, i as u32, total_tickets, &winners, idx);
            winning_indices.set(i, idx);
        }
        let owner = get_ticket_owner(env, idx + 1).ok_or(Error::TicketNotFound)?;
        winners.push_back(owner.clone());
        WinnerDrawn {
            winner: owner,
            ticket_id: idx + 1,
            tier_index: i,
            timestamp: env.ledger().timestamp(),
        }
        .publish(env);
    }

    env.storage().persistent().set(
        &DataKey::RandomnessSeed,
        &FairnessMetadata {
            seed,
            randomness_source: raffle.randomness_source.clone(),
            winning_ticket_indices: winning_indices.clone(),
            draw_timestamp: env.ledger().timestamp(),
            draw_sequence: env.ledger().sequence(),
            unique_winners: raffle.unique_winners,
            quorum_contributions,
        },
    );

    let mut winner_records = Vec::new(env);
    for winner in winners.iter() {
        winner_records.push_back(crate::Winner {
            address: winner,
            claimed: false,
        });
    }
    raffle.winners = winner_records;
    raffle.finalized_at = Some(env.ledger().timestamp());
    transition_status(
        env,
        &mut raffle,
        RaffleStatus::Finalized,
        env.ledger().timestamp(),
    )?;
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
    crate::draw::clear_quorum_storage(env);
    env.storage().instance().set(&DataKey::DrawingLock, &false);

    let mut winner_addresses = Vec::new(env);
    for w in winners.iter() {
        winner_addresses.push_back(w);
    }

    RaffleFinalized {
        raffle_id: env.current_contract_address(),
        winners: winner_addresses,
        winning_ticket_ids: winning_indices,
        total_tickets_sold: raffle.tickets_sold,
        randomness_source: raffle.randomness_source.clone(),
        randomness_type,
        finalized_at: env.ledger().timestamp(),
        unique_winners: raffle.unique_winners,
    }
    .publish(env);

    record_leaderboard(env, &raffle);
    bump_raffle_ttl(env, total_tickets);
    Ok(())
}

fn record_leaderboard(env: &Env, raffle: &Raffle) {
    let factory: Address = match env.storage().instance().get(&DataKey::Factory) {
        Some(f) => f,
        None => return,
    };
    let raffle_id = env.current_contract_address();
    
    let tickets = raffle.tickets_sold;
    let max_tickets = raffle.max_tickets;
    let eb_limit = (max_tickets.saturating_mul(raffle.early_bird_ticket_percentage)) / 100;
    
    let eb_tickets = if tickets < eb_limit { tickets } else { eb_limit };
    let norm_tickets = tickets.saturating_sub(eb_tickets);
    
    let discounted_price = apply_bp(
        raffle.ticket_price,
        (10_000u32).saturating_sub(raffle.early_bird_discount_bp),
    )
    .unwrap_or(raffle.ticket_price);

    let volume = (eb_tickets as i128).saturating_mul(discounted_price)
        .saturating_add((norm_tickets as i128).saturating_mul(raffle.ticket_price));

    let tickets_i128 = tickets as i128;
    let args: Vec<Val> = (
        raffle_id.clone(),
        tickets_i128,
        raffle.prize_amount,
        volume,
    )
        .into_val(env);

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
        use raffle_shared::BuyQuote;

/// Shared by `buy_tickets` and `preview_buy` so charges never diverge.
///
/// Precedence:
/// 1. Best bundle with `quantity <= purchase qty` (else list `ticket_price`)
/// 2. Early-bird discount on that unit price (window from `tickets_sold`)
/// 3. Protocol fee on post-discount total
pub(crate) fn calculate_buy_quote(raffle: &Raffle, quantity: u32) -> Result<BuyQuote, Error> {
    if quantity == 0 {
        return Err(Error::InvalidQuantity);
    }

    let mut unit = raffle.ticket_price;
    for i in 0..raffle.bundles.len() {
        let b = raffle.bundles.get(i).unwrap();
        if b.quantity <= quantity {
            unit = b.price_per_ticket;
        }
    }

    let gross = unit
        .checked_mul(quantity as i128)
        .ok_or(Error::ArithmeticOverflow)?;

    let mut discount: i128 = 0;
    if raffle.early_bird_ticket_percentage > 0 && raffle.early_bird_discount_bp > 0 {
        let eb_cap = (raffle.max_tickets as u64)
            .saturating_mul(raffle.early_bird_ticket_percentage as u64)
            / 100;
        let sold = raffle.tickets_sold as u64;
        if sold < eb_cap {
            let remaining = (eb_cap - sold).min(quantity as u64) as i128;
            let disc_per = raffle_shared::apply_bp(unit, raffle.early_bird_discount_bp)
                .ok_or(Error::ArithmeticOverflow)?;
            discount = disc_per
                .checked_mul(remaining)
                .ok_or(Error::ArithmeticOverflow)?;
        }
    }

    let after_discount = gross
        .checked_sub(discount)
        .ok_or(Error::ArithmeticOverflow)?;

    // Floor fee — match current buy_tickets style (fee = total * bp / 10000)
    let fee = raffle_shared::apply_bp(after_discount, raffle.protocol_fee_bp)
        .ok_or(Error::ArithmeticOverflow)?;

    let effective_ticket_price = after_discount
        .checked_div(quantity as i128)
        .ok_or(Error::ArithmeticOverflow)?;

    Ok(BuyQuote {
        gross,
        discount,
        fee,
        net_to_pay: after_discount,
        effective_ticket_price,

    );
}

use raffle_shared::BuyQuote;
use raffle_shared::math::{apply_bp, split_bp};

    let mut unit = raffle.ticket_price;
    for i in 0..raffle.bundles.len() {
        let b = raffle.bundles.get(i).unwrap();
        if b.quantity <= quantity {
            unit = b.price_per_ticket;
        }
    }

    let gross = unit
        .checked_mul(quantity as i128)
        .ok_or(Error::ArithmeticOverflow)?;

    let mut discount: i128 = 0;
    if raffle.early_bird_ticket_percentage > 0 && raffle.early_bird_discount_bp > 0 {
        let eb_cap = (raffle.max_tickets as u64)
            .saturating_mul(raffle.early_bird_ticket_percentage as u64)
            / 100;
        let sold = raffle.tickets_sold as u64;
        if sold < eb_cap {
            let remaining = (eb_cap - sold).min(quantity as u64) as i128;
            let disc_per = unit
                .checked_mul(raffle.early_bird_discount_bp as i128)
                .ok_or(Error::ArithmeticOverflow)?
                / 10_000;
            discount = disc_per
                .checked_mul(remaining)
                .ok_or(Error::ArithmeticOverflow)?;
        }
    }

    let after_discount = gross
        .checked_sub(discount)
        .ok_or(Error::ArithmeticOverflow)?;

    // Floor fee — match current buy_tickets style (fee = total * bp / 10000)
    let fee = after_discount
        .checked_mul(raffle.protocol_fee_bp as i128)
        .ok_or(Error::ArithmeticOverflow)?
        / 10_000;

    let effective_ticket_price = after_discount
        .checked_div(quantity as i128)
        .ok_or(Error::ArithmeticOverflow)?;

    Ok(BuyQuote {
        gross,
        discount,
        fee,
        net_to_pay: after_discount,
        effective_ticket_price,
    })
}


#[cfg(any(test, feature = "testutils"))]
fn checked_add(lhs: i128, rhs: i128, label: &str) -> i128 {
    lhs.checked_add(rhs)
        .unwrap_or_else(|| panic!("solvency invariant overflow while adding {label}"))
}

#[cfg(any(test, feature = "testutils"))]
fn ticket_payment_amount(raffle: &Raffle, ticket_id: u32) -> i128 {
    let discounted_tickets =
        (raffle.max_tickets as u64 * raffle.early_bird_ticket_percentage as u64 / 100) as u32;
    if ticket_id > discounted_tickets || raffle.early_bird_discount_bp == 0 {
        return raffle.ticket_price;
    }

    let discount = raffle_shared::apply_bp(raffle.ticket_price, raffle.early_bird_discount_bp)
    let discount = apply_bp(raffle.ticket_price, raffle.early_bird_discount_bp)
        .expect("solvency invariant overflow while calculating early-bird discount");
    raffle
        .ticket_price
        .checked_sub(discount)
        .expect("solvency invariant underflow while calculating early-bird ticket payment")
}

#[cfg(any(test, feature = "testutils"))]
fn unrefunded_ticket_total(env: &Env, raffle: &Raffle) -> i128 {
    if matches!(raffle.status, RaffleStatus::Finalized | RaffleStatus::Claimed) {
        return 0;
    }

    let mut total = 0i128;
    for ticket_id in 1..=raffle.tickets_sold {
        if !env.storage().persistent().has(&DataKey::TicketRefunded(ticket_id)) {
            // Use the price actually charged (bundles / early-bird / fee
            // rounding) rather than recomputing it from the config, so the
            // escrow identity matches what `refund_ticket` will pay out.
            let paid = env
                .storage()
                .persistent()
                .get::<_, crate::Ticket>(&DataKey::Ticket(ticket_id))
                .map(|t| t.price_paid)
                .unwrap_or_else(|| ticket_payment_amount(raffle, ticket_id));
            total = checked_add(total, paid, "ticket refunds");
        }
    }
    total
}

#[cfg(any(test, feature = "testutils"))]
fn unclaimed_prize_total(raffle: &Raffle) -> i128 {
    if !raffle.prize_deposited {
        return 0;
    }
    if raffle.status == RaffleStatus::Claimed {
        // Every tier has been paid out; nothing is left in escrow.
        return 0;
    }
    if raffle.status != RaffleStatus::Finalized {
        return raffle.prize_amount;
    }

    let mut total = 0i128;
    for tier_index in 0..raffle.winners.len() {
        if !raffle
            .winners
            .get(tier_index)
            .map(|w| w.claimed)
            .unwrap_or(false)
        {
            let amount = calculate_tier_prize(raffle, tier_index)
                .expect("solvency invariant failed to calculate tier prize");
            total = checked_add(total, amount, "unclaimed prizes");
        }
    }
    total
}

/// Assert that configured-token balances cover all stored raffle entitlements.
///
/// This is test/fuzz-only. It derives every obligation from contract storage:
/// deposited/unclaimed prizes, unrefunded tickets, and recorded accumulated
/// fees. When `payment_token == prize_token`, both sides are folded into one
/// combined inequality over the single token balance.
#[cfg(any(test, feature = "testutils"))]
pub fn assert_solvent(env: &Env) {
    let raffle = read_raffle(env).expect("solvency invariant requires initialized raffle");
    let prize_owed = unclaimed_prize_total(&raffle);
    // Protocol fees are forwarded to the treasury when the tickets are sold,
    // so the contract no longer owes them; only un-refunded ticket payments
    // remain a payment-token obligation.
    let payment_owed = unrefunded_ticket_total(env, &raffle);

    let payment_balance =
        token::Client::new(env, &raffle.payment_token).balance(&env.current_contract_address());
    if raffle.payment_token == raffle.prize_token {
        let combined_owed = checked_add(payment_owed, prize_owed, "combined entitlements");
        assert!(
            payment_balance >= combined_owed,
            "escrow insolvent for combined token: balance {payment_balance}, owed {combined_owed}"
        );
        return;
    }

    let prize_balance =
        token::Client::new(env, &raffle.prize_token).balance(&env.current_contract_address());
    assert!(
        prize_balance >= prize_owed,
        "escrow insolvent for prize token: balance {prize_balance}, owed {prize_owed}"
    );
    assert!(
        payment_balance >= payment_owed,
        "escrow insolvent for payment token: balance {payment_balance}, owed {payment_owed}"
    );
}

pub(crate) fn calculate_buy_quote(
    raffle: &Raffle,
    quantity: u32,
) -> Result<(i128, i128, i128), Error> {
    let early_bird_cap = (raffle.ticket_supply as u64)
        .checked_mul(raffle.early_bird_ticket_percentage as u64)
        .ok_or(Error::ArithmeticOverflow)?
        / 100;
    let early_bird_remaining = (early_bird_cap as u32).saturating_sub(raffle.tickets_sold);
    let early_bird_quantity = u32::min(quantity, early_bird_remaining);
    let regular_quantity = quantity - early_bird_quantity;

    let discount = raffle_shared::apply_bp(raffle.ticket_price, raffle.early_bird_discount_bp)
        .ok_or(Error::ArithmeticOverflow)?;
    let discounted_price = raffle
        .ticket_price
        .checked_sub(discount)
        .ok_or(Error::ArithmeticOverflow)?;

    let early_bird_cost = (early_bird_quantity as i128)
        .checked_mul(discounted_price)
        .ok_or(Error::ArithmeticOverflow)?;
    let regular_cost = (regular_quantity as i128)
        .checked_mul(raffle.ticket_price)
        .ok_or(Error::ArithmeticOverflow)?;
    let total_price = early_bird_cost
        .checked_add(regular_cost)
        .ok_or(Error::ArithmeticOverflow)?;

    let protocol_fee = raffle_shared::apply_bp(total_price, raffle.protocol_fee_bp)
        .ok_or(Error::ArithmeticOverflow)?;

    let effective_ticket_price = total_price
        .checked_div(quantity as i128)
        .ok_or(Error::ArithmeticOverflow)?;

    Ok((total_price, protocol_fee, effective_ticket_price))
}
