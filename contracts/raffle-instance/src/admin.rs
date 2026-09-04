use soroban_sdk::{token, Address, BytesN, Env};

use raffle_shared::CancelReason;
use raffle_shared::constants::TIMELOCK_DELAY_SECONDS;

use crate::events::{
    CancelScheduled, ContractPaused, ContractUnpaused, EmergencyWithdrawn, FeesWithdrawn,
    MetadataHashUpdated, OracleAddressUpdated, ProtocolFeeUpdated, RaffleCancelled, StorageWiped,
    SwapDeadlineUpdated, TicketSalesPaused, TicketSalesResumed, TokensRescued,
};
use crate::{
    calculate_tier_prize, read_raffle, require_admin, write_raffle, DataKey, Error, RaffleStatus,
    transition_status, EMERGENCY_WITHDRAW_DELAY_SECONDS, MAX_PROTOCOL_FEE_BP,
    MAX_SWAP_DEADLINE_SECONDS,
};

fn outstanding_ticket_refunds(env: &Env, raffle: &crate::Raffle) -> Result<i128, Error> {
    let mut outstanding = 0i128;
    for ticket_id in 1..=raffle.tickets_sold {
        if !env.storage().persistent().has(&DataKey::TicketRefunded(ticket_id)) {
            outstanding = outstanding
                .checked_add(raffle.ticket_price)
                .ok_or(Error::ArithmeticOverflow)?;
        }
    }
    Ok(outstanding)
}

fn outstanding_prize(env: &Env, raffle: &crate::Raffle) -> Result<i128, Error> {
    if !raffle.prize_deposited {
        return Ok(0);
    }
    if raffle.status != RaffleStatus::Finalized {
        return Ok(raffle.prize_amount);
    }

    let mut outstanding = 0i128;
    for tier_index in 0..raffle.winners.len() {
        if !raffle
            .winners
            .get(tier_index)
            .map(|winner| winner.claimed)
            .unwrap_or(false)
        {
            outstanding = outstanding
                .checked_add(calculate_tier_prize(raffle, tier_index)?)
                .ok_or(Error::ArithmeticOverflow)?;
        }
    }
    Ok(outstanding)
}

fn token_entitlement(env: &Env, raffle: &crate::Raffle, token: &Address) -> Result<i128, Error> {
    let mut entitlement = 0i128;
    if token == &raffle.payment_token
        && raffle.status != RaffleStatus::Finalized
        && raffle.status != RaffleStatus::Claimed
    {
        entitlement = entitlement
            .checked_add(outstanding_ticket_refunds(env, raffle)?)
            .and_then(|value| {
                value.checked_add(
                    env.storage()
                        .instance()
                        .get::<_, i128>(&DataKey::AccumulatedFees)
                        .unwrap_or(0),
                )
            })
            .ok_or(Error::ArithmeticOverflow)?;
    }
    if token == &raffle.prize_token {
        entitlement = entitlement
            .checked_add(outstanding_prize(env, raffle)?)
            .ok_or(Error::ArithmeticOverflow)?;
    }
    Ok(entitlement)
}

pub(crate) fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
    let _old = require_admin(&env)?;
    if !new_admin.exists() || new_admin == env.current_contract_address() {
        return Err(Error::InvalidAdminAddress);
    }
    // `init` and `require_admin` read/write DataKey::Admin from instance storage;
    // keep the rotated admin on the same tier or every admin entrypoint bricks
    // after a rotation (#751).
    env.storage().instance().set(&DataKey::Admin, &new_admin);
    Ok(())
}

