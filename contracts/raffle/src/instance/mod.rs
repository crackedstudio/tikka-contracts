// Instance submodule
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env, IntoVal, String,
    Symbol, Vec,
};

use crate::types::{effective_limit, PageResult_Tickets, PaginationParams};

use crate::events::{
    DrawTriggered, PrizeClaimed, PrizeDeposited, RaffleCancelled, RaffleCreated, RaffleFinalized,
    RandomnessReceived, RandomnessRequested, StatusChanged, TicketPurchased,
};


// Define a trait for Soroswap Router
#[soroban_sdk::contractclient(name = "SoroswapRouterClient")]
pub trait SoroswapRouter {

// --- External Contract Traits ---
#[soroban_sdk::contractclient(name = "SoroswapRouterClient")]
pub trait SoroswapRouterTrait {

    fn swap_exact_tokens_for_tokens(
        env: Env,
        amount_in: i128,
        amount_out_min: i128,
        path: Vec<Address>,
        to: Address,
        deadline: u64,
    ) -> i128;
}

#[contract]
pub struct Contract;

#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RaffleStatus {
    Proposed = 0,
    Active = 1,
    Drawing = 2,
    Finalized = 3,
    Claimed = 4,
    Cancelled = 5,
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum CancelReason {
    CreatorCancelled = 0,
    AdminCancelled = 1,
    OracleTimeout = 2,
    MinTicketsNotMet = 3,
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RandomnessSource {
    Internal = 0,
    External = 1,
}

#[derive(Clone)]
#[contracttype]
pub struct Raffle {
    pub creator: Address,
    pub description: String,
    pub end_time: u64,
    pub max_tickets: u32,
    pub allow_multiple: bool,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub prize_amount: i128,
    pub prizes: Vec<u32>, // Basis points for each tier (e.g., [5000, 3000, 2000])
    pub tickets_sold: u32,
    pub status: RaffleStatus,
    pub prize_deposited: bool,
    pub winners: Vec<Address>,
    pub claimed_winners: Vec<bool>, // Track which tier has been claimed
    pub randomness_source: RandomnessSource,
    pub oracle_address: Option<Address>,
    pub protocol_fee_bp: u32,
    pub treasury_address: Option<Address>,
    pub swap_router: Option<Address>,
    pub tikka_token: Option<Address>,
    pub finalized_at: Option<u64>,
    pub swap_router: Option<Address>,
    pub tikka_token: Option<Address>,
    pub winner_ticket_id: Option<u32>,
}

#[derive(Clone)]
#[contracttype]
pub struct RaffleConfig {
    pub description: String,
    pub end_time: u64,
    pub max_tickets: u32,
    pub allow_multiple: bool,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub prize_amount: i128,
    pub prizes: Vec<u32>, // Basis points for each tier
    pub randomness_source: RandomnessSource,
    pub oracle_address: Option<Address>,
    pub protocol_fee_bp: u32,
    pub treasury_address: Option<Address>,
    pub swap_router: Option<Address>,
    pub tikka_token: Option<Address>,
}

#[derive(Clone)]
#[contracttype]
pub struct Ticket {
    pub id: u32,
    pub owner: Address,
    pub purchase_time: u64,
    pub ticket_number: u32,
}

// Helper function to publish events with standardized topics
fn publish_event<T>(env: &Env, event_name: &str, event: T)
where
    T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.events().publish(
        (Symbol::new(env, "tikka"), Symbol::new(env, event_name)),
        event,
    );
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Raffle,
    Tickets,
    TicketCount(Address),
    Ticket(u32),
    NextTicketId,
    Factory,
    RefundStatus(u32), // ticket_id -> bool
    ReentrancyGuard,
    Approved(u32), // ticket_id -> Address
    ApprovedForAll(Address, Address), // (owner, operator) -> bool
    Paused,
    Admin,
}

// --- Error Types ---

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Error {
    // General errors (1-10)
    RaffleNotFound = 1,
    RaffleInactive = 2,
    TicketsSoldOut = 3,
    InsufficientFunds = 4,
    NotAuthorized = 5,
    
    // Prize/Claim errors (11-20)
    PrizeNotDeposited = 11,
    PrizeAlreadyClaimed = 12,
    PrizeAlreadyDeposited = 13,
    NotWinner = 14,
    ClaimTooEarly = 15,
    
    // State/Validation errors (21-30)
    InvalidParameters = 21,
    InvalidStatus = 22,
    ContractPaused = 23,
    InvalidStateTransition = 24,
    RaffleExpired = 25,
    
    // Ticket errors (31-40)
    InsufficientTickets = 31,
    MultipleTicketsNotAllowed = 32,
    NoTicketsSold = 33,
    TicketNotFound = 34,
    
    // System errors (41-50)
    ArithmeticOverflow = 41,
    AlreadyInitialized = 42,
    NotInitialized = 43,
    Reentrancy = 44,
    // Admin errors (51-60)
    AdminTransferPending = 51,
    NoPendingTransfer = 52,
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

fn require_not_paused(env: &Env) -> Result<(), Error> {
    if Contract::is_paused(env.clone()) {
        return Err(Error::ContractPaused);
    }
    Ok(())
}

fn read_tickets(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Tickets)
        .unwrap_or_else(|| Vec::new(env))
}

fn write_tickets(env: &Env, tickets: &Vec<Ticket>) {
    env.storage().instance().set(&DataKey::Tickets, tickets);
}

fn read_ticket_count(env: &Env, buyer: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::TicketCount(buyer.clone()))
        .unwrap_or(0)
}

