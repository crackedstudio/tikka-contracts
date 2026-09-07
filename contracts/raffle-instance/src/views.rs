//! Read-only query functions for the raffle-instance contract.
//!
//! All functions in this module are pure reads — they never mutate state,
//! emit events, or require authorisation.  They exist to give off-chain
//! clients and other contracts a stable query surface without having to parse
//! raw storage keys.
//!
//! | Function | Returns |
//! |---|---|
//! | [`get_raffle`] | Full [`Raffle`](crate::Raffle) struct |
//! | [`get_fairness_data`] | Post-draw audit data ([`FairnessData`]) |
//! | [`get_stats`] | Aggregate dashboard metrics ([`RaffleStats`]) |
//! | [`is_paused`] | Whether the instance-level pause flag is set |
//! | [`is_ticket_sales_paused`] | Whether ticket sales are paused within an active raffle |
//! | [`get_accumulated_fees`] | Protocol fees collected but not yet withdrawn |

use soroban_sdk::{Address, Env, Vec};

use raffle_shared::{BuyQuote, FairnessData, RaffleStats};

use crate::helpers::calculate_buy_quote;
use crate::{read_raffle, DataKey, Error, FairnessMetadata};

/// Return the full [`Raffle`](crate::Raffle) struct from instance storage.
///
/// This is the primary state-inspection endpoint.  It returns every field of
/// the raffle configuration and runtime state in a single call.
///
/// # Errors
///
/// - [`Error::NotInitialized`] — the contract has not been initialised yet
///   (i.e., [`init`](crate::init::init) has never succeeded).
pub(crate) fn get_raffle(env: Env) -> Result<crate::Raffle, Error> {
    read_raffle(&env)
}

/// Return the post-draw fairness audit data for this raffle.
///
/// [`FairnessData`] contains everything needed for an off-chain observer to
/// independently verify that the winner selection was performed correctly:
///
/// - The seed used for winner selection.
/// - The randomness source that produced it.
/// - The ordered list of ticket IDs that were in scope for the draw.
/// - The winning ticket indices (zero-based offsets into `ticket_ids`).
/// - The ledger timestamp and sequence at draw time.
///
/// The data is stored in **persistent** storage under
/// [`DataKey::RandomnessSeed`] by
/// [`do_finalize_with_seed`](crate::helpers::do_finalize_with_seed) so it
/// survives ledger-entry TTL expiry and remains queryable long after the raffle
/// ends.
///
/// # Errors
///
/// - [`Error::InvalidStatus`] — the draw has not completed yet (no
///   `FairnessMetadata` written to storage).
/// - [`Error::NotInitialized`] — the contract has not been initialised.
///
/// See also: [`docs/RANDOMNESS.md`](../../../../docs/RANDOMNESS.md) — audit
/// and replay verification.
pub(crate) fn get_fairness_data(env: Env) -> Result<FairnessData, Error> {
    let meta: FairnessMetadata = env.storage().persistent().get(&DataKey::RandomnessSeed).ok_or(Error::InvalidStatus)?;
    let raffle = read_raffle(&env)?;
    let mut ticket_ids = Vec::new(&env);
    for i in 1..=raffle.tickets_sold { ticket_ids.push_back(i); }
    Ok(FairnessData {
        seed: meta.seed,
        randomness_source: meta.randomness_source,
        ticket_ids,
        winning_ticket_indices: meta.winning_ticket_indices,
        draw_timestamp: meta.draw_timestamp,
        draw_sequence: meta.draw_sequence,
        unique_winners: meta.unique_winners,
        quorum_contributions: meta.quorum_contributions,
    })
}

/// Return a single-call dashboard view of the raffle's key metrics.
///
/// Aggregates data from multiple storage keys — [`Raffle`](crate::Raffle),
/// [`TicketBuyers`](crate::DataKey::TicketBuyers), and
/// [`AccumulatedFees`](crate::DataKey::AccumulatedFees) — into a single
/// [`RaffleStats`] payload so frontends no longer need multiple RPC round-trips.
///
/// # Fields returned
///
/// | Field | Source |
/// |---|---|
/// | `tickets_sold` | [`Raffle::tickets_sold`](crate::Raffle) |
/// | `unique_buyers` | `TicketBuyers` — deduplicated buyer address list |
/// | `gross_revenue` | `tickets_sold * ticket_price` |
/// | `fees_accrued` | `AccumulatedFees` instance storage |
/// | `prize_funded` | [`Raffle::prize_deposited`](crate::Raffle) |
/// | `status` | [`Raffle::status`](crate::Raffle) |
/// | `time_remaining` | `end_time - ledger_timestamp` while `ledger_timestamp < end_time`; `0` at or after `end_time`, or whenever `no_deadline` is `true` (see `docs/GLOSSARY.md` § "End Time") |
///
/// # Errors
///
/// - [`Error::NotInitialized`] — the raffle has not been initialised.
/// - [`Error::ArithmeticOverflow`] — the gross revenue calculation overflows.
pub(crate) fn get_stats(env: Env) -> Result<RaffleStats, Error> {
    let raffle = read_raffle(&env)?;

    let buyers: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::TicketBuyers)
        .unwrap_or_else(|| Vec::new(&env));

    let fees_accrued: i128 = env
        .storage()
        .instance()
        .get(&DataKey::AccumulatedFees)
        .unwrap_or(0);

    let now = env.ledger().timestamp();

    let time_remaining = if raffle.no_deadline {
        0
    } else if raffle.end_time > now {
        raffle.end_time - now
    } else {
        0
    };

    let gross_revenue = (raffle.tickets_sold as i128)
        .checked_mul(raffle.ticket_price)
        .ok_or(Error::ArithmeticOverflow)?;

    Ok(RaffleStats {
        tickets_sold: raffle.tickets_sold,
        unique_buyers: buyers.len(),
        gross_revenue,
        fees_accrued,
        prize_funded: raffle.prize_deposited,
        status: raffle.status,
        time_remaining,
    })
}