pub(crate) fn update_oracle_address(env: Env, new_oracle: Address) -> Result<(), Error> {
    let admin = require_admin(&env)?;
    let mut raffle = read_raffle(&env)?;
    if raffle.randomness_source != raffle_shared::RandomnessSource::External {
        return Err(Error::InvalidParameters);
    }
    if new_oracle == env.current_contract_address() {
        return Err(Error::InvalidParameters);
    }
    if raffle.status == RaffleStatus::Finalized
        || raffle.status == RaffleStatus::Claimed
        || raffle.status == RaffleStatus::Cancelled
    {
        return Err(Error::InvalidStatus);
    }
    let old = raffle.oracle_address.clone();
    raffle.oracle_address = Some(new_oracle.clone());
    write_raffle(&env, &raffle);
    OracleAddressUpdated {
        old_oracle: old,
        new_oracle,
        updated_by: admin,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn set_protocol_fee_bp(env: Env, new_fee_bp: u32) -> Result<(), Error> {
    let admin = require_admin(&env)?;
    if new_fee_bp > MAX_PROTOCOL_FEE_BP {
        return Err(Error::InvalidParameters);
    }
    let mut raffle = read_raffle(&env)?;
    if raffle.tickets_sold > 0 {
        return Err(Error::InvalidStatus);
    }
    let old = raffle.protocol_fee_bp;
    raffle.protocol_fee_bp = new_fee_bp;
    write_raffle(&env, &raffle);
    ProtocolFeeUpdated {
        old_fee_bp: old,
        new_fee_bp,
        updated_by: admin,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn set_swap_deadline(env: Env, new_deadline_seconds: u64) -> Result<(), Error> {
    let admin = require_admin(&env)?;
    if new_deadline_seconds > MAX_SWAP_DEADLINE_SECONDS {
        return Err(Error::InvalidParameters);
    }
    let mut raffle = read_raffle(&env)?;
    if raffle.tickets_sold > 0 {
        return Err(Error::InvalidStatus);
    }
    let old = raffle.swap_deadline_seconds;
    raffle.swap_deadline_seconds = new_deadline_seconds;
    write_raffle(&env, &raffle);
    SwapDeadlineUpdated {
        old_deadline_seconds: old,
        new_deadline_seconds,
        updated_by: admin,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn cancel_raffle(env: Env, reason: CancelReason) -> Result<(), Error> {
    let mut raffle = read_raffle(&env)?;
    match reason {
        CancelReason::AdminCancelled => {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(Error::NotAuthorized)?;
            admin.require_auth();
            // For admin cancels, schedule the cancellation with a timelock
            let now = env.ledger().timestamp();
            let cancel_at = now.checked_add(TIMELOCK_DELAY_SECONDS)
                .ok_or(Error::ArithmeticOverflow)?;
            env.storage()
                .instance()
                .set(&DataKey::PendingAdminCancel, &cancel_at);
            CancelScheduled {
                creator: raffle.creator.clone(),
                scheduled_by: admin,
                tickets_sold: raffle.tickets_sold,
                cancel_at,
                timestamp: now,
            }
            .publish(&env);
            return Ok(());
        }
        _ => raffle.creator.require_auth(),
    }
    if raffle.status == RaffleStatus::Finalized
        || raffle.status == RaffleStatus::Cancelled
        || raffle.status == RaffleStatus::Claimed
    {
        return Err(Error::InvalidStatus);
    }
    transition_status(
        &env,
        &mut raffle,
        RaffleStatus::Cancelled,
        env.ledger().timestamp(),
    )?;
    RaffleCancelled {
        creator: raffle.creator.clone(),
        reason,
        tickets_sold: raffle.tickets_sold,
        prize_refunded: raffle.prize_deposited,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn execute_admin_cancel(env: Env) -> Result<(), Error> {
    let mut raffle = read_raffle(&env)?;
    
    // Check if there's a pending admin cancel
    let cancel_at: u64 = env
        .storage()
        .instance()
        .get(&DataKey::PendingAdminCancel)
        .ok_or(Error::CancelNotScheduled)?;
    
    // Check if the timelock has elapsed
    let now = env.ledger().timestamp();
    if now < cancel_at {
        return Err(Error::CancelTimelockActive);
    }
    
    // Check raffle status
    if raffle.status == RaffleStatus::Finalized
        || raffle.status == RaffleStatus::Cancelled
        || raffle.status == RaffleStatus::Claimed
    {
        return Err(Error::InvalidStatus);
    }
    
    // Execute the cancel
    transition_status(
        &env,
        &mut raffle,
        RaffleStatus::Cancelled,
        now,
    )?;
    
    // Clear the pending cancel
    env.storage().instance().remove(&DataKey::PendingAdminCancel);
    
    RaffleCancelled {
        creator: raffle.creator,
        reason: CancelReason::AdminCancelled,
        tickets_sold: raffle.tickets_sold,
        prize_refunded: raffle.prize_deposited,
        timestamp: now,
    }
    .publish(&env);
    
    Ok(())
}

pub(crate) fn update_metadata_hash(env: Env, new_hash: BytesN<32>) -> Result<(), Error> {
    let admin = require_admin(&env)?;
    let old_hash = env
        .storage()
        .instance()
        .get::<_, BytesN<32>>(&DataKey::MetadataHash)
        .ok_or(Error::NotInitialized)?;
    
    env.storage()
        .instance()
        .set(&DataKey::MetadataHash, &new_hash);
    
    MetadataHashUpdated {
        old_hash,
        new_hash,
        updated_by: admin,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    
    Ok(())
}

pub(crate) fn pause(env: Env) -> Result<(), Error> {
    let f: Address = env
        .storage()
        .instance()
        .get(&DataKey::Factory)
        .ok_or(Error::NotAuthorized)?;
    f.require_auth();
    env.storage().instance().set(&DataKey::Paused, &true);
    ContractPaused {
        paused_by: f,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn unpause(env: Env) -> Result<(), Error> {
    let f: Address = env
        .storage()
        .instance()
        .get(&DataKey::Factory)
        .ok_or(Error::NotAuthorized)?;
    f.require_auth();
    env.storage().instance().set(&DataKey::Paused, &false);
    ContractUnpaused {
        unpaused_by: f,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn pause_ticket_sales(env: Env, caller: Address) -> Result<(), Error> {
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
    if raffle.status != RaffleStatus::Active {
        return Err(Error::InvalidStatus);
    }
    raffle.ticket_sales_paused = true;
    write_raffle(&env, &raffle);
    TicketSalesPaused {
        paused_by: caller,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn resume_ticket_sales(env: Env, caller: Address) -> Result<(), Error> {
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
    if raffle.status != RaffleStatus::Active {
        return Err(Error::InvalidStatus);
    }
    raffle.ticket_sales_paused = false;
    write_raffle(&env, &raffle);
    TicketSalesResumed {
        resumed_by: caller,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn withdraw_fees(env: Env, recipient: Address, amount: i128) -> Result<(), Error> {
    let _admin = require_admin(&env)?;
    let raffle = read_raffle(&env)?;
    if raffle.status != RaffleStatus::Finalized && raffle.status != RaffleStatus::Claimed {
        return Err(Error::InvalidStatus);
    }
    if amount <= 0 {
        return Err(Error::InvalidParameters);
    }
    let acc: i128 = env
        .storage()
        .instance()
        .get(&DataKey::AccumulatedFees)
        .unwrap_or(0);
    if amount > acc {
        return Err(Error::InsufficientAccumulatedFees);
    }
    let tc = token::Client::new(&env, &raffle.payment_token);
    tc.transfer(&env.current_contract_address(), &recipient, &amount);
    env.storage()
        .instance()
        .set(&DataKey::AccumulatedFees, &(acc - amount));
    FeesWithdrawn {
        recipient,
        amount,
        token: raffle.payment_token.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

/// Rescue unrelated tokens, or only the surplus of a configured token.
///
/// For `payment_token` and `prize_token`, the requested amount must leave all
/// unpaid ticket refunds, accumulated fees, and outstanding prize claims in
/// the contract. Other tokens may be rescued from the available balance.
pub(crate) fn rescue_tokens(
    env: Env,
    token: Address,
    recipient: Address,
    amount: i128,
) -> Result<(), Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotAuthorized)?;
    admin.require_auth();
    if amount <= 0 {
        return Err(Error::InvalidParameters);
    }
    if let Ok(raffle) = read_raffle(&env) {
        let balance = token::Client::new(&env, &token)
            .balance(&env.current_contract_address());
        let entitlement = token_entitlement(&env, &raffle, &token)?;
        if entitlement > balance || amount > balance - entitlement {
            return Err(Error::InvalidStateTransition);
        }
    }
    let tc = token::Client::new(&env, &token);
    let _ = tc
        .try_transfer(&env.current_contract_address(), &recipient, &amount)
        .map_err(|_| Error::TokenTransferFailed)?;
    TokensRescued {
        rescued_by: admin,
        token,
        recipient,
        amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

/// Sweep residual payment-token dust to the treasury after the raffle is fully
/// settled. Only allowed in `Claimed` / `Cancelled`, and only when no prize or
/// ticket-refund entitlement remains outstanding.
/// Sweep only payment-token balance that exceeds every remaining entitlement.
///
/// This is allowed after `Claimed`, or after `Cancelled` with the prize
/// refunded and every ticket refund completed. Accumulated fees remain for
/// `withdraw_fees` and are never swept as dust.
pub(crate) fn sweep_dust(env: Env) -> Result<(), Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotAuthorized)?;
    admin.require_auth();

    let raffle = read_raffle(&env)?;
    if raffle.status != RaffleStatus::Claimed && raffle.status != RaffleStatus::Cancelled {
        return Err(Error::InvalidStatus);
    }

    // Cancelled raffles may still owe the prize or ticket refunds.
    if raffle.status == RaffleStatus::Cancelled {
        if raffle.prize_deposited {
            return Err(Error::InvalidStateTransition);
        }
        for ticket_id in 1..=raffle.tickets_sold {
            if !env
                .storage()
                .persistent()
                .has(&DataKey::TicketRefunded(ticket_id))
            {
                return Err(Error::InvalidStateTransition);
            }
        }
    }

    let treasury = raffle
        .treasury_address
        .clone()
        .ok_or(Error::InvalidParameters)?;

    let token_client = token::Client::new(&env, &raffle.payment_token);
    let balance = token_client.balance(&env.current_contract_address());
    let entitlement = token_entitlement(&env, &raffle, &raffle.payment_token)?;
    if balance < entitlement {
        return Err(Error::InvalidStateTransition);
    }
    let dust = balance - entitlement;
    if dust > 0 {
        let _ = token_client
            .try_transfer(&env.current_contract_address(), &treasury, &dust)
            .map_err(|_| Error::TokenTransferFailed)?;

        DustSwept {
            swept_by: admin,
            token: raffle.payment_token,
            treasury,
            amount: dust,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);
    }

    Ok(())
}

/// Wipe settlement data from a raffle that has reached a terminal state and
/// holds no payment or prize tokens.
///
/// This removes ticket records, refund markers, commit-reveal entries, owner
/// and buyer indexes, quorum randomness entries, and transient lifecycle keys.
/// It deliberately retains the raffle, factory, admin, metadata, and fairness
/// records because those keys are still needed by read and privileged paths.
/// The operation is rejected for every non-terminal status or when either
/// configured token has a non-zero balance. It emits [`StorageWiped`].
pub(crate) fn wipe_storage(env: Env) -> Result<(), Error> {
    let factory: Address = env
        .storage()
        .instance()
        .get(&DataKey::Factory)
        .ok_or(Error::NotAuthorized)?;
    factory.require_auth();
    let raffle = read_raffle(&env)?;
    if raffle.status != RaffleStatus::Cancelled
        && raffle.status != RaffleStatus::Claimed
        && raffle.status != RaffleStatus::Failed
    {
        return Err(Error::InvalidStatus);
    }

    if raffle.status == RaffleStatus::Cancelled || raffle.status == RaffleStatus::Failed {
        if raffle.prize_deposited {
            return Err(Error::InvalidStateTransition);
        }
        for ticket_id in 1..=raffle.tickets_sold {
            if !env
                .storage()
                .persistent()
                .has(&DataKey::TicketRefunded(ticket_id))
            {
                return Err(Error::InvalidStateTransition);
            }
        }
    }

    let payment_balance = token::Client::new(&env, &raffle.payment_token)
        .balance(&env.current_contract_address());
    let prize_balance = if raffle.prize_token == raffle.payment_token {
        payment_balance
    } else {
        token::Client::new(&env, &raffle.prize_token)
            .balance(&env.current_contract_address())
    };
    if payment_balance != 0 || prize_balance != 0 {
        return Err(Error::InvalidStateTransition);
    }

    for i in 1..=raffle.tickets_sold {
        env.storage().persistent().remove(&DataKey::Ticket(i));
        env.storage()
            .persistent()
            .remove(&DataKey::TicketRefunded(i));
        env.storage().persistent().remove(&DataKey::CommitEntry(i));
    }
    let buyers: soroban_sdk::Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::TicketBuyers)
        .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
    for b in buyers.iter() {
        env.storage()
            .persistent()
            .remove(&DataKey::TicketCount(b.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::OwnerTickets(b.clone()));
    }
    env.storage().persistent().remove(&DataKey::TicketBuyers);

    env.storage().instance().remove(&DataKey::Paused);
    env.storage().instance().remove(&DataKey::ReentrancyGuard);
    env.storage().instance().remove(&DataKey::AccumulatedFees);
    env.storage()
        .instance()
        .remove(&DataKey::RandomnessRequested);
    env.storage()
        .instance()
        .remove(&DataKey::RandomnessRequestLedger);
    env.storage()
        .instance()
        .remove(&DataKey::RandomnessRequestId);
    env.storage().instance().remove(&DataKey::DrawingLock);
    env.storage().instance().remove(&DataKey::FinishTime);
    env.storage().instance().remove(&DataKey::PendingAdminCancel);

    let submitted_oracles: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::QuorumSubmittedOracles)
        .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
    for oracle in submitted_oracles.iter() {
        env.storage()
            .persistent()
            .remove(&DataKey::QuorumSeed(oracle));
    }
    env.storage()
        .instance()
        .remove(&DataKey::QuorumSubmittedOracles);

    StorageWiped {
        wiped_by: factory,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);

    Ok(())
}

/// Recover a prize from a drawing that has become stuck.
///
/// The delay starts at `end_time` for raffles with a deadline. For
/// no-deadline raffles it starts at the randomness request ledger, converted
/// using the five-second ledger estimate already used by this contract.
/// Finalized raffles are never eligible: their winners may still claim.
pub(crate) fn emergency_withdraw(env: Env, caller: Address) -> Result<(), Error> {
    caller.require_auth();
    let mut raffle = read_raffle(&env)?;
    if !raffle.prize_deposited {
        return Err(Error::PrizeNotDeposited);
    }

    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotAuthorized)?;
    if caller != raffle.creator && caller != admin {
        return Err(Error::NotAuthorized);
    }

    let now = env.ledger().timestamp();
    if raffle.status != RaffleStatus::Drawing {
        return Err(Error::InvalidStatus);
    }
    if raffle.no_deadline {
        let rl: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequestLedger)
            .unwrap_or(0);
        let est = (env.ledger().sequence().saturating_sub(rl) as u64) * 5;
        if est < EMERGENCY_WITHDRAW_DELAY_SECONDS {
            return Err(Error::EmergencyTooEarly);
        }
    } else if now < raffle.end_time + EMERGENCY_WITHDRAW_DELAY_SECONDS {
        return Err(Error::EmergencyTooEarly);
    }

    let prize_token = raffle.prize_token.clone();
    let prize_balance = token::Client::new(&env, &prize_token)
        .balance(&env.current_contract_address());
    let prize_entitlement = token_entitlement(&env, &raffle, &prize_token)?;
    if prize_balance < raffle.prize_amount
        || prize_balance - raffle.prize_amount
            < prize_entitlement - raffle.prize_amount
    {
        return Err(Error::InvalidStateTransition);
    }

    raffle.prize_deposited = false;
    transition_status(
        &env,
        &mut raffle,
        RaffleStatus::Cancelled,
        env.ledger().timestamp(),
    )?;

    let tc = token::Client::new(&env, &prize_token);
    tc.transfer(
        &env.current_contract_address(),
        &raffle.creator,
        &raffle.prize_amount,
    );

    EmergencyWithdrawn {
        withdrawn_by: caller,
        to: raffle.creator.clone(),
        amount: raffle.prize_amount,
        token: prize_token,
        timestamp: now,
    }
    .publish(&env);
    Ok(())
}
