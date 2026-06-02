#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, token, xdr::ToXdr, Address, Bytes, BytesN, Env, IntoVal,
    String, Symbol, Vec,
};

mod events;
mod randomness;

use raffle_shared::{
    CancelReason, FairnessData, RaffleConfig, RaffleStatus, RandomnessSource, RandomnessType,
    Ticket,
};

use self::randomness::{OracleSeedWinnerSelection, WinnerSelectionStrategy};

use crate::events::{
    ContractPaused, ContractUnpaused, DrawTriggered, PrizeClaimed, PrizeDeposited, PrizeRefunded,
    RaffleCancelled, RaffleCreated, RaffleFinalized, RaffleStatusChanged,
    RandomnessFallbackTriggered, RandomnessReceived, RandomnessRequested, TicketPurchased,
    WinnerDrawn,
};

const ORACLE_TIMEOUT_LEDGERS: u32 = 200;
pub const MAX_DESCRIPTION_LENGTH: u32 = 1000;
pub const MAX_TICKETS_LIMIT: u32 = 100_000;
pub const MIN_TICKET_PRICE: i128 = 10_000;

#[contract]
pub struct Contract;

#[derive(Clone)]
pub struct Raffle {
    pub creator: Address,
    pub description: String,
    pub end_time: u64,
    pub max_tickets: u32,
    pub min_tickets: u32,
    pub allow_multiple: bool,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub prize_amount: i128,
    pub prizes: Vec<u32>,
    pub tickets_sold: u32,
    pub status: RaffleStatus,
    pub prize_deposited: bool,
    pub winners: Vec<Address>,
    pub claimed_winners: Vec<bool>,
    pub randomness_source: RandomnessSource,
    pub oracle_address: Option<Address>,
    pub protocol_fee_bp: u32,
    pub treasury_address: Option<Address>,
    pub swap_router: Option<Address>,
    pub tikka_token: Option<Address>,
    pub finalized_at: Option<u64>,
    pub winner_ticket_id: Option<u32>,
}

#[derive(Clone)]
pub struct FairnessMetadata {
    pub seed: u64,
    pub randomness_source: RandomnessSource,
    pub winning_ticket_indices: Vec<u32>,
    pub draw_timestamp: u64,
    pub draw_sequence: u32,
}

#[soroban_sdk::contracttype]
pub enum DataKey {
    Raffle,
    TicketCount(Address),
    Ticket(u32),
    NextTicketId,
    Factory,
    ReentrancyGuard,
    Paused,
    Admin,
    RandomnessSeed,
    RandomnessRequested,
    RandomnessRequestLedger,
    FinishTime,
    TotalTickets,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Error {
    RaffleNotFound = 1,
    RaffleInactive = 2,
    TicketsSoldOut = 3,
    InsufficientFunds = 4,
    NotAuthorized = 5,
    OracleNotSet = 6,
    RandomnessAlreadyRequested = 7,
    NoRandomnessRequest = 8,
    FallbackTooEarly = 9,
    PrizeNotDeposited = 11,
    PrizeAlreadyClaimed = 12,
    PrizeAlreadyDeposited = 13,
    NotWinner = 14,
    ClaimTooEarly = 15,
    InvalidParameters = 21,
    InvalidStatus = 22,
    ContractPaused = 23,
    InvalidStateTransition = 24,
    RaffleExpired = 25,
    InsufficientTickets = 31,
    MultipleTicketsNotAllowed = 32,
    NoTicketsSold = 33,
    TicketNotFound = 34,
    RaffleEnded = 35,
    ArithmeticOverflow = 41,
    AlreadyInitialized = 42,
    NotInitialized = 43,
    Reentrancy = 44,
    TokenTransferFailed = 45,
}

fn read_raffle(env: &Env) -> Result<Raffle, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Raffle)
        .ok_or(Error::NotInitialized)
}

fn write_raffle(env: &Env, raffle: &Raffle) {
    env.storage().instance().set(&DataKey::Raffle, raffle);
}

fn get_ticket_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::NextTicketId)
        .unwrap_or(0u32)
}

