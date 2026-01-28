#![no_std]
use core::cmp::min;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env,
    String, Vec,
};

#[contract]
pub struct Contract;

#[derive(Clone)]
#[contracttype]
pub struct Raffle {
    pub id: u64,
    pub creator: Address,
    pub description: String,
    pub end_time: u64,
    pub max_tickets: u32,
    pub allow_multiple: bool,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub prize_amount: i128,
    pub tickets_sold: u32,
    pub is_active: bool,
    pub prize_deposited: bool,
    pub prize_claimed: bool,
    pub winner: Option<Address>,
}

#[derive(Clone, PartialEq, Eq)]
#[contracttype]
pub enum RaffleStatus {
    Active,
    Finalized,
    Claimed,
}

#[derive(Clone)]
#[contracttype]
pub struct RaffleStats {
    pub tickets_sold: u32,
    pub max_tickets: u32,
    pub tickets_remaining: u32,
    pub total_revenue: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct RaffleWithStats {
    pub raffle: Raffle,
    pub stats: RaffleStats,
}

#[derive(Clone)]
#[contracttype]
pub struct Ticket {
    pub id: u32,
    pub raffle_id: u64,
    pub buyer: Address,
    pub purchase_time: u64,
    pub ticket_number: u32,
}

// --- Events (Fixed: Added #[contractevent] to all) ---

#[contractevent(topics = ["PrizeClaimed", "raffle_id"])]
#[derive(Clone)]
pub struct PrizeClaimed {
    pub raffle_id: u64,
    pub winner: Address,
    pub gross_amount: i128,
    pub net_amount: i128,
    pub platform_fee: i128,
    pub claimed_at: u64,
}

#[contractevent(topics = ["RaffleCreated", "raffle_id"])]
#[derive(Clone)]
pub struct RaffleCreated {
    pub raffle_id: u64,
    pub creator: Address,
    pub end_time: u64,
    pub max_tickets: u32,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub description: String,
}

#[contractevent(topics = ["RaffleFinalized", "raffle_id"])]
#[derive(Clone, Debug)]
pub struct RaffleFinalized {
    pub raffle_id: u64,
    pub winner: Address,
    pub winning_ticket_id: u32,
    pub total_tickets_sold: u32,
    pub randomness_source: String,
    pub finalized_at: u64,
}

#[contractevent(topics = ["TicketPurchased", "raffle_id"])]
#[derive(Clone)]
pub struct TicketPurchased {
    pub raffle_id: u64,
    pub buyer: Address,
    pub ticket_ids: Vec<u32>,
    pub quantity: u32,
    pub total_paid: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    NextRaffleId,
    Raffle(u64),
    Tickets(u64),
    TicketCount(u64, Address),
    ActiveRaffles,
    Ticket(u64, u32),
    NextTicketId(u64),
    UserRaffles(Address),
}

// --- Error Types ---

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Error {
    /// Raffle with the specified ID does not exist (Code: 1)
    RaffleNotFound = 1,

    /// Raffle is not active or has been finalized (Code: 2)
    RaffleInactive = 2,

    /// All tickets have been sold out (Code: 3)
    TicketsSoldOut = 3,

    /// Payment amount is insufficient (Code: 4)
    InsufficientPayment = 4,

    /// Caller is not authorized for this operation (Code: 5)
    NotAuthorized = 5,

    /// Prize has not been deposited yet (Code: 6)
    PrizeNotDeposited = 6,

    /// Prize has already been claimed (Code: 7)
    PrizeAlreadyClaimed = 7,

    /// Invalid parameters provided (Code: 8)
    InvalidParameters = 8,

    /// Contract is paused (Code: 9)
    ContractPaused = 9,

    /// Not enough tickets available for purchase (Code: 10)
    InsufficientTickets = 10,

    /// Raffle has already ended (Code: 11)
    RaffleEnded = 11,

    /// Raffle is still running and cannot be finalized yet (Code: 12)
    RaffleStillRunning = 12,

    /// No tickets were sold for this raffle (Code: 13)
    NoTicketsSold = 13,

    /// Multiple tickets not allowed for this raffle (Code: 14)
    MultipleTicketsNotAllowed = 14,

    /// Prize has already been deposited (Code: 15)
    PrizeAlreadyDeposited = 15,

    /// Caller is not the winner (Code: 16)
    NotWinner = 16,

    /// Arithmetic overflow occurred (Code: 17)
    ArithmeticOverflow = 17,
}

// --- Helper Functions ---

/// Pagination metadata for list queries
#[derive(Clone)]
#[contracttype]
pub struct PaginationMeta {
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
    pub has_more: bool,
}

/// Paginated result for raffle ID queries
#[derive(Clone)]
#[contracttype]
pub struct PaginatedRaffleIds {
    pub data: Vec<u64>,
    pub meta: PaginationMeta,
}

/// Paginated result for ticket (address) queries
#[derive(Clone)]
#[contracttype]
pub struct PaginatedTickets {
    pub data: Vec<Address>,
    pub meta: PaginationMeta,
}

/// User participation data for raffles
#[derive(Clone)]
#[contracttype]
pub struct UserParticipation {
    pub raffle_ids: Vec<u64>,
    pub ticket_counts: Vec<u32>,
    pub total_spent: i128,
    pub win_count: u32,
    pub total_winnings: i128,
}

const MAX_PAGE_LIMIT: u32 = 100;

fn read_raffle(env: &Env, raffle_id: u64) -> Result<Raffle, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Raffle(raffle_id))
        .ok_or(Error::RaffleNotFound)
}