fn write_ticket_count(env: &Env, buyer: &Address, count: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::TicketCount(buyer.clone()), &count);
}

fn next_ticket_id(env: &Env) -> u32 {
    let current = env
        .storage()
        .instance()
        .get(&DataKey::NextTicketId)
        .unwrap_or(0u32);
    let next = current + 1;
    env.storage().instance().set(&DataKey::NextTicketId, &next);
    next
}

fn write_ticket(env: &Env, ticket: &Ticket) {
    env.storage()
        .persistent()
        .set(&DataKey::Ticket(ticket.id), ticket);
}

fn require_not_paused(env: &Env) -> Result<(), Error> {
    if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {
        return Err(Error::ContractPaused);
    }
    Ok(())
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

    if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {

        return Err(Error::ContractPaused);
    }
    Ok(())
}

fn do_transfer(env: &Env, from: Address, to: Address, token_id: u32) -> Result<(), Error> {
    let mut ticket = env
        .storage()
        .persistent()
        .get::<_, Ticket>(&DataKey::Ticket(token_id))
        .ok_or(Error::InvalidParameters)?;

    if ticket.owner != from {
        return Err(Error::NotAuthorized);
    }

    let raffle = read_raffle(env)?;
    let to_count = read_ticket_count(env, &to);
    if !raffle.allow_multiple && to_count > 0 {
        return Err(Error::MultipleTicketsNotAllowed);
    }

    let from_count = read_ticket_count(env, &from);
    write_ticket_count(env, &from, from_count.saturating_sub(1));
    write_ticket_count(env, &to, to_count + 1);

    ticket.owner = to.clone();
    write_ticket(env, &ticket);

    let mut all_tickets = read_tickets(env);
    let index = ticket.ticket_number.saturating_sub(1) as u32;
    let mut old_ticket = all_tickets.get(index).unwrap();
    old_ticket.owner = to.clone();
    all_tickets.set(index, old_ticket);
    write_tickets(env, &all_tickets);

    env.storage().persistent().remove(&DataKey::Approved(token_id));

    Ok(())
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

        let now = env.ledger().timestamp();
        if config.end_time < now && config.end_time != 0 {
            return Err(Error::InvalidParameters);
        }
        if config.max_tickets == 0 {
            return Err(Error::InvalidParameters);
        }
        if config.ticket_price <= 0 {
            return Err(Error::InvalidParameters);
        }
        if config.prize_amount <= 0 {
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

        let raffle = Raffle {
            creator: creator.clone(),
            description: config.description.clone(),
            end_time: config.end_time,
            max_tickets: config.max_tickets,
            allow_multiple: config.allow_multiple,
            ticket_price: config.ticket_price,
            payment_token: config.payment_token.clone(),
            prize_amount: config.prize_amount,
            prizes: config.prizes.clone(),
            tickets_sold: 0,
            status: RaffleStatus::Proposed,
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
            swap_router: config.swap_router,
            tikka_token: config.tikka_token,
            winner_ticket_id: None,
        };
        write_raffle(&env, &raffle);
        env.storage().instance().set(&DataKey::Factory, &factory);
        env.storage().instance().set(&DataKey::Admin, &admin);

        publish_event(
            &env,
            "raffle_created",
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
            },
        );

        Ok(())
    }

    pub fn deposit_prize(env: Env) -> Result<(), Error> {
        require_not_paused(&env)?;
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.status != RaffleStatus::Proposed {
            return Err(Error::InvalidStateTransition);
        }
        if raffle.prize_deposited {
            return Err(Error::PrizeAlreadyDeposited);
        }

        // Effects: update state BEFORE external call (CEI pattern)
        raffle.prize_deposited = true;
        raffle.status = RaffleStatus::Active;
        write_raffle(&env, &raffle);

        // Interaction: external token transfer
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&raffle.creator, &contract_address, &raffle.prize_amount);

        publish_event(
            &env,
            "prize_deposited",
            PrizeDeposited {
                creator: raffle.creator.clone(),
                amount: raffle.prize_amount,
                token: raffle.payment_token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "status_changed",
            StatusChanged {
                old_status: RaffleStatus::Proposed,
                new_status: RaffleStatus::Active,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn buy_ticket(env: Env, buyer: Address) -> Result<u32, Error> {
        require_not_paused(&env)?;
        buyer.require_auth();
        let mut raffle = read_raffle(&env)?;

        if raffle.status != RaffleStatus::Active {
            return Err(Error::RaffleInactive);
        }
        if raffle.end_time != 0 && env.ledger().timestamp() > raffle.end_time {
            return Err(Error::RaffleEnded);
        }
        if raffle.tickets_sold >= raffle.max_tickets {
            return Err(Error::TicketsSoldOut);
        }

        let current_count = read_ticket_count(&env, &buyer);
        if !raffle.allow_multiple && current_count > 0 {
            return Err(Error::MultipleTicketsNotAllowed);
        }

        // Effects: update ALL state BEFORE external call (CEI pattern)
        let ticket_id = next_ticket_id(&env);
        let timestamp = env.ledger().timestamp();

        let ticket = Ticket {
            id: ticket_id,
            owner: buyer.clone(),
            purchase_time: timestamp,
            ticket_number: raffle.tickets_sold + 1,
        };
        write_ticket(&env, &ticket);

        let mut tickets = read_tickets(&env);
        tickets.push_back(ticket);
        write_tickets(&env, &tickets);

        raffle.tickets_sold += 1;

        if raffle.tickets_sold >= raffle.max_tickets {
            raffle.status = RaffleStatus::Drawing;
            publish_event(
                &env,
                "status_changed",
                StatusChanged {
                    old_status: RaffleStatus::Active,
                    new_status: RaffleStatus::Drawing,
                    timestamp: env.ledger().timestamp(),
                },
            );
        }

        write_ticket_count(&env, &buyer, current_count + 1);
        write_raffle(&env, &raffle);

        // Update global volume in factory
        if let Some(factory_address) = env.storage().instance().get::<_, Address>(&DataKey::Factory) {
            env.invoke_contract::<()>(
                &factory_address,
                &Symbol::new(&env, "record_volume"),
                (raffle.payment_token.clone(), raffle.ticket_price).into_val(&env),
            );
        }

        // Interaction: external token transfer
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&buyer, &contract_address, &raffle.ticket_price);

        let mut ticket_ids = Vec::new(&env);
        ticket_ids.push_back(ticket_id);

        publish_event(
            &env,
            "ticket_purchased",
            TicketPurchased {
                buyer,
                ticket_ids,
                quantity: 1u32,
                total_paid: raffle.ticket_price,
                timestamp,
            },
        );

        Ok(raffle.tickets_sold)
    }

    pub fn finalize_raffle(env: Env) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.status == RaffleStatus::Active {
            if (raffle.end_time != 0 && env.ledger().timestamp() >= raffle.end_time)
                || raffle.tickets_sold >= raffle.max_tickets
            {
                raffle.status = RaffleStatus::Drawing;
                publish_event(
                    &env,
                    "status_changed",
                    StatusChanged {
                        old_status: RaffleStatus::Active,
                        new_status: RaffleStatus::Drawing,
                        timestamp: env.ledger().timestamp(),
                    },
                );
            }
        }

        if raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStateTransition);
        }

        if raffle.tickets_sold == 0 {
            return Err(Error::NoTicketsSold);
        }

        publish_event(
            &env,
            "draw_triggered",
            DrawTriggered {
                triggered_by: raffle.creator.clone(),
                total_tickets_sold: raffle.tickets_sold,
                timestamp: env.ledger().timestamp(),
            },
        );

        if raffle.randomness_source == RandomnessSource::External {
            let oracle = raffle
                .oracle_address
                .as_ref()
                .ok_or(Error::InvalidParameters)?
                .clone();
            publish_event(
                &env,
                "randomness_requested",
                RandomnessRequested {
                    oracle,
                    timestamp: env.ledger().timestamp(),
                },
            );
            return Ok(());
        }

        let tickets = read_tickets(&env);
        let mut winners = Vec::new(&env);
        let mut winning_ticket_ids = Vec::new(&env);
        let mut current_seed = env.ledger().timestamp() + env.ledger().sequence() as u64;

        for _ in 0..raffle.prizes.len() {
            let winner_index = (current_seed % tickets.len() as u64) as u32;
            let winner = tickets.get(winner_index).expect("Ticket out of bounds");
            winners.push_back(winner);
            winning_ticket_ids.push_back(winner_index);
            // Change seed for the next winner
            current_seed = current_seed.wrapping_add(1);
        }

        let mut claimed_winners = Vec::new(&env);
        for _ in 0..raffle.prizes.len() {
            claimed_winners.push_back(false);
        }

        raffle.status = RaffleStatus::Finalized;
        raffle.winners = winners.clone();
        raffle.claimed_winners = claimed_winners;
        raffle.finalized_at = Some(env.ledger().timestamp());
        write_raffle(&env, &raffle);

        publish_event(
            &env,
            "raffle_finalized",
            RaffleFinalized {
                winners,
                winning_ticket_ids,
                total_tickets_sold: raffle.tickets_sold,
                randomness_source: RandomnessSource::Internal,
                finalized_at: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "status_changed",
            StatusChanged {
                old_status: RaffleStatus::Drawing,
                new_status: RaffleStatus::Finalized,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn provide_randomness(env: Env, random_seed: u64) -> Result<Address, Error> {
        let mut raffle = read_raffle(&env)?;
        match &raffle.oracle_address {
            Some(oracle) => oracle.require_auth(),
            None => return Err(Error::NotAuthorized),
        }

        if raffle.status != RaffleStatus::Drawing
            || raffle.randomness_source != RandomnessSource::External
        {
            return Err(Error::InvalidStateTransition);
        }

        let tickets = read_tickets(&env);
        if tickets.len() == 0 {
            return Err(Error::NoTicketsSold);
        }

        let mut winners = Vec::new(&env);
        let mut winning_ticket_ids = Vec::new(&env);
        let mut current_seed = random_seed;

        for _ in 0..raffle.prizes.len() {
            let winner_index = (current_seed % tickets.len() as u64) as u32;
            let winner = tickets
                .get(winner_index)
                .expect("Ticket out of bounds callback");
            winners.push_back(winner);
            winning_ticket_ids.push_back(winner_index);
            // Change seed for the next winner
            current_seed = current_seed.wrapping_add(1);
        }

        let mut claimed_winners = Vec::new(&env);
        for _ in 0..raffle.prizes.len() {
            claimed_winners.push_back(false);
        }

        raffle.status = RaffleStatus::Finalized;
        raffle.winners = winners.clone();
        raffle.claimed_winners = claimed_winners;
        raffle.finalized_at = Some(env.ledger().timestamp());
        write_raffle(&env, &raffle);

        publish_event(
            &env,
            "randomness_received",
            RandomnessReceived {
                oracle: raffle.oracle_address.clone().ok_or(Error::InvalidParameters)?,
                seed: random_seed,
                timestamp: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "raffle_finalized",
            RaffleFinalized {
                winners: winners.clone(),
                winning_ticket_ids,
                total_tickets_sold: raffle.tickets_sold,
                randomness_source: RandomnessSource::External,
                finalized_at: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "status_changed",
            StatusChanged {
                old_status: RaffleStatus::Drawing,
                new_status: RaffleStatus::Finalized,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(winners.get(0).unwrap())
    }

    pub fn claim_prize(env: Env, winner: Address, tier_index: u32) -> Result<i128, Error> {
        winner.require_auth();
        let mut raffle = read_raffle(&env)?;

        // Checks
        if raffle.status != RaffleStatus::Finalized && raffle.status != RaffleStatus::Claimed {
            return Err(Error::InvalidStateTransition);
        }

        if tier_index >= raffle.winners.len() {
            return Err(Error::InvalidParameters);
        }

        if raffle.winners.get(tier_index).unwrap() != winner {
            return Err(Error::NotWinner);
        }

        if raffle.claimed_winners.get(tier_index).unwrap() {
            return Err(Error::PrizeAlreadyClaimed);
        }

        if !raffle.prize_deposited {
            return Err(Error::PrizeNotDeposited);
        }

        if env.ledger().timestamp() < raffle.finalized_at.unwrap_or(0) + 3600 {
            return Err(Error::ClaimTooEarly);
        }

        // Reentrancy guard
        acquire_guard(&env)?;

        let tier_prize_bp = raffle.prizes.get(tier_index).unwrap();
        let tier_prize_amount = (raffle.prize_amount * tier_prize_bp as i128) / 10000;

        let mut platform_fee = 0i128;
        if raffle.protocol_fee_bp > 0 {
            platform_fee = (tier_prize_amount * raffle.protocol_fee_bp as i128) / 10000;
        }
        let net_amount = tier_prize_amount - platform_fee;
        let claimed_at = env.ledger().timestamp();

        // Effects: update state BEFORE external calls (CEI pattern)
        let mut claimed_winners = raffle.claimed_winners;
        claimed_winners.set(tier_index, true);
        raffle.claimed_winners = claimed_winners;

        let mut all_claimed = true;
        for c in raffle.claimed_winners.iter() {
            if !c {
                all_claimed = false;
                break;
            }
        }

        let old_status = raffle.status.clone();
        if all_claimed {
            raffle.status = RaffleStatus::Claimed;
        }
        write_raffle(&env, &raffle);

        // Interactions: external token transfers
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();

        token_client.transfer(&contract_address, &winner, &net_amount);

        if platform_fee > 0 {
            if let (Some(router), Some(tikka)) = (&raffle.swap_router, &raffle.tikka_token) {
                if raffle.payment_token != *tikka {
                    // Approve router
                    token_client.approve(
                        &contract_address,
                        router,
                        &platform_fee,
                        &(env.ledger().sequence() + 100),
                    );

                    let mut path = Vec::new(&env);
                    path.push_back(raffle.payment_token.clone());
                    path.push_back(tikka.clone());

                    let router_client = SoroswapRouterClient::new(&env, router);
                    let amount_out = router_client.swap_exact_tokens_for_tokens(
                        &platform_fee,
                        &0i128,
                        &path,
                        &contract_address,
                        &(env.ledger().timestamp() + 300),
                    );

                    let tikka_client = token::Client::new(&env, tikka);
                    tikka_client.burn(&contract_address, &amount_out);

                    publish_event(
                        &env,
                        "buyback_and_burn_executed",
                        crate::events::BuybackAndBurnExecuted {
                            router: router.clone(),
                            tikka_token: tikka.clone(),
                            amount_in: platform_fee,
                            amount_out,
                            timestamp: env.ledger().timestamp(),
                        },
                    );
                } else {
                    let tikka_client = token::Client::new(&env, tikka);
                    tikka_client.burn(&contract_address, &platform_fee);

                    publish_event(
                        &env,
                        "buyback_and_burn_executed",
                        crate::events::BuybackAndBurnExecuted {
                            router: contract_address.clone(),
                            tikka_token: tikka.clone(),
                            amount_in: platform_fee,
                            amount_out: platform_fee,
                            timestamp: env.ledger().timestamp(),
                        },
                    );
                }
            } else if raffle.treasury_address.is_some() {
                token_client.transfer(
                    &contract_address,
                    &raffle.treasury_address.clone().unwrap(),
                    &platform_fee,
                );
            }
        }

        release_guard(&env);

        publish_event(
            &env,
            "prize_claimed",
            PrizeClaimed {
                winner: winner.clone(),
                tier_index,
                gross_amount: tier_prize_amount,
                net_amount,
                platform_fee,
                claimed_at,
            },
        );

        if old_status != raffle.status {
            publish_event(
                &env,
                "status_changed",
                StatusChanged {
                    old_status,
                    new_status: raffle.status.clone(),
                    timestamp: env.ledger().timestamp(),
                },
            );
        }

        Ok(net_amount)
    }

    pub fn cancel_raffle(env: Env, reason: CancelReason) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;

        // Admin or Creator can cancel
        match reason {
            CancelReason::CreatorCancelled => raffle.creator.require_auth(),
            CancelReason::AdminCancelled
            | CancelReason::OracleTimeout
            | CancelReason::MinTicketsNotMet => {
                let _factory: Address = env.storage().instance().get(&DataKey::Factory).unwrap();
                if reason == CancelReason::AdminCancelled {
                    raffle.creator.require_auth();
                } else {
                    raffle.creator.require_auth();
                }
            }
        }

        if raffle.status == RaffleStatus::Finalized
            || raffle.status == RaffleStatus::Claimed
            || raffle.status == RaffleStatus::Cancelled
        {
            return Err(Error::InvalidStateTransition);
        }

        let old_status = raffle.status.clone();
        raffle.status = RaffleStatus::Cancelled;

        // Effects: persist state BEFORE external call (CEI pattern)
        let should_refund_prize = raffle.prize_deposited;
        if should_refund_prize {
            raffle.prize_deposited = false;
        }
        write_raffle(&env, &raffle);

        // Interaction: external token transfer
        if should_refund_prize {
            let token_client = token::Client::new(&env, &raffle.payment_token);
            let contract_address = env.current_contract_address();
            token_client.transfer(&contract_address, &raffle.creator, &raffle.prize_amount);
        }

        publish_event(
            &env,
            "raffle_cancelled",
            RaffleCancelled {
                creator: raffle.creator.clone(),
                reason,
                tickets_sold: raffle.tickets_sold,
                timestamp: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "status_changed",
            StatusChanged {
                old_status,
                new_status: RaffleStatus::Cancelled,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn refund_ticket(env: Env, ticket_id: u32) -> Result<i128, Error> {
        let raffle = read_raffle(&env)?;

        if raffle.status != RaffleStatus::Cancelled {
            return Err(Error::InvalidStateTransition);
        }

        let ticket_opt = env
            .storage()
            .persistent()
            .get::<_, Ticket>(&DataKey::Ticket(ticket_id));
        if ticket_opt.is_none() {
            // Re-using InvalidParameters for missing ticket to avoid adding new error enum right now
            return Err(Error::InvalidParameters);
        }
        let ticket = ticket_opt.unwrap();

        ticket.owner.require_auth();

        let is_refunded = env
            .storage()
            .persistent()
            .get(&DataKey::RefundStatus(ticket_id))
            .unwrap_or(false);
        // Enforce idempotency
        if is_refunded {
            return Err(Error::InvalidStateTransition);
        }

        // Reentrancy guard
        acquire_guard(&env)?;

        // Effects: mark refunded BEFORE external call (CEI pattern)
        env.storage()
            .persistent()
            .set(&DataKey::RefundStatus(ticket_id), &true);

        // Interaction: external token transfer
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&contract_address, &ticket.owner, &raffle.ticket_price);

        release_guard(&env);

        publish_event(
            &env,
            "ticket_refunded",
            crate::events::TicketRefunded {
                buyer: ticket.owner.clone(),
                ticket_id,
                amount: raffle.ticket_price,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(raffle.ticket_price)
    }

    // --- NFT Interface ---
    pub fn name(env: Env) -> String {
        String::from_str(&env, "Tikka Raffle Ticket")
    }

    pub fn symbol(env: Env) -> String {
        String::from_str(&env, "TIKKA_TKT")
    }

    pub fn token_uri(env: Env, _token_id: u32) -> String {
        String::from_str(&env, "https://tikka.app/api/ticket")
    }

    pub fn balance(env: Env, owner: Address) -> u32 {
        read_ticket_count(&env, &owner)
    }

    pub fn owner_of(env: Env, token_id: u32) -> Result<Address, Error> {
        let ticket_opt = env.storage().persistent().get::<_, Ticket>(&DataKey::Ticket(token_id));
        if let Some(ticket) = ticket_opt {
            Ok(ticket.owner)
        } else {
            Err(Error::InvalidParameters)
        }
    }

    pub fn approve(env: Env, caller: Address, operator: Option<Address>, token_id: u32) -> Result<(), Error> {
        caller.require_auth();
        let ticket_opt = env.storage().persistent().get::<_, Ticket>(&DataKey::Ticket(token_id));
        let owner = ticket_opt.ok_or(Error::InvalidParameters)?.owner;
        
        let is_approved_for_all = env.storage().persistent().get::<_, bool>(&DataKey::ApprovedForAll(owner.clone(), caller.clone())).unwrap_or(false);
        if caller != owner && !is_approved_for_all {
            return Err(Error::NotAuthorized);
        }

        if let Some(op) = operator {
            env.storage().persistent().set(&DataKey::Approved(token_id), &op);
        } else {
            env.storage().persistent().remove(&DataKey::Approved(token_id));
        }
        Ok(())
    }

    pub fn set_approval_for_all(env: Env, caller: Address, operator: Address, approved: bool) -> Result<(), Error> {
        caller.require_auth();
        env.storage().persistent().set(&DataKey::ApprovedForAll(caller, operator), &approved);
        Ok(())
    }

    pub fn get_approved(env: Env, token_id: u32) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Approved(token_id))
    }

    pub fn is_approved_for_all(env: Env, owner: Address, operator: Address) -> bool {
        env.storage().persistent().get(&DataKey::ApprovedForAll(owner, operator)).unwrap_or(false)
    }

    pub fn transfer(env: Env, from: Address, to: Address, token_id: u32) -> Result<(), Error> {
        from.require_auth();
        do_transfer(&env, from, to, token_id)
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, token_id: u32) -> Result<(), Error> {
        spender.require_auth();
        let is_approved_for_all = env.storage().persistent().get::<_, bool>(&DataKey::ApprovedForAll(from.clone(), spender.clone())).unwrap_or(false);
        let individual_approval = env.storage().persistent().get::<_, Address>(&DataKey::Approved(token_id));
        
        if spender != from && !is_approved_for_all && individual_approval != Some(spender.clone()) {
            return Err(Error::NotAuthorized);
        }
        do_transfer(&env, from, to, token_id)
    }

    pub fn get_raffle(env: Env) -> Result<Raffle, Error> {
        read_raffle(&env)
    }

    /// Get all tickets or a paginated subset
    /// Returns tickets from start index for count number of tickets
    pub fn get_tickets(env: Env, start: u32, count: u32) -> Vec<Ticket> {
        let all_tickets = read_tickets(&env);
        let total = all_tickets.len();
        
        if start >= total {
            return Vec::new(&env);
        }
        
        let end = if start + count > total { total } else { start + count };
        let mut result = Vec::new(&env);
        
        for i in start..end {
            result.push_back(all_tickets.get(i).unwrap());
        }
        
        result
    }

    /// Get total ticket count
    pub fn get_ticket_count(env: Env) -> u32 {
        read_tickets(&env).len()
    }

    pub fn pause(env: Env) -> Result<(), Error> {
        let factory: Address = env
            .storage()
            .instance()
            .get(&DataKey::Factory)
            .ok_or(Error::NotAuthorized)?;
        factory.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);

        publish_event(
            &env,
            "contract_paused",
            crate::events::ContractPaused {
                paused_by: factory,
                timestamp: env.ledger().timestamp(),
            },
        );

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

        publish_event(
            &env,
            "contract_unpaused",
            crate::events::ContractUnpaused {
                unpaused_by: factory,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let factory: Address = env
            .storage()
            .instance()
            .get(&DataKey::Factory)
            .ok_or(Error::NotAuthorized)?;
        factory.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }
}

#[cfg(test)]
mod test;