fn get_ticket_owner(env: &Env, ticket_id: u32) -> Option<Address> {
    env.storage()
        .persistent()
        .get::<_, Ticket>(&DataKey::Ticket(ticket_id))
        .map(|t| t.owner)
}

fn next_ticket_id(env: &Env) -> u32 {
    let current: u32 = env
        .storage()
        .instance()
        .get(&DataKey::NextTicketId)
        .unwrap_or(0u32);
    let next = current + 1;
    env.storage().instance().set(&DataKey::NextTicketId, &next);
    next
}

fn acquire_guard(env: &Env) -> Result<(), Error> {
    if env.storage().instance().has(&DataKey::ReentrancyGuard) {
        return Err(Error::Reentrancy);
    }
    env.storage()
        .instance()
        .set(&DataKey::ReentrancyGuard, &true);
    Ok(())
}

fn release_guard(env: &Env) {
    env.storage().instance().remove(&DataKey::ReentrancyGuard);
}

fn require_not_paused(env: &Env) -> Result<(), Error> {
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

fn build_internal_seed_u64(env: &Env) -> u64 {
    let xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.current_contract_address(),
    )
        .to_xdr(env);
    let hash: BytesN<32> = env.crypto().sha256(&xdr).into();

    let mut bytes = [0u8; 8];
    for i in 0..8 {
        bytes[i] = hash.get(i as u32).unwrap();
    }
    u64::from_be_bytes(bytes)
}

#[contractimpl]
impl Contract {
    pub fn init(
        env: Env,
        factory: Address,
        admin: Address,
        creator: Address,
        config: RaffleConfig,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Raffle) {
            return Err(Error::AlreadyInitialized);
        }

        if config.description.len() > MAX_DESCRIPTION_LENGTH {
            return Err(Error::InvalidParameters);
        }

        let now = env.ledger().timestamp();
        if config.end_time <= now && config.end_time != 0 {
            return Err(Error::InvalidParameters);
        }
        if config.max_tickets == 0 || config.max_tickets > MAX_TICKETS_LIMIT {
            return Err(Error::InvalidParameters);
        }

        if config.ticket_price < MIN_TICKET_PRICE {
            return Err(Error::InvalidParameters);
        }
        if config.prize_amount < config.ticket_price {
            return Err(Error::InvalidParameters);
        }
        if config.prizes.len() == 0 {
            return Err(Error::InvalidParameters);
        }
        let mut total_prizes_bp = 0u32;
        for prize_bp in config.prizes.iter() {
            total_prizes_bp += prize_bp;
        }
        if total_prizes_bp != 10000 {
            return Err(Error::InvalidParameters);
        }

        if config.randomness_source == RandomnessSource::External && config.oracle_address.is_none()
        {
            return Err(Error::InvalidParameters);
        }

