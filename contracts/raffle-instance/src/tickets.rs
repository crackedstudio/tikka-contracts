//! Ticket purchase and commit-reveal entropy submission.
//!
//! This module implements the two buyer-facing entry points:
//!
//! - [`buy_tickets`] — the primary ticket-purchase function.  It validates the
//!   buyer's request, charges the payment token, issues [`Ticket`] records,
//!   and — when the last ticket sells — automatically triggers the drawing
//!   phase.
//!
//! - [`submit_commit`] — used exclusively with the
//!   [`RandomnessSource::CommitReveal`] mode.  Ticket holders submit a hash
//!   commitment before the draw so that their entropy is folded into the final
//!   seed during [`finalize_raffle`].
//!
//! ## Ticket numbering
//!
//! Tickets are numbered starting at 1.  The `ticket_id` of the `i`th ticket
//! purchased in a transaction equals `tickets_sold_before_tx + i`.  IDs are
//! monotonically increasing and never reused or reordered.
//!
//! ## Concurrent purchase guard
//!
//! `buy_tickets` performs a snapshot-then-verify pattern: it reads
//! `tickets_sold` before charging the buyer, then re-reads it from storage
//! immediately before writing new tickets to detect concurrent modifications.
//! If the counts diverge, it returns [`Error::InvalidStateTransition`].
//!
//! ## Automatic draw trigger
//!
//! When the purchase fills the last available slot (`tickets_sold >=
//! max_tickets`), `buy_tickets` calls [`transition_to_drawing`] and, for
//! `External` randomness, calls [`request_randomness`] to notify the oracle.
//!
//! See [`docs/RANDOMNESS.md`](../../../../docs/RANDOMNESS.md) for the full
//! description of each randomness mode.

use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    token, Address, BytesN, Env, IntoVal, Symbol, Val, Vec,
};

use raffle_shared::{RandomnessSource, Ticket};

use crate::events::{DrawTriggered, RandomnessRequested, TicketPurchased};
use crate::helpers::calculate_buy_quote;
use crate::{
    request_randomness, require_not_paused, transition_to_drawing, CommitRevealEntry, DataKey,
    Error, RaffleStatus,
};
use crate::helpers::bump_raffle_ttl;