/// Return `true` if the instance-level contract pause flag is set.
///
/// When `true`, [`buy_tickets`](crate::tickets::buy_tickets) and
/// [`deposit_prize`](crate::init::deposit_prize) will return
/// [`Error::ContractPaused`].  The flag is set/cleared by the factory relay
/// via the `pause` / `unpause` admin entry points.
pub(crate) fn is_paused(env: Env) -> bool {
    env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
}

/// Return `true` if ticket sales are paused within an otherwise-active raffle.
///
/// This is a finer-grained pause than [`is_paused`]: the raffle remains
/// `Active` and the prize stays locked, but new ticket purchases are rejected.
/// The flag is controlled by the creator or admin via `pause_ticket_sales` /
/// `resume_ticket_sales`.
///
/// Returns `false` if the raffle has not been initialised.
pub(crate) fn is_ticket_sales_paused(env: Env) -> bool {
    read_raffle(&env).map(|r| r.ticket_sales_paused).unwrap_or(false)
}

/// Return the number of tickets `owner` may still receive.
///
/// If `max_tickets_per_address` is non-zero, it limits the result and supersedes
/// `allow_multiple`. A zero per-address cap means unlimited, in which case
/// `allow_multiple: false` still limits an address to one ticket. The result
/// never exceeds the raffle-wide remaining capacity.
pub(crate) fn get_remaining_ticket_allowance(
    env: Env,
    owner: Address,
) -> Result<u32, Error> {
    let raffle = read_raffle(&env)?;
    let current_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::TicketCount(owner))
        .unwrap_or(0);
    let per_address_limit = if raffle.max_tickets_per_address > 0 {
        raffle.max_tickets_per_address
    } else if !raffle.allow_multiple {
        1
    } else {
        raffle.max_tickets
    };
    let global_remaining = raffle.max_tickets.saturating_sub(raffle.tickets_sold);
    let address_remaining = per_address_limit.saturating_sub(current_count);
    Ok(global_remaining.min(address_remaining))
}

/// Return the total protocol fees collected in this raffle instance that have
/// not yet been withdrawn.
///
/// Fees accumulate in instance storage under [`DataKey::AccumulatedFees`]
/// on each successful ticket purchase.  They are swept out by the
/// `withdraw_fees` admin function, which requires the raffle to be
/// `Finalized` or `Claimed`.
///
/// Returns `0` before any tickets have been sold or after fees have been
/// fully withdrawn.
pub(crate) fn get_accumulated_fees(env: Env) -> i128 {
    env.storage().instance().get(&DataKey::AccumulatedFees).unwrap_or(0)
}

/// Quote the exact cost of buying `quantity` tickets including early-bird
/// discounts and protocol fees.
///
/// This is a **read-only** view — it does not mutate state, does not require
/// authorisation, and does not check business-rule validations such as
/// raffle status, pausing, or ticket availability.  It exists purely to let
/// wallets and frontends compute the precise amount that
/// [`buy_tickets`](crate::tickets::buy_tickets) would charge for a given
/// `quantity` at the current `tickets_sold`.
///
/// The underlying helper [`calculate_buy_quote`](crate::helpers::calculate_buy_quote)
/// is the **same function** called by `buy_tickets` at execution time, so
/// the preview can never diverge from the on-chain charge.
///
/// # Returns
///
/// [`BuyQuote`] with `{ gross, discount, fee, net_to_pay, effective_ticket_price }`.
///
/// # Errors
///
/// - [`Error::NotInitialized`] — the contract has not been initialised.
/// - [`Error::InvalidQuantity`] — `quantity == 0`.
/// - [`Error::InvalidParameters`] — overflow computing the gross total.
/// - [`Error::ArithmeticOverflow`] — overflow in discount or fee math.
pub(crate) fn preview_buy(env: Env, quantity: u32) -> Result<BuyQuote, Error> {
    let raffle = read_raffle(&env)?;
    calculate_buy_quote(&raffle, quantity)
}

/// Returns the timestamp at which a scheduled admin cancel becomes
/// executable, or `None` if no cancel is currently scheduled (#406).
///
/// When an admin calls `cancel_raffle`, a cancellation is scheduled with a
/// timelock delay.  This function allows clients to check if a cancel is
/// pending and when it will become available.
///
/// Returns `None` if no admin cancel has been scheduled.
pub(crate) fn get_pending_cancel(env: Env) -> Option<u64> {
    env.storage().instance().get(&DataKey::PendingAdminCancel)
}

/// Return all ticket IDs owned by `owner`.
///
/// Uses the `OwnerTickets` index maintained during `buy_tickets` for an
/// O(1) read.  Falls back to an empty Vec when the address has never
/// purchased a ticket.
pub(crate) fn get_my_tickets(env: Env, owner: Address) -> Vec<u32> {
    env.storage()
        .persistent()
        .get(&DataKey::OwnerTickets(owner))
        .unwrap_or_else(|| Vec::new(&env))
}