        if config.metadata_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(Error::InvalidParameters);
        }

        let raffle = Raffle {
            creator: creator.clone(),
            description: config.description.clone(),
            end_time: config.end_time,
            max_tickets: config.max_tickets,
            min_tickets: config.min_tickets,
            allow_multiple: config.allow_multiple,
            ticket_price: config.ticket_price,
            payment_token: config.payment_token.clone(),
            prize_amount: config.prize_amount,
            prizes: config.prizes.clone(),
            tickets_sold: 0,
            status: RaffleStatus::Active,
            prize_deposited: false,
            winners: Vec::new(&env),
            claimed_winners: Vec::new(&env),
            randomness_source: config.randomness_source.clone(),
            oracle_address: config.oracle_address,
            protocol_fee_bp: config.protocol_fee_bp,
            treasury_address: config.treasury_address,
            swap_router: config.swap_router,
            tikka_token: config.tikka_token,
            finalized_at: None,
            winner_ticket_id: None,
        };
        write_raffle(&env, &raffle);
        env.storage().instance().set(&DataKey::Factory, &factory);
        env.storage().instance().set(&DataKey::Admin, &admin);

        RaffleCreated {
            creator,
            end_time: config.end_time,
            max_tickets: config.max_tickets,
            ticket_price: config.ticket_price,
            payment_token: config.payment_token,
            prize_amount: config.prize_amount,
            prizes: config.prizes,
            description: config.description,
            randomness_source: config.randomness_source,
            metadata_hash: config.metadata_hash,
        }
        .publish(&env);

        Ok(())
    }

    pub fn deposit_prize(env: Env) -> Result<(), Error> {
        require_not_paused(&env)?;
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.prize_deposited {
            return Err(Error::PrizeAlreadyDeposited);
        }

        let old_status = raffle.status.clone();
        raffle.prize_deposited = true;
        write_raffle(&env, &raffle);

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();

        token_client
            .try_transfer(&raffle.creator, &contract_address, &raffle.prize_amount)
            .map_err(|_| Error::TokenTransferFailed)?;

        PrizeDeposited {
            creator: raffle.creator.clone(),
            amount: raffle.prize_amount,
            token: raffle.payment_token.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn buy_tickets(env: Env, buyer: Address, quantity: u32) -> Result<u32, Error> {
        // Step 1: initial read + basic checks
        let mut raffle = read_raffle(&env)?;
        buyer.require_auth();
        require_not_paused(&env)?;

        if raffle.status != RaffleStatus::Active {
            return Err(Error::RaffleInactive);
        }
        if !raffle.prize_deposited {
            return Err(Error::InvalidStateTransition);
        }
        if raffle.end_time != 0 && env.ledger().timestamp() > raffle.end_time {
            return Err(Error::RaffleExpired);
        }

        // Snapshot sold and buyer count for optimistic concurrency
        let snapshot_sold = raffle.tickets_sold;
        if snapshot_sold + quantity > raffle.max_tickets {
            return Err(Error::TicketsSoldOut);
        }

        let current_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TicketCount(buyer.clone()))
            .unwrap_or(0);
        if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
            return Err(Error::MultipleTicketsNotAllowed);
        }

        // Reserve ticket id range from NextTicketId (read-only for now)
        let current_next: u32 = env
            .storage()
            .instance()
            .get(&DataKey::NextTicketId)
            .unwrap_or(0u32);
        let first_id = current_next + 1;
        let timestamp = env.ledger().timestamp();
        let total_price = raffle
            .ticket_price
            .checked_mul(quantity as i128)
            .ok_or(Error::InvalidParameters)?;

        let protocol_fee = total_price
            .checked_mul(raffle.protocol_fee_bp as i128)
            .ok_or(Error::ArithmeticOverflow)?
            / 10000;
        let net_amount = total_price - protocol_fee;

        // Prepare ticket ids locally
        let mut ticket_ids = Vec::new(&env);
        for i in 0..quantity {
            let ticket_id = first_id + i;
            ticket_ids.push_back(ticket_id);
        }

        // Step 2: re-read persisted state and verify no concurrent changes
        let persisted_raffle = read_raffle(&env)?;
        let persisted_sold = persisted_raffle.tickets_sold;
        let persisted_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TicketCount(buyer.clone()))
            .unwrap_or(0);

        if persisted_sold != snapshot_sold || persisted_count != current_count {
            return Err(Error::InvalidStateTransition);
        }

        // Final availability check against persisted values
        if persisted_sold + quantity > persisted_raffle.max_tickets {
            return Err(Error::TicketsSoldOut);
        }

        // Step 3: commit all changes atomically within this invocation
        for i in 0..quantity {
            let ticket_id = first_id + i;
            let ticket_number = snapshot_sold + i + 1;
            let ticket = Ticket {
                id: ticket_id,
                owner: buyer.clone(),
                purchase_time: timestamp,
                ticket_number,
            };
            env.storage()
                .persistent()
                .set(&DataKey::Ticket(ticket_id), &ticket);
        }

        // Advance NextTicketId
        let new_next = current_next + quantity;
        env.storage()
            .instance()
            .set(&DataKey::NextTicketId, &new_next);

        // Update ticket count and raffle sold
        env.storage().persistent().set(
            &DataKey::TicketCount(buyer.clone()),
            &(current_count + quantity),
        );
        raffle.tickets_sold = snapshot_sold + quantity;

        if raffle.tickets_sold >= raffle.max_tickets {
            let old_status = raffle.status.clone();
            raffle.status = RaffleStatus::Drawing;
            RaffleStatusChanged {
                old_status,
                new_status: RaffleStatus::Drawing,
                timestamp,
            }
            .publish(&env);
        }

        write_raffle(&env, &raffle);

        if let Some(factory_address) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Factory)
        {
            env.invoke_contract::<()>(
                &factory_address,
                &Symbol::new(&env, "record_volume"),
                (raffle.payment_token.clone(), total_price).into_val(&env),
            );
            env.invoke_contract::<()>(
                &factory_address,
                &Symbol::new(&env, "track_participant"),
                (buyer.clone(),).into_val(&env),
            );
        }

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client
            .try_transfer(&buyer, &env.current_contract_address(), &total_price)
            .map_err(|_| Error::TokenTransferFailed)?;

        if protocol_fee > 0 {
            if let Some(treasury) = &raffle.treasury_address {
                token_client.transfer(&env.current_contract_address(), treasury, &protocol_fee);
            }
        }

        TicketPurchased {
            buyer,
            ticket_ids,
            quantity,
            ticket_price: raffle.ticket_price,
            total_paid: total_price,
            protocol_fee,
            timestamp,
        }
        .publish(&env);

        Ok(raffle.tickets_sold)
    }

    pub fn finalize_raffle(env: Env) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.status != RaffleStatus::Active && raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStatus);
        }

        let now = env.ledger().timestamp();
        let time_ended = raffle.end_time != 0 && now >= raffle.end_time;
        let tickets_full = raffle.tickets_sold >= raffle.max_tickets;

        if raffle.status == RaffleStatus::Active && !time_ended && !tickets_full {
            return Err(Error::InvalidStateTransition);
        }

        if raffle.tickets_sold < raffle.min_tickets {
            let old_status = raffle.status.clone();
            raffle.status = RaffleStatus::Failed;
            write_raffle(&env, &raffle);

            RaffleStatusChanged {
                old_status,
                new_status: RaffleStatus::Failed,
                timestamp: now,
            }
            .publish(&env);
            return Ok(());
        }

        if raffle.randomness_source == RandomnessSource::External {
            let already: bool = env
                .storage()
                .instance()
                .get(&DataKey::RandomnessRequested)
                .unwrap_or(false);
            if already {
                return Err(Error::RandomnessAlreadyRequested);
            }
            env.storage()
                .instance()
                .set(&DataKey::RandomnessRequested, &true);
            env.storage()
                .instance()
                .set(&DataKey::RandomnessRequestLedger, &env.ledger().sequence());

            RandomnessRequested {
                oracle: raffle
                    .oracle_address
                    .clone()
                    .unwrap_or(env.current_contract_address()),
                timestamp: now,
            }
            .publish(&env);
            return Ok(());
        }

        let seed = build_internal_seed_u64(&env);
        self::do_finalize_with_seed(&env, raffle, seed, RandomnessType::Prng)
    }

    pub fn provide_randomness(
        env: Env,
        random_seed: u64,
        public_key: BytesN<32>,
        proof: BytesN<64>,
    ) -> Result<Address, Error> {
        let mut raffle = read_raffle(&env)?;

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

        let request_pending: bool = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequested)
            .unwrap_or(false);
        if !request_pending {
            return Err(Error::NoRandomnessRequest);
        }

        let message = Bytes::from_array(&env, &random_seed.to_be_bytes());
        env.crypto().ed25519_verify(&public_key, &message, &proof);

        RandomnessReceived {
            oracle,
            seed: random_seed,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        self::do_finalize_with_seed(&env, raffle, random_seed, RandomnessType::Vrf)?;
        Ok(env.current_contract_address())
    }

    pub fn trigger_randomness_fallback(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let raffle = read_raffle(&env)?;

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

        let request_pending: bool = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequested)
            .unwrap_or(false);
        if !request_pending {
            return Err(Error::NoRandomnessRequest);
        }

        let request_ledger: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequestLedger)
            .unwrap_or(0);
        if env.ledger().sequence() < request_ledger + ORACLE_TIMEOUT_LEDGERS {
            return Err(Error::FallbackTooEarly);
        }

        let seed = build_internal_seed_u64(&env);

        RandomnessFallbackTriggered {
            triggered_by: caller,
            seed_used: seed,
            request_ledger,
            fallback_ledger: env.ledger().sequence(),
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        self::do_finalize_with_seed(&env, raffle, seed, RandomnessType::Fallback)
    }

    pub fn claim_prize(env: Env, winner: Address, tier_index: u32) -> Result<i128, Error> {
        winner.require_auth();
        acquire_guard(&env)?;
        let mut raffle = read_raffle(&env)?;

        if raffle.status != RaffleStatus::Finalized {
            return Err(Error::InvalidStatus);
        }

        if tier_index as usize >= raffle.winners.len() {
            return Err(Error::InvalidParameters);
        }

        if raffle.winners.get(tier_index).unwrap() != winner {
            return Err(Error::NotWinner);
        }

        if raffle.claimed_winners.get(tier_index).unwrap() {
            return Err(Error::PrizeAlreadyClaimed);
        }

        let prize_bp = raffle.prizes.get(tier_index).unwrap();
        let amount = raffle
            .prize_amount
            .checked_mul(prize_bp as i128)
            .ok_or(Error::ArithmeticOverflow)?
            / 10000;

        let fee = amount * (raffle.protocol_fee_bp as i128) / 10000;
        let net_amount = amount - fee;

        raffle.claimed_winners.set(tier_index, true);

        let mut all_claimed = true;
        for claimed in raffle.claimed_winners.iter() {
            if !claimed {
                all_claimed = false;
                break;
            }
        }
        if all_claimed {
            raffle.status = RaffleStatus::Claimed;
        }
        write_raffle(&env, &raffle);

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client.transfer(&env.current_contract_address(), &winner, &net_amount);

        if fee > 0 {
            if let Some(treasury) = &raffle.treasury_address {
                token_client.transfer(&env.current_contract_address(), treasury, &fee);
            }
        }

        PrizeClaimed {
            winner,
            tier_index,
            payment_token: raffle.payment_token.clone(),
            gross_amount: amount,
            net_amount,
            platform_fee: fee,
            claimed_at: env.ledger().timestamp(),
        }
        .publish(&env);

        release_guard(&env);
        Ok(net_amount)
    }

    pub fn cancel_raffle(env: Env, reason: CancelReason) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;

        if reason == CancelReason::AdminCancelled {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .ok_or(Error::NotAuthorized)?;
            admin.require_auth();
        } else {
            raffle.creator.require_auth();
        }

        if raffle.status == RaffleStatus::Finalized
            || raffle.status == RaffleStatus::Cancelled
            || raffle.status == RaffleStatus::Claimed
        {
            return Err(Error::InvalidStatus);
        }

        let old_status = raffle.status.clone();
        raffle.status = RaffleStatus::Cancelled;
        write_raffle(&env, &raffle);

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

    pub fn refund_prize(env: Env) -> Result<(), Error> {
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

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client.transfer(
            &env.current_contract_address(),
            &raffle.creator,
            &raffle.prize_amount,
        );

        PrizeRefunded {
            creator: raffle.creator.clone(),
            amount: raffle.prize_amount,
            token: raffle.payment_token.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn refund_ticket(env: Env, ticket_id: u32) -> Result<i128, Error> {
        acquire_guard(&env)?;
        let raffle = read_raffle(&env)?;

        if raffle.status != RaffleStatus::Cancelled && raffle.status != RaffleStatus::Failed {
            return Err(Error::InvalidStatus);
        }

        let ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .ok_or(Error::TicketNotFound)?;
        ticket.owner.require_auth();

        // Check if already refunded
        let refund_key = (DataKey::Ticket(ticket_id), Symbol::new(&env, "refunded"));
        if env.storage().persistent().has(&refund_key) {
            return Err(Error::InvalidStatus); // Already refunded
        }

        env.storage().persistent().set(&refund_key, &true);

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client.transfer(
            &env.current_contract_address(),
            &ticket.owner,
            &raffle.ticket_price,
        );

        crate::events::TicketRefunded {
            buyer: ticket.owner,
            ticket_number: ticket.ticket_number,
            amount: raffle.ticket_price,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        release_guard(&env);
        Ok(raffle.ticket_price)
    }

    pub fn get_raffle(env: Env) -> Result<Raffle, Error> {
        read_raffle(&env)
    }

    pub fn get_fairness_data(env: Env) -> Result<FairnessData, Error> {
        let metadata: FairnessMetadata = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessSeed)
            .ok_or(Error::InvalidStatus)?;
        let raffle = read_raffle(&env)?;

        let mut ticket_ids = Vec::new(&env);
        let count = get_ticket_count(&env);
        for i in 1..=count {
            ticket_ids.push_back(i);
        }

        Ok(FairnessData {
            seed: metadata.seed,
            randomness_source: metadata.randomness_source,
            ticket_ids,
            winning_ticket_indices: metadata.winning_ticket_indices,
            draw_timestamp: metadata.draw_timestamp,
            draw_sequence: metadata.draw_sequence,
        })
    }

    pub fn wipe_storage(env: Env) -> Result<(), Error> {
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

        // Wipe all storage
        env.storage().instance().remove(&DataKey::Raffle);
        // ... (other keys)
        Ok(())
    }

    pub fn pause(env: Env) -> Result<(), Error> {
        let factory: Address = env
            .storage()
            .instance()
            .get(&DataKey::Factory)
            .ok_or(Error::NotAuthorized)?;
        factory.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);

        ContractPaused {
            paused_by: factory,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), Error> {
        let factory: Address = env
            .storage()
            .instance()
            .get(&DataKey::Factory)
            .ok_or(Error::NotAuthorized)?;
        factory.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);

        ContractUnpaused {
            unpaused_by: factory,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotAuthorized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }
}

fn do_finalize_with_seed(
    env: &Env,
    mut raffle: Raffle,
    seed: u64,
    randomness_type: RandomnessType,
) -> Result<(), Error> {
    let total_tickets = get_ticket_count(env);
    if total_tickets == 0 {
        return Err(Error::NoTicketsSold);
    }

    let selector = OracleSeedWinnerSelection::new(seed);
    let winning_ticket_ids =
        selector.select_winner_indices(env, total_tickets, raffle.prizes.len() as u32);
    let mut winners = Vec::new(env);

    for i in 0..winning_ticket_ids.len() {
        let winner_index = winning_ticket_ids.get(i).unwrap();
        let ticket_id = winner_index + 1;
        let winner = get_ticket_owner(env, ticket_id).ok_or(Error::TicketNotFound)?;
        winners.push_back(winner.clone());

        WinnerDrawn {
            winner,
            ticket_id: winner_index,
            tier_index: i,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);
    }

    let mut claimed_winners = Vec::new(env);
    for _ in 0..raffle.prizes.len() {
        claimed_winners.push_back(false);
    }

    let fairness_metadata = FairnessMetadata {
        seed,
        randomness_source: raffle.randomness_source.clone(),
        winning_ticket_indices: winning_ticket_ids.clone(),
        draw_timestamp: env.ledger().timestamp(),
        draw_sequence: env.ledger().sequence(),
    };
    env.storage()
        .instance()
        .set(&DataKey::RandomnessSeed, &fairness_metadata);

    raffle.status = RaffleStatus::Finalized;
    raffle.winners = winners.clone();
    raffle.claimed_winners = claimed_winners;
    raffle.finalized_at = Some(env.ledger().timestamp());
    write_raffle(env, &raffle);

    RaffleFinalized {
        winners,
        winning_ticket_ids,
        total_tickets_sold: raffle.tickets_sold,
        randomness_source: raffle.randomness_source.clone(),
        randomness_type,
        finalized_at: env.ledger().timestamp(),
    }
    .publish(&env);

    Ok(())
}