/// Purchase one or more raffle tickets for `buyer`.
///
/// ## Purchase flow
///
/// 1. Validates [`DrawingLock`](DataKey::DrawingLock) is not set — an active
///    draw blocks new purchases.
/// 2. Validates `quantity > 0` and `quantity ≤ raffle.max_tickets_per_tx`.
/// 3. Requires authorization from `buyer`.
/// 4. Checks the contract is not paused and ticket sales are not paused.
/// 5. Verifies the raffle is `Active`, prize has been deposited, and the
///    deadline has not passed (unless `no_deadline` is set).
/// 6. Checks `allow_multiple` — when `false`, each address may only hold one
///    ticket across all transactions. When `max_tickets_per_address > 0`, the
///    per-address cap is enforced and exceeding it returns
///    [`Error::ExceedsMaxTicketsPerAddress`].
/// 7. Performs a double-read concurrency guard (snapshot vs. persisted state).
/// 8. Writes each [`Ticket`] to persistent storage under
///    [`DataKey::Ticket(id)`](crate::DataKey::Ticket).
/// 9. Charges `ticket_price * quantity` from the buyer via
///    `try_transfer`; deducts `protocol_fee` to the treasury.
/// 10. If the purchase fills the raffle, calls [`transition_to_drawing`] and
///     (for `External` mode) [`request_randomness`].
/// 11. Reports volume to the factory via a cross-contract `record_volume` call.
///
/// ## Protocol fee
///
/// The fee is collected at purchase time only. Prize-claim fee accounting is
/// not implemented. Formula: `floor(total_price × protocol_fee_bp / 10 000)`.
///
/// # Auth
///
/// Requires authorization from `buyer`.
///
/// # Parameters
///
/// - `buyer` — Address purchasing tickets (pays `ticket_price * quantity`).
/// - `quantity` — Number of tickets to purchase in this transaction.
///
/// # Returns
///
/// The new total number of tickets sold across the entire raffle after this
/// purchase completes.
///
/// # Errors
///
/// - [`Error::DrawingAlreadyInProgress`] — `DrawingLock` is set; draw is in
///   progress.
/// - [`Error::InvalidQuantity`] — `quantity == 0`.
/// - [`Error::ExceedsMaxTicketsPerTx`] — `quantity > max_tickets_per_tx`.
/// - [`Error::NotInitialized`] — raffle state missing.
/// - [`Error::ContractPaused`] — contract or ticket sales are paused.
/// - [`Error::RaffleInactive`] — raffle is not in `Active` status.
/// - [`Error::InvalidStateTransition`] — prize not yet deposited, or
///   concurrent modification detected.
/// - [`Error::RaffleExpired`] — deadline passed and `no_deadline` is false.
/// - [`Error::TicketsSoldOut`] — purchase would exceed `max_tickets`.
/// - [`Error::ExceedsMaxTicketsPerAddress`] — purchase would exceed
///   `max_tickets_per_address`.
/// - [`Error::MultipleTicketsNotAllowed`] — `allow_multiple` is false and
///   buyer already holds a ticket or `quantity > 1`.
/// - [`Error::InvalidParameters`] — arithmetic overflow computing
///   `total_price`.
/// - [`Error::ArithmeticOverflow`] — arithmetic overflow computing
///   `protocol_fee`.
/// - [`Error::TokenTransferFailed`] — payment token transfer failed.
///
/// # Events
///
/// - [`events::TicketPurchased`] — always emitted on success.
/// - [`events::DrawTriggered`] — emitted when the purchase fills the raffle.
/// - [`events::RandomnessRequested`] — emitted when `External` randomness is
///   requested after a sell-out.
///
/// See also: [`docs/EVENTS.md`](../../../../docs/EVENTS.md) —
/// `TicketPurchased`, `DrawTriggered`, `RandomnessRequested`.
pub(crate) fn buy_tickets(env: Env, buyer: Address, quantity: u32) -> Result<u32, Error> {
    //  1. Take reentrancy guard FIRST
    let _guard = Guard::new(&env)?;

    //  2. Validate inputs
    let drawing_lock: bool = env
        .storage()
        .instance()
        .get(&crate::DataKey::DrawingLock)
        .unwrap_or(false);
    if drawing_lock {
        return Err(Error::DrawingAlreadyInProgress);
    }
    if quantity == 0 {
        return Err(Error::InvalidQuantity);
    }
    if quantity > raffle.max_tickets_per_tx {
        return Err(Error::ExceedsMaxTicketsPerTx);
    }
    require_not_paused(env)?;
    crate::require_global_not_paused(env)?;

    if raffle.status != RaffleStatus::Active {
        return Err(Error::RaffleInactive);
    }
    if raffle.ticket_sales_paused {
        return Err(Error::ContractPaused);
    }
    if !raffle.prize_deposited {
        return Err(Error::InvalidStateTransition);
    }
    if !raffle.no_deadline && env.ledger().timestamp() >= raffle.end_time {
        return Err(Error::RaffleExpired);
    }
    Ok(())
}