fn write_raffle(env: &Env, raffle: &Raffle) {
    env.storage()
        .persistent()
        .set(&DataKey::Raffle(raffle.id), raffle);
}

fn read_tickets(env: &Env, raffle_id: u64) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::Tickets(raffle_id))
        .unwrap_or_else(|| Vec::new(env))
}

fn write_tickets(env: &Env, raffle_id: u64, tickets: &Vec<Address>) {
    env.storage()
        .persistent()
        .set(&DataKey::Tickets(raffle_id), tickets);
}

fn read_ticket_count(env: &Env, raffle_id: u64, buyer: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::TicketCount(raffle_id, buyer.clone()))
        .unwrap_or(0)
}

fn write_ticket_count(env: &Env, raffle_id: u64, buyer: &Address, count: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::TicketCount(raffle_id, buyer.clone()), &count);
}

fn build_raffle_stats(raffle: &Raffle) -> Result<RaffleStats, Error> {
    let tickets_remaining = raffle
        .max_tickets
        .checked_sub(raffle.tickets_sold)
        .ok_or(Error::ArithmeticOverflow)?;
    let total_revenue = raffle
        .ticket_price
        .checked_mul(raffle.tickets_sold as i128)
        .ok_or(Error::ArithmeticOverflow)?;

    Ok(RaffleStats {
        tickets_sold: raffle.tickets_sold,
        max_tickets: raffle.max_tickets,
        tickets_remaining,
        total_revenue,
    })
}

fn build_raffle_status(raffle: &Raffle) -> RaffleStatus {
    if raffle.prize_claimed {
        return RaffleStatus::Claimed;
    }
    if raffle.is_active {
        return RaffleStatus::Active;
    }
    RaffleStatus::Finalized
}

fn next_raffle_id(env: &Env) -> u64 {
    let current = env
        .storage()
        .persistent()
        .get(&DataKey::NextRaffleId)
        .unwrap_or(0u64);
    let next = current + 1;
    env.storage()
        .persistent()
        .set(&DataKey::NextRaffleId, &next);
    current
}

fn read_active_raffles(env: &Env) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::ActiveRaffles)
        .unwrap_or_else(|| Vec::new(env))
}

fn write_active_raffles(env: &Env, active_raffles: &Vec<u64>) {
    env.storage()
        .persistent()
        .set(&DataKey::ActiveRaffles, active_raffles);
}

