use raffle_shared::constants::MAX_SWEEP_UNCLAIMED_PER_CALL;
use soroban_sdk::{token, Address, Env};

use crate::events::{PrizeClaimed, PrizeRefunded, PrizeSwept, TicketRefunded};
use crate::{
    calculate_tier_prize, read_raffle, transition_status, write_raffle, DataKey, Error, Guard,
    RaffleStatus, Winner,
};

pub(crate) fn claim_prize(env: Env, winner: Address, tier_index: u32) -> Result<i128, Error> {
    winner.require_auth();
    let _guard = Guard::new(&env)?;
    let mut raffle = read_raffle(&env)?;

    if raffle.status != RaffleStatus::Finalized {
        return Err(Error::InvalidStatus);
    }
    if let Some(fa) = raffle.finalized_at {
        if env.ledger().timestamp() < fa + raffle.claim_lockup_seconds {
            return Err(Error::ClaimTooEarly);
        }
    }
    if tier_index >= raffle.winners.len() {
        return Err(Error::InvalidParameters);
    }

    let entry = raffle.winners.get(tier_index).ok_or(Error::InvalidIndex)?;
    if entry.address != winner {
        return Err(Error::NotWinner);
    }
    if entry.claimed {
        return Err(Error::PrizeAlreadyClaimed);
    }

    let amount = calculate_tier_prize(&raffle, tier_index)?;
    if amount <= 0 {
        return Err(Error::ZeroPrize);
    }

    let protocol_fee = amount
        .checked_mul(raffle.protocol_fee_bp as i128)
        .ok_or(Error::ArithmeticOverflow)?
        .checked_add(9999)
        .ok_or(Error::ArithmeticOverflow)?
        / 10000;

    let net_amount = amount
        .checked_sub(protocol_fee)
        .ok_or(Error::ArithmeticOverflow)?;
    let tc = token::Client::new(&env, &raffle.prize_token);
    let balance = tc.balance(&env.current_contract_address());
    if balance < amount {
        return Err(Error::InsufficientFunds);
    }

    raffle.winners.set(
        tier_index,
        Winner {
            claimed: true,
            ..entry
        },
    );

    let all_claimed = raffle.winners.iter().all(|entry| entry.claimed);
    if all_claimed {
        transition_status(
            &env,
            &mut raffle,
            RaffleStatus::Claimed,
            env.ledger().timestamp(),
        )?;
    }
    write_raffle(&env, &raffle);

    if net_amount > 0 {
        let _ = tc
            .try_transfer(&env.current_contract_address(), &winner, &net_amount)
            .map_err(|_| Error::TokenTransferFailed)?;
    }

    if protocol_fee > 0 {
        if let Some(treasury) = &raffle.treasury_address {
            tc.transfer(&env.current_contract_address(), treasury, &protocol_fee);
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

    PrizeClaimed {
        winner,
        tier_index,
        payment_token: raffle.prize_token.clone(),
        gross_amount: amount,
        net_amount,
        platform_fee: protocol_fee,
        claimed_at: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(amount)
}

/// Permissionless sweep of unclaimed prizes to treasury after `claim_expiry_seconds`
/// has elapsed since finalization.  Marks each swept winner as claimed and emits
/// a `PrizeSwept` event per winner.  Transitions raffle to `Claimed` when all
/// prizes are accounted for (claimed + swept).
pub(crate) fn sweep_unclaimed(
    env: Env,
    start_index: u32,
    limit: u32,
) -> Result<u32, Error> {
    let _guard = Guard::new(&env)?;
    let mut raffle = read_raffle(&env)?;

    if raffle.status != RaffleStatus::Finalized {
        return Err(Error::InvalidStatus);
    }
    if raffle.claim_expiry_seconds == 0 {
        return Err(Error::InvalidStateTransition);
    }

    let fa = raffle.finalized_at.ok_or(Error::InvalidStatus)?;
    let now = env.ledger().timestamp();
    if now < fa + raffle.claim_expiry_seconds {
        return Err(Error::ClaimTooEarly);
    }

    let treasury = raffle.treasury_address.clone().ok_or(Error::NotAuthorized)?;
    let tc = token::Client::new(&env, &raffle.prize_token);
    let mut swept: u32 = 0;

    let len = raffle.winners.len();
    if start_index >= len {
        return Ok(0);
    }

    let max_items = if limit == 0 {
        MAX_SWEEP_UNCLAIMED_PER_CALL
    } else {
        limit.min(MAX_SWEEP_UNCLAIMED_PER_CALL)
    };
    let end_index = start_index.saturating_add(max_items).min(len);

    for i in start_index..end_index {
        let entry = raffle.winners.get(i).ok_or(Error::InvalidIndex)?;
        if entry.claimed {
            continue;
        }
        let amount = calculate_tier_prize(&raffle, i)?;
        if amount <= 0 {
            continue;
        }
        let _ = tc
            .try_transfer(&env.current_contract_address(), &treasury, &amount)
            .map_err(|_| Error::TokenTransferFailed)?;
        raffle.winners.set(
            i,
            Winner {
                claimed: true,
                ..entry.clone()
            },
        );
        PrizeSwept {
            winner: entry.address,
            tier_index: i,
            treasury: treasury.clone(),
            amount,
            swept_at: now,
        }
        .publish(&env);
        swept += 1;
    }

    let all_claimed = raffle.winners.iter().all(|entry| entry.claimed);
    if all_claimed {
        transition_status(&env, &mut raffle, RaffleStatus::Claimed, now)?;
    }
    write_raffle(&env, &raffle);
    Ok(swept)
}

pub(crate) fn refund_prize(env: Env) -> Result<(), Error> {
    let mut raffle = read_raffle(&env)?;
    raffle.creator.require_auth();

    if raffle.status != RaffleStatus::Cancelled && raffle.status != RaffleStatus::Failed {
        return Err(Error::InvalidStatus);
    }
    if !raffle.prize_deposited {
        return Err(Error::PrizeNotDeposited);
    }

    raffle.prize_deposited = false;
    write_raffle(&env, &raffle);

    let token_client = token::Client::new(&env, &raffle.prize_token);
    token_client
        .try_transfer(
            &env.current_contract_address(),
            &raffle.creator,
            &raffle.prize_amount,
        )
        .map_err(|_| Error::TokenTransferFailed)?;
    PrizeRefunded {
        creator: raffle.creator.clone(),
        amount: raffle.prize_amount,
        token: raffle.prize_token.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(())
}

pub(crate) fn refund_ticket(env: Env, caller: Address, ticket_id: u32) -> Result<i128, Error> {
    let raffle = read_raffle(&env)?;
    if raffle.status != RaffleStatus::Cancelled && raffle.status != RaffleStatus::Failed {
        return Err(Error::InvalidStatus);
    }

    let _guard = Guard::new(&env)?;
    let ticket: crate::Ticket = env
        .storage()
        .persistent()
        .get(&DataKey::Ticket(ticket_id))
        .ok_or(Error::TicketNotFound)?;
    ticket.payer.require_auth();

    if env
        .storage()
        .persistent()
        .has(&DataKey::TicketRefunded(ticket_id))
    {
        return Err(Error::PrizeAlreadyClaimed);
    }
    env.storage()
        .persistent()
        .set(&DataKey::TicketRefunded(ticket_id), &true);

    let token_client = token::Client::new(&env, &raffle.payment_token);
    token_client.try_transfer(&env.current_contract_address(), &ticket.owner, &ticket.price_paid).map_err(|_| Error::TokenTransferFailed)?;
    TicketRefunded { buyer: ticket.owner, ticket_number: ticket.ticket_number, amount: ticket.price_paid, timestamp: env.ledger().timestamp() }.publish(&env);
    Ok(ticket.price_paid)
    token_client
        .try_transfer(
            &env.current_contract_address(),
            &ticket.owner,
            &raffle.ticket_price,
        )
        .map_err(|_| Error::TokenTransferFailed)?;
    TicketRefunded {
        buyer: ticket.owner,
        ticket_number: ticket.ticket_number,
        amount: raffle.ticket_price,
        timestamp: env.ledger().timestamp(),
    }
    .publish(&env);
    Ok(raffle.ticket_price)
}

pub(crate) fn batch_refund_tickets(
    env: Env,
    caller: Address,
    ticket_ids: soroban_sdk::Vec<u32>,
) -> Result<i128, Error> {
    let raffle = read_raffle(&env)?;
    if raffle.status != RaffleStatus::Cancelled && raffle.status != RaffleStatus::Failed { return Err(Error::InvalidStatus); }

    let _guard = Guard::new(&env)?;
    caller.require_auth();
    
    let token_client = token::Client::new(&env, &raffle.payment_token);
    let mut total_refunded: i128 = 0;

    for ticket_id in ticket_ids.iter() {
        let ticket: crate::Ticket = env.storage().persistent().get(&DataKey::Ticket(ticket_id)).ok_or(Error::TicketNotFound)?;
        
        if caller != ticket.payer && caller != ticket.owner {
            return Err(Error::NotAuthorized);
        }

        if !env.storage().persistent().has(&DataKey::TicketRefunded(ticket_id)) {
            env.storage().persistent().set(&DataKey::TicketRefunded(ticket_id), &true);
            
            token_client.try_transfer(&env.current_contract_address(), &ticket.payer, &raffle.ticket_price).map_err(|_| Error::TokenTransferFailed)?;
            
            TicketRefunded { payer: ticket.payer, owner: ticket.owner, ticket_number: ticket.ticket_number, amount: raffle.ticket_price, timestamp: env.ledger().timestamp() }.publish(&env);
            
            total_refunded = total_refunded.checked_add(raffle.ticket_price).ok_or(Error::ArithmeticOverflow)?;
        }
    }

    Ok(total_refunded)
}

pub(crate) fn batch_refund_tickets(
    env: Env,
    owner: Address,
    ticket_ids: Vec<u32>,
) -> Result<i128, Error> {
    owner.require_auth();
    let raffle = read_raffle(&env)?;
    
    if raffle.status != RaffleStatus::Cancelled && raffle.status != RaffleStatus::Failed {
        return Err(Error::InvalidStatus);
    }
    
    let _guard = Guard::new(&env)?;
    let mut total_refunded = 0i128;
    let token_client = token::Client::new(&env, &raffle.payment_token);
    
    for ticket_id in ticket_ids.iter() {
        let ticket: crate::Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .ok_or(Error::TicketNotFound)?;
        
        // Check that the ticket is owned by the caller
        if ticket.owner != owner {
            return Err(Error::NotAuthorized);
        }
        
        // Skip if already refunded
        if env.storage().persistent().has(&DataKey::TicketRefunded(ticket_id)) {
            continue;
        }
        
        env.storage()
            .persistent()
            .set(&DataKey::TicketRefunded(ticket_id), &true);
        
        token_client
            .try_transfer(
                &env.current_contract_address(),
                &ticket.owner,
                &raffle.ticket_price,
            )
            .map_err(|_| Error::TokenTransferFailed)?;
        
        TicketRefunded {
            buyer: ticket.owner.clone(),
            ticket_number: ticket.ticket_number,
            amount: raffle.ticket_price,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);
        
        total_refunded = total_refunded
            .checked_add(raffle.ticket_price)
            .ok_or(Error::ArithmeticOverflow)?;
    }
    
    Ok(total_refunded)
}