pub(crate) fn buy_tickets(env: Env, buyer: Address, quantity: u32) -> Result<u32, Error> {
    buyer.require_auth();
    let mut raffle = crate::read_raffle(&env)?;
    validate_purchase_preconditions(&env, &raffle, quantity)?;

    let snapshot_sold = raffle.tickets_sold;
    let current_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::TicketCount(buyer.clone()))
        .unwrap_or(0);

    if snapshot_sold
        .checked_add(quantity)
        .ok_or(Error::ArithmeticOverflow)?
        > raffle.max_tickets
    {
        return Err(Error::TicketsSoldOut);
    }
    if raffle.max_tickets_per_address == 0
        && !raffle.allow_multiple
        && (current_count > 0 || quantity > 1)
    {
        return Err(Error::MultipleTicketsNotAllowed);
    }
    if raffle.max_tickets_per_address > 0
        && current_count
            .checked_add(quantity)
            .ok_or(Error::ArithmeticOverflow)?
            > raffle.max_tickets_per_address
    {
        return Err(Error::ExceedsMaxTicketsPerAddress);
    }

    let timestamp = env.ledger().timestamp();
    let quote = calculate_buy_quote(&raffle, quantity)?;
    let total_price = quote.net_to_pay;
    let protocol_fee = quote.fee;
    let effective_price = quote.effective_ticket_price;

    //  3. Verify no concurrent modification
    let persisted = crate::read_raffle(&env)?;
    let persisted_sold = persisted.tickets_sold;
    let persisted_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::TicketCount(buyer.clone()))
        .unwrap_or(0);
    if persisted_sold != snapshot_sold || persisted_count != current_count {
        return Err(Error::InvalidStateTransition);
    }
    if persisted_sold
        .checked_add(quantity)
        .ok_or(Error::ArithmeticOverflow)?
        > persisted.max_tickets
    {
        return Err(Error::TicketsSoldOut);
    }

    //  4. PAYMENT FIRST! (before any state mutation)
    let token_client = token::Client::new(&env, &raffle.payment_token);
    let contract_address = env.current_contract_address();
    token_client
        .try_transfer(&buyer, &contract_address, &total_price)
        .map_err(|_| Error::TokenTransferFailed)?;

    //  5. Transfer protocol fee to treasury
    if protocol_fee > 0 {
        if let Some(treasury) = &raffle.treasury_address {
            token_client.transfer(&contract_address, treasury, &protocol_fee);
        }
        let prev: i128 = env
            .storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::AccumulatedFees, &(prev + protocol_fee));
    }

    //  6. NOW mutate state (write tickets)
    if current_count == 0 {
        let mut buyers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::TicketBuyers)
            .unwrap_or_else(|| Vec::new(&env));
        buyers.push_back(buyer.clone());
        env.storage()
            .persistent()
            .set(&DataKey::TicketBuyers, &buyers);
    }

    let mut ticket_ids = Vec::new(&env);
    for i in 0..quantity {
        let ticket_id = snapshot_sold
            .checked_add(i)
            .and_then(|v| v.checked_add(1))
            .ok_or(Error::ArithmeticOverflow)?;
        let ticket = Ticket {
            id: ticket_id,
            owner: buyer.clone(),
            payer: buyer.clone(),
            purchase_time: timestamp,
            ticket_number: ticket_id,
            price_paid: raffle.ticket_price,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        ticket_ids.push_back(ticket_id);
    }

    env.storage().persistent().set(
        &DataKey::TicketCount(buyer.clone()),
        &current_count
            .checked_add(quantity)
            .ok_or(Error::ArithmeticOverflow)?,
    );
    raffle.tickets_sold = snapshot_sold
        .checked_add(quantity)
        .ok_or(Error::ArithmeticOverflow)?;

    //  7. transition_to_drawing and request_randomness (if needed)
    if raffle.tickets_sold >= raffle.max_tickets {
        transition_to_drawing(&env, &mut raffle, timestamp)?;
        if raffle.randomness_source == RandomnessSource::External {
            let request_id = request_randomness(&env)?;
            DrawTriggered {
                caller: buyer.clone(),
                total_tickets_sold: raffle.tickets_sold,
                timestamp,
            }
            .publish(&env);
            RandomnessRequested {
                oracle: raffle
                    .oracle_address
                    .clone()
                    .unwrap_or(env.current_contract_address()),
                request_id,
                timestamp,
            }
            .publish(&env);
        }
    }

    crate::write_raffle(&env, &raffle);

    //  8. Emit event (after state is final)
    TicketPurchased {
        buyer: buyer.clone(),
        ticket_ids,
        quantity,
        ticket_price: raffle.ticket_price,
        effective_ticket_price: raffle.ticket_price,
        total_paid: total_price,
        protocol_fee,
        timestamp,
    }
    .publish(&env);

    //  9. Factory notifications LAST (external calls)
    if let Some(factory_address) = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Factory)
    {
        let args: Vec<Val> = (raffle.payment_token.clone(), total_price).into_val(&env);
        env.authorize_as_current_contract(Vec::from_array(
            &env,
            [InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: factory_address.clone(),
                    fn_name: Symbol::new(&env, "record_volume"),
                    args: args.clone(),
                },
                sub_invocations: Vec::new(&env),
            })],
        ));
        env.invoke_contract::<()>(&factory_address, &Symbol::new(&env, "record_volume"), args);
        env.invoke_contract::<()>(
            &factory_address,
            &Symbol::new(&env, "track_participant"),
            (buyer,).into_val(&env),
        );
    }

        fix/security-checks-effects-763
    //  10. Bump TTLs

    let token_client = token::Client::new(&env, &raffle.payment_token);
    let _ = token_client
        .try_transfer(&buyer, env.current_contract_address(), &total_price)
        .map_err(|_| Error::TokenTransferFailed)?;

    if protocol_fee > 0 {
        if let Some(treasury) = &raffle.treasury_address {
            token_client.transfer(&env.current_contract_address(), treasury, &protocol_fee);
        }
        let prev: i128 = env
            .storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::AccumulatedFees, &(prev + protocol_fee));
    }

    TicketPurchased {
        buyer,
        ticket_ids,
        quantity,
        ticket_price: raffle.ticket_price,
        effective_ticket_price: raffle.ticket_price,
        total_paid: total_price,
        protocol_fee,
        timestamp,
    }
    .publish(&env);

    // Opportunistically bump TTLs so a long-running raffle doesn't get archived.
        master
    bump_raffle_ttl(&env, raffle.tickets_sold);

    Ok(raffle.tickets_sold)
}