fn add_active_raffle(env: &Env, raffle_id: u64) {
    let mut active_raffles = read_active_raffles(env);
    active_raffles.push_back(raffle_id);
    write_active_raffles(env, &active_raffles);
}

fn remove_active_raffle(env: &Env, raffle_id: u64) {
    let active_raffles = read_active_raffles(env);
    let mut new_active = Vec::new(env);
    for i in 0..active_raffles.len() {
        let id = active_raffles.get(i).unwrap();
        if id != raffle_id {
            new_active.push_back(id);
        }
    }
    write_active_raffles(env, &new_active);
}

fn next_ticket_id(env: &Env, raffle_id: u64) -> u32 {
    let current = env
        .storage()
        .persistent()
        .get(&DataKey::NextTicketId(raffle_id))
        .unwrap_or(0u32);
    let next = current + 1;
    env.storage()
        .persistent()
        .set(&DataKey::NextTicketId(raffle_id), &next);
    next
}

fn write_ticket(env: &Env, raffle_id: u64, ticket: &Ticket) {
    env.storage()
        .persistent()
        .set(&DataKey::Ticket(raffle_id, ticket.id), ticket);
}

fn read_ticket(env: &Env, raffle_id: u64, ticket_id: u32) -> Option<Ticket> {
    env.storage()
        .persistent()
        .get(&DataKey::Ticket(raffle_id, ticket_id))
}