/// Purchase one or more raffle tickets on behalf of `recipient`, paid for by
/// `buyer` (a "gift" purchase).
///
/// Shares most of [`buy_tickets`]'s validation, fee model, and automatic
/// draw trigger — except `recipient` (not `buyer`) is the address credited
/// with owning the tickets and checked against `max_tickets_per_address` /
/// `allow_multiple`; `buyer` still pays and must authorize the call. Unlike
/// [`buy_tickets`], this function does **not** check the factory-level
/// global pause flag (`require_global_not_paused`) — only the per-raffle
/// `ticket_sales_paused` flag is enforced here. This is pre-existing
/// behavior, not part of the `end_time` fix below; see the raffle-wide
/// pause note under `# Errors`.
///
/// # Auth
///
/// Requires authorization from `buyer`.
///
/// # Parameters
///
/// - `buyer` — Address paying `ticket_price * quantity`.
/// - `recipient` — Address credited with the purchased tickets.
/// - `quantity` — Number of tickets to purchase in this transaction.
///
/// # Returns
///
/// The new total number of tickets sold across the entire raffle after this
/// purchase completes.
///
/// # Errors
///
/// Same error set as [`buy_tickets`] (minus the global-pause case noted
/// above), with per-address checks evaluated against `recipient` instead of
/// `buyer`. Notably:
///
/// - [`Error::RaffleExpired`] — deadline passed and `no_deadline` is false.
///   `end_time` is an exclusive boundary: the deadline is reached starting
///   at `ledger_timestamp == end_time` (see `docs/GLOSSARY.md` § "End Time").
///
/// See [`buy_tickets`] for the full error list and event documentation.
pub(crate) fn buy_tickets_for(env: Env, buyer: Address, recipient: Address, quantity: u32) -> Result<u32, Error> {
    //  1. Take reentrancy guard FIRST
    let _guard = Guard::new(&env)?;

    //  2. Validate inputs
    let drawing_lock: bool = env.storage().instance().get(&crate::DataKey::DrawingLock).unwrap_or(false);
    if drawing_lock {
        return Err(Error::DrawingAlreadyInProgress);
    }
    if quantity == 0 {
        return Err(Error::InvalidQuantity);
    }
    let mut raffle = crate::read_raffle(&env)?;
    if quantity > raffle.max_tickets_per_tx {
        return Err(Error::ExceedsMaxTicketsPerTx);
    }
    buyer.require_auth();
    require_not_paused(&env)?;
    crate::require_global_not_paused(&env)?;

    if raffle.status != RaffleStatus::Active {
        return Err(Error::RaffleInactive);
    }
    if raffle.ticket_sales_paused {
        return Err(Error::ContractPaused);
    }
    if !raffle.prize_deposited {
        return Err(Error::InvalidStateTransition);
    }
    if !raffle.no_deadline && env.ledger().timestamp() >= raffle.end_time {
        return Err(Error::RaffleExpired);
    }

    let snapshot_sold = raffle.tickets_sold;
    let current_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::TicketCount(recipient.clone()))
        .unwrap_or(0);

    if snapshot_sold
        .checked_add(quantity)
        .ok_or(Error::ArithmeticOverflow)?
        > raffle.max_tickets
    {
        return Err(Error::TicketsSoldOut);
    }
    if raffle.max_tickets_per_address == 0
        && !raffle.allow_multiple
        && (current_count > 0 || quantity > 1)
    {
        return Err(Error::MultipleTicketsNotAllowed);
    }
    if raffle.max_tickets_per_address > 0
        && current_count
            .checked_add(quantity)
            .ok_or(Error::ArithmeticOverflow)?
            > raffle.max_tickets_per_address
    {
        return Err(Error::ExceedsMaxTicketsPerAddress);
    }

    let timestamp = env.ledger().timestamp();
        fix/security-checks-effects-763
    let total_price = raffle
        .ticket_price
        .checked_mul(quantity as i128)
        .ok_or(Error::ArithmeticOverflow)?;
    let protocol_fee = total_price
        .checked_mul(raffle.protocol_fee_bp as i128)
        .ok_or(Error::ArithmeticOverflow)?
        / 10000;

    let protocol_fee = total_price.checked_mul(raffle.protocol_fee_bp as i128).ok_or(Error::ArithmeticOverflow)? / 10000;
        master

    //  3. Verify no concurrent modification
    let persisted = crate::read_raffle(&env)?;
    let persisted_sold = persisted.tickets_sold;
    let persisted_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::TicketCount(recipient.clone()))
        .unwrap_or(0);
    if persisted_sold != snapshot_sold || persisted_count != current_count {
        return Err(Error::InvalidStateTransition);
    }
    if persisted_sold
        .checked_add(quantity)
        .ok_or(Error::ArithmeticOverflow)?
        > persisted.max_tickets
    {
        return Err(Error::TicketsSoldOut);
    }

    //  4. PAYMENT FIRST! (before any state mutation)
    let token_client = token::Client::new(&env, &raffle.payment_token);
    let contract_address = env.current_contract_address();
    token_client
        .try_transfer(&buyer, &contract_address, &total_price)
        .map_err(|_| Error::TokenTransferFailed)?;

    //  5. Transfer protocol fee to treasury
    if protocol_fee > 0 {
        if let Some(treasury) = &raffle.treasury_address {
            token_client.transfer(&contract_address, treasury, &protocol_fee);
        }
        let prev: i128 = env
            .storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::AccumulatedFees, &(prev + protocol_fee));
    }

    //  6. NOW mutate state (write tickets)
    if current_count == 0 {
        let mut buyers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::TicketBuyers)
            .unwrap_or_else(|| Vec::new(&env));
        buyers.push_back(recipient.clone());
        env.storage()
            .persistent()
            .set(&DataKey::TicketBuyers, &buyers);
    }

    let mut ticket_ids = Vec::new(&env);
    for i in 0..quantity {
        let ticket_id = snapshot_sold
            .checked_add(i)
            .and_then(|v| v.checked_add(1))
            .ok_or(Error::ArithmeticOverflow)?;
        let ticket = Ticket {
            id: ticket_id,
            owner: recipient.clone(),
            purchase_time: timestamp,
            ticket_number: ticket_id,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Ticket(ticket_id), &ticket);
        ticket_ids.push_back(ticket_id);
    }

    env.storage().persistent().set(
        &DataKey::TicketCount(recipient.clone()),
        &current_count
            .checked_add(quantity)
            .ok_or(Error::ArithmeticOverflow)?,
    );
    raffle.tickets_sold = snapshot_sold
        .checked_add(quantity)
        .ok_or(Error::ArithmeticOverflow)?;

    //  7. transition_to_drawing and request_randomness (if needed)
    if raffle.tickets_sold >= raffle.max_tickets {
        transition_to_drawing(&env, &mut raffle, timestamp)?;
        if raffle.randomness_source == RandomnessSource::External {
            let request_id = request_randomness(&env)?;
            DrawTriggered {
                caller: recipient.clone(),
                total_tickets_sold: raffle.tickets_sold,
                timestamp,
            }
            .publish(&env);
            RandomnessRequested {
                oracle: raffle
                    .oracle_address
                    .clone()
                    .unwrap_or(env.current_contract_address()),
                request_id,
                timestamp,
            }
            .publish(&env);
        }
    }

    crate::write_raffle(&env, &raffle);

    //  8. Emit event (after state is final)
    TicketPurchased {
        buyer: recipient.clone(),
        ticket_ids,
        quantity,
        ticket_price: raffle.ticket_price,
        effective_ticket_price: raffle.ticket_price,
        total_paid: total_price,
        protocol_fee,
        timestamp,
    }
    .publish(&env);

    //  9. Factory notifications LAST (external calls)
    if let Some(factory_address) = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Factory)
    {
        let args: Vec<Val> = (raffle.payment_token.clone(), total_price).into_val(&env);
        env.authorize_as_current_contract(Vec::from_array(
            &env,
            [InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: factory_address.clone(),
                    fn_name: Symbol::new(&env, "record_volume"),
                    args: args.clone(),
                },
                sub_invocations: Vec::new(&env),
            })],
        ));
        env.invoke_contract::<()>(&factory_address, &Symbol::new(&env, "record_volume"), args);
        env.invoke_contract::<()>(
            &factory_address,
            &Symbol::new(&env, "track_participant"),
            (recipient,).into_val(&env),
        );
    }

    //  10. Bump TTLs
    bump_raffle_ttl(&env, raffle.tickets_sold);

    Ok(raffle.tickets_sold)
}
/// Submit a hash commitment for the [`RandomnessSource::CommitReveal`] path.
///
/// In the commit-reveal scheme each ticket holder contributes entropy to the
/// draw by committing a hash before `finalize_raffle` is called.  During
/// finalization, all discovered commits are concatenated and hashed together to
/// form the final draw seed — see [`finalize_raffle`] for details.
///
/// ## Why keyed by ticket ID?
///
/// Commits are stored under [`DataKey::CommitEntry(ticket_id)`] rather than
/// the owner's address so that the commit remains valid even if the ticket is
/// transferred after submission.  The original committer's address is preserved
/// in [`CommitRevealEntry::committer`] for audit purposes, but the draw logic
/// looks up entries by ticket ID.
///
/// ## Timing
///
/// Commits may be submitted while the raffle is `Active` **or** `Drawing`.
/// Submitting after `finalize_raffle` has already been called is rejected with
/// [`Error::InvalidStatus`].
///
/// # Auth
///
/// Requires authorization from the **current owner** of `ticket_id`.  If the
/// ticket has been transferred, the new owner must authorize.
///
/// # Parameters
///
/// - `ticket_id` — 1-indexed ID of the ticket whose entropy is being committed.
/// - `hash` — 32-byte hash of the secret pre-image.  The contract does not
///   inspect or validate the pre-image — it is the caller's responsibility to
///   reveal it off-chain during finalization monitoring.
///
/// # Errors
///
/// - [`Error::InvalidParameters`] — randomness source is not `CommitReveal`.
/// - [`Error::InvalidStatus`] — raffle is not `Active` or `Drawing`.
/// - [`Error::TicketNotFound`] — no ticket with `ticket_id` exists in storage.
///
/// See also: [`docs/RANDOMNESS.md`](../../../../docs/RANDOMNESS.md) —
/// Commit-Reveal mode,
/// [`docs/COMMIT_REVEAL.md`](../../../../docs/COMMIT_REVEAL.md).
pub(crate) fn submit_commit(env: Env, ticket_id: u32, hash: BytesN<32>) -> Result<(), Error> {
    let raffle = crate::read_raffle(&env)?;

    if raffle.randomness_source != RandomnessSource::CommitReveal {
        return Err(Error::InvalidParameters);
    }

    // Commit window: Active only — no last-look after Drawing begins.
    if raffle.status != RaffleStatus::Active {
        return Err(Error::InvalidStatus);
    }

    require_not_paused(&env)?;
    crate::require_global_not_paused(&env)?;

    let ticket: Ticket = env
        .storage()
        .persistent()
        .get(&DataKey::Ticket(ticket_id))
        .ok_or(Error::TicketNotFound)?;
    ticket.owner.require_auth();

    let key = DataKey::CommitEntry(ticket_id);
    if env.storage().persistent().has(&key) {
        return Err(Error::CommitAlreadySubmitted);
    }

    env.storage().persistent().set(
        &key,
        &CommitRevealEntry {
            committer: ticket.owner,
            hash,
        },
    );

    // Keep the commit live through a long Active period (adjust if the repo
    // already defines shared TTL constants — prefer those).
    env.storage().persistent().extend_ttl(&key, 100, 535_680);

    Ok(())
}