fn read_user_raffles(env: &Env, user: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::UserRaffles(user.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn write_user_raffles(env: &Env, user: &Address, raffle_ids: &Vec<u64>) {
    env.storage()
        .persistent()
        .set(&DataKey::UserRaffles(user.clone()), raffle_ids);
}

fn add_user_raffle(env: &Env, user: &Address, raffle_id: u64) {
    let mut user_raffles = read_user_raffles(env, user);
    // Check if raffle_id is already in the list
    let mut found = false;
    for i in 0..user_raffles.len() {
        if user_raffles.get(i).unwrap() == raffle_id {
            found = true;
            break;
        }
    }
    if !found {
        user_raffles.push_back(raffle_id);
        write_user_raffles(env, user, &user_raffles);
    }
}

// --- Contract Implementation ---

#[contractimpl]
impl Contract {
    pub fn create_raffle(
        env: Env,
        creator: Address,
        description: String,
        end_time: u64,
        max_tickets: u32,
        allow_multiple: bool,
        ticket_price: i128,
        payment_token: Address,
        prize_amount: i128,
    ) -> Result<u64, Error> {
        creator.require_auth();
        let now = env.ledger().timestamp();
        if end_time < now && end_time != 0 {
            return Err(Error::InvalidParameters);
        }
        if max_tickets == 0 {
            return Err(Error::InvalidParameters);
        }
        if ticket_price <= 0 {
            return Err(Error::InvalidParameters);
        }
        if prize_amount <= 0 {
            return Err(Error::InvalidParameters);
        }

        let raffle_id = next_raffle_id(&env);
        let raffle = Raffle {
            id: raffle_id,
            creator: creator.clone(),
            description: description.clone(),
            end_time,
            max_tickets,
            allow_multiple,
            ticket_price,
            payment_token: payment_token.clone(),
            prize_amount,
            tickets_sold: 0,
            is_active: true,
            prize_deposited: false,
            prize_claimed: false,
            winner: None,
        };
        write_raffle(&env, &raffle);

        RaffleCreated {
            raffle_id,
            creator,
            end_time,
            max_tickets,
            ticket_price,
            payment_token,
            description,
        }
        .publish(&env);

        add_active_raffle(&env, raffle_id);
        Ok(raffle_id)
    }

    pub fn deposit_prize(env: Env, raffle_id: u64) -> Result<(), Error> {
        let mut raffle = read_raffle(&env, raffle_id)?;
        raffle.creator.require_auth();
        if !raffle.is_active {
            return Err(Error::RaffleInactive);
        }
        if raffle.prize_deposited {
            return Err(Error::PrizeAlreadyDeposited);
        }

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&raffle.creator, &contract_address, &raffle.prize_amount);

        raffle.prize_deposited = true;
        write_raffle(&env, &raffle);
        Ok(())
    }

    pub fn buy_ticket(env: Env, raffle_id: u64, buyer: Address) -> Result<u32, Error> {
        buyer.require_auth();
        let mut raffle = read_raffle(&env, raffle_id)?;
        if !raffle.is_active {
            return Err(Error::RaffleInactive);
        }
        if raffle.end_time != 0 && env.ledger().timestamp() > raffle.end_time {
            return Err(Error::RaffleEnded);
        }
        if raffle.tickets_sold >= raffle.max_tickets {
            return Err(Error::TicketsSoldOut);
        }

        let current_count = read_ticket_count(&env, raffle_id, &buyer);
        if !raffle.allow_multiple && current_count > 0 {
            return Err(Error::MultipleTicketsNotAllowed);
        }

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&buyer, &contract_address, &raffle.ticket_price);

        let ticket_id = next_ticket_id(&env, raffle_id);
        let timestamp = env.ledger().timestamp();

        let ticket = Ticket {
            id: ticket_id,
            raffle_id,
            buyer: buyer.clone(),
            purchase_time: timestamp,
            ticket_number: raffle.tickets_sold + 1,
        };
        write_ticket(&env, raffle_id, &ticket);

        let mut tickets = read_tickets(&env, raffle_id);
        tickets.push_back(buyer.clone());
        write_tickets(&env, raffle_id, &tickets);

        raffle.tickets_sold += 1;
        write_ticket_count(&env, raffle_id, &buyer, current_count + 1);
        write_raffle(&env, &raffle);
        add_user_raffle(&env, &buyer, raffle_id);

        let mut ticket_ids = Vec::new(&env);
        ticket_ids.push_back(ticket_id);

        TicketPurchased {
            raffle_id,
            buyer,
            ticket_ids,
            quantity: 1u32,
            total_paid: raffle.ticket_price,
            timestamp,
        }
        .publish(&env);

        Ok(raffle.tickets_sold)
    }

    /// Purchases multiple tickets for the specified raffle in a single transaction.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    /// * `buyer` - The address purchasing the tickets (must be authenticated)
    /// * `quantity` - The number of tickets to purchase
    ///
    /// # Returns
    /// * `u32` - The total number of tickets sold for this raffle after purchase
    ///
    /// # Panics
    /// * If quantity is zero
    /// * If the raffle is inactive
    /// * If the raffle has ended
    /// * If quantity exceeds available tickets (max_tickets - tickets_sold)
    /// * If multiple tickets are not allowed and buyer already has tickets
    /// * If multiple tickets are not allowed and quantity > 1
    pub fn buy_tickets(
        env: Env,
        raffle_id: u64,
        buyer: Address,
        quantity: u32,
    ) -> Result<u32, Error> {
        buyer.require_auth();
        let mut raffle = read_raffle(&env, raffle_id)?;

        if quantity == 0 {
            return Err(Error::InvalidParameters);
        }
        if !raffle.is_active {
            return Err(Error::RaffleInactive);
        }
        if raffle.end_time != 0 && env.ledger().timestamp() > raffle.end_time {
            return Err(Error::RaffleEnded);
        }

        let remaining_tickets = raffle.max_tickets - raffle.tickets_sold;
        if quantity > remaining_tickets {
            return Err(Error::InsufficientTickets);
        }

        let current_count = read_ticket_count(&env, raffle_id, &buyer);
        if !raffle.allow_multiple {
            if current_count > 0 {
                return Err(Error::MultipleTicketsNotAllowed);
            }
            if quantity > 1 {
                return Err(Error::MultipleTicketsNotAllowed);
            }
        }

        // Calculate total cost: quantity × ticket_price
        let total_cost = raffle
            .ticket_price
            .checked_mul(quantity as i128)
            .ok_or(Error::ArithmeticOverflow)?;

        // Process single payment transfer
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&buyer, &contract_address, &total_cost);

        let timestamp = env.ledger().timestamp();
        let mut ticket_ids = Vec::new(&env);

        for i in 0..quantity {
            let ticket_id = next_ticket_id(&env, raffle_id);
            let ticket = Ticket {
                id: ticket_id,
                raffle_id,
                buyer: buyer.clone(),
                purchase_time: timestamp,
                ticket_number: raffle.tickets_sold + i + 1,
            };
            write_ticket(&env, raffle_id, &ticket);
            ticket_ids.push_back(ticket_id);
        }

        let mut tickets = read_tickets(&env, raffle_id);
        for _ in 0..quantity {
            tickets.push_back(buyer.clone());
        }
        write_tickets(&env, raffle_id, &tickets);

        raffle.tickets_sold += quantity;
        write_ticket_count(&env, raffle_id, &buyer, current_count + quantity);
        write_raffle(&env, &raffle);
        add_user_raffle(&env, &buyer, raffle_id);

        // Emit TicketPurchased event with all ticket IDs
        TicketPurchased {
            raffle_id,
            buyer,
            ticket_ids,
            quantity,
            total_paid: total_cost,
            timestamp,
        }
        .publish(&env);

        Ok(raffle.tickets_sold)
    }

    /// Finalizes a raffle and selects a winner.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle to finalize
    /// * `source` - The randomness source identifier
    ///
    /// # Returns
    /// * `Address` - The address of the winner
    ///
    /// # Errors
    /// * If the caller is not the creator
    /// * If the raffle is inactive
    /// * If the raffle has not ended yet
    /// * If no tickets were sold
    pub fn finalize_raffle(env: Env, raffle_id: u64, source: String) -> Result<Address, Error> {
        let mut raffle = read_raffle(&env, raffle_id)?;
        raffle.creator.require_auth();
        if !raffle.is_active {
            return Err(Error::RaffleInactive);
        }
        if raffle.end_time != 0 && env.ledger().timestamp() < raffle.end_time {
            return Err(Error::RaffleStillRunning);
        }
        if raffle.tickets_sold == 0 {
            return Err(Error::NoTicketsSold);
        }

        let tickets = read_tickets(&env, raffle_id);
        let seed = env.ledger().timestamp() + env.ledger().sequence() as u64;
        let winner_index = (seed % tickets.len() as u64) as u32;
        let winner = tickets.get(winner_index).unwrap();

        raffle.is_active = false;
        raffle.winner = Some(winner.clone());
        write_raffle(&env, &raffle);
        remove_active_raffle(&env, raffle_id);

        RaffleFinalized {
            raffle_id,
            winner: winner.clone(),
            winning_ticket_id: winner_index,
            total_tickets_sold: raffle.tickets_sold,
            randomness_source: source,
            finalized_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(winner)
    }

    pub fn claim_prize(env: Env, raffle_id: u64, winner: Address) -> Result<i128, Error> {
        winner.require_auth();
        let mut raffle = read_raffle(&env, raffle_id)?;
        if raffle.winner != Some(winner.clone()) {
            return Err(Error::NotWinner);
        }
        if !raffle.prize_deposited {
            return Err(Error::PrizeNotDeposited);
        }
        if raffle.prize_claimed {
            return Err(Error::PrizeAlreadyClaimed);
        }

        let gross_amount = raffle.prize_amount;
        let platform_fee = 0i128;
        let net_amount = gross_amount - platform_fee;
        let claimed_at = env.ledger().timestamp();

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&contract_address, &winner, &net_amount);

        PrizeClaimed {
            raffle_id,
            winner: winner.clone(),
            gross_amount,
            net_amount,
            platform_fee,
            claimed_at,
        }
        .publish(&env);

        raffle.prize_claimed = true;
        write_raffle(&env, &raffle);
        Ok(net_amount)
    }

    pub fn get_raffle(env: Env, raffle_id: u64) -> Result<Raffle, Error> {
        read_raffle(&env, raffle_id)
    }

    pub fn get_raffle_by_id(env: Env, raffle_id: u64) -> Result<RaffleWithStats, Error> {
        let raffle = read_raffle(&env, raffle_id)?;
        let stats = build_raffle_stats(&raffle)?;
        Ok(RaffleWithStats { raffle, stats })
    }

    pub fn get_user_tickets(env: Env, raffle_id: u64, user: Address) -> u32 {
        read_ticket_count(&env, raffle_id, &user)
    }

    pub fn get_raffle_status(env: Env, raffle_id: u64) -> Result<RaffleStatus, Error> {
        let raffle = read_raffle(&env, raffle_id)?;
        Ok(build_raffle_status(&raffle))
    }

    /// Retrieves aggregated statistics for a raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    ///
    /// # Returns
    /// * `RaffleStats` - Aggregated statistics for the raffle
    ///
    /// # Errors
    /// * If the raffle does not exist
    pub fn get_raffle_stats(env: Env, raffle_id: u64) -> Result<RaffleStats, Error> {
        let raffle = read_raffle(&env, raffle_id)?;
        build_raffle_stats(&raffle)
    }

    /// Retrieves all raffle IDs with pagination.
    ///
    /// # Arguments
    /// * `offset` - The starting index within the sorted list
    /// * `limit` - Maximum number of results (capped at 100)
    /// * `newest_first` - If true, returns newest raffles first
    ///
    /// # Returns
    /// * `PaginatedRaffleIds` - Paginated result with data and metadata
    pub fn get_all_raffle_ids(
        env: Env,
        offset: u32,
        limit: u32,
        newest_first: bool,
    ) -> PaginatedRaffleIds {
        let total = env
            .storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u64) as u32;
        let capped_limit = min(limit, MAX_PAGE_LIMIT);
        let mut data = Vec::new(&env);

        if capped_limit == 0 || total == 0 || offset >= total {
            return PaginatedRaffleIds {
                data,
                meta: PaginationMeta {
                    total,
                    offset,
                    limit: capped_limit,
                    has_more: false,
                },
            };
        }

        let end = min(offset + capped_limit, total);
        if newest_first {
            for position in offset..end {
                let raffle_id = (total - 1 - position) as u64;
                data.push_back(raffle_id);
            }
        } else {
            for raffle_id in offset..end {
                data.push_back(raffle_id as u64);
            }
        }

        let has_more = end < total;

        PaginatedRaffleIds {
            data,
            meta: PaginationMeta {
                total,
                offset,
                limit: capped_limit,
                has_more,
            },
        }
    }

    pub fn get_tickets(env: Env, raffle_id: u64) -> Result<Vec<Ticket>, Error> {
        let raffle = read_raffle(&env, raffle_id)?;
        let mut tickets = Vec::new(&env);
        for ticket_num in 1..=raffle.tickets_sold {
            if let Some(ticket) = read_ticket(&env, raffle_id, ticket_num) {
                tickets.push_back(ticket);
            }
        }
        Ok(tickets)
    }

    pub fn get_ticket(env: Env, raffle_id: u64, ticket_id: u32) -> Option<Ticket> {
        read_ticket(&env, raffle_id, ticket_id)
    }

    pub fn get_tickets_by_buyer(
        env: Env,
        raffle_id: u64,
        buyer: Address,
    ) -> Result<Vec<Ticket>, Error> {
        let raffle = read_raffle(&env, raffle_id)?;
        let mut buyer_tickets = Vec::new(&env);
        for ticket_num in 1..=raffle.tickets_sold {
            if let Some(ticket) = read_ticket(&env, raffle_id, ticket_num) {
                if ticket.buyer == buyer {
                    buyer_tickets.push_back(ticket);
                }
            }
        }
        Ok(buyer_tickets)
    }

    /// Retrieves active raffle IDs with pagination.
    ///
    /// # Arguments
    /// * `offset` - The starting index
    /// * `limit` - Maximum number of results (capped at 100)
    ///
    /// # Returns
    /// * `PaginatedRaffleIds` - Paginated result with data and metadata
    pub fn get_active_raffle_ids(env: Env, offset: u32, limit: u32) -> PaginatedRaffleIds {
        let capped_limit = min(limit, MAX_PAGE_LIMIT);
        let all_active = read_active_raffles(&env);
        let current_time = env.ledger().timestamp();

        // First pass: count total active raffles
        let mut total_active = 0u32;
        for i in 0..all_active.len() {
            let raffle_id = all_active.get(i).unwrap();
            if let Ok(raffle) = read_raffle(&env, raffle_id) {
                if raffle.is_active && raffle.end_time > current_time {
                    total_active += 1;
                }
            }
        }

        let mut data = Vec::new(&env);

        if capped_limit == 0 || total_active == 0 || offset >= total_active {
            return PaginatedRaffleIds {
                data,
                meta: PaginationMeta {
                    total: total_active,
                    offset,
                    limit: capped_limit,
                    has_more: false,
                },
            };
        }

        // Second pass: collect paginated results
        let mut count = 0u32;
        let mut skipped = 0u32;

        for i in 0..all_active.len() {
            if count >= capped_limit {
                break;
            }
            let raffle_id = all_active.get(i).unwrap();
            if let Ok(raffle) = read_raffle(&env, raffle_id) {
                if raffle.is_active && (raffle.end_time == 0 || raffle.end_time > current_time) {
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    data.push_back(raffle_id);
                    count += 1;
                }
            }
        }

        let has_more = (offset + count) < total_active;

        PaginatedRaffleIds {
            data,
            meta: PaginationMeta {
                total: total_active,
                offset,
                limit: capped_limit,
                has_more,
            },
        }
    }

    /// Retrieves comprehensive participation data for a user across all raffles with pagination.
    ///
    /// # Arguments
    /// * `user` - The address of the user
    /// * `offset` - The starting index within the user's raffle list
    /// * `limit` - Maximum number of results (capped at 100)
    ///
    /// # Returns
    /// * `UserParticipation` - Complete participation data including statistics
    pub fn get_user_raffle_participation(
        env: Env,
        user: Address,
        offset: u32,
        limit: u32,
    ) -> UserParticipation {
        let capped_limit = min(limit, MAX_PAGE_LIMIT);
        let all_user_raffles = read_user_raffles(&env, &user);
        let total = all_user_raffles.len() as u32;

        let mut raffle_ids = Vec::new(&env);
        let mut ticket_counts = Vec::new(&env);
        let mut total_spent = 0i128;
        let mut win_count = 0u32;
        let mut total_winnings = 0i128;

        if capped_limit == 0 || total == 0 || offset >= total {
            return UserParticipation {
                raffle_ids,
                ticket_counts,
                total_spent,
                win_count,
                total_winnings,
            };
        }

        let end = min(offset + capped_limit, total);
        for i in offset..end {
            let raffle_id = all_user_raffles.get(i as u32).unwrap();
            
            // Read raffle to get ticket price and check if user won
            if let Ok(raffle) = read_raffle(&env, raffle_id) {
                let ticket_count = read_ticket_count(&env, raffle_id, &user);
                
                // Calculate total spent for this raffle
                let spent_for_raffle = raffle
                    .ticket_price
                    .checked_mul(ticket_count as i128)
                    .unwrap_or(0i128);
                total_spent = total_spent
                    .checked_add(spent_for_raffle)
                    .unwrap_or(total_spent);

                // Check if user won this raffle
                if let Some(winner) = raffle.winner {
                    if winner == user {
                        win_count += 1;
                        // Calculate net winnings (prize amount minus platform fee)
                        let platform_fee = 0i128; // Currently no platform fee
                        let net_winnings = raffle
                            .prize_amount
                            .checked_sub(platform_fee)
                            .unwrap_or(0i128);
                        total_winnings = total_winnings
                            .checked_add(net_winnings)
                            .unwrap_or(total_winnings);
                    }
                }

                raffle_ids.push_back(raffle_id);
                ticket_counts.push_back(ticket_count);
            }
        }

        UserParticipation {
            raffle_ids,
            ticket_counts,
            total_spent,
            win_count,
            total_winnings,
        }
    }
}

#[cfg(test)]
mod test;
