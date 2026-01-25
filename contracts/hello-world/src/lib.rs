#![no_std]
use core::cmp::min;
use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, token, Address, Env, String, Vec, symbol_short
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

#[derive(Clone)]
#[contracttype]
pub struct PrizeClaimed {
    pub raffle_id: u64,
    pub winner: Address,
    pub gross_amount: i128,
    pub net_amount: i128,
    pub platform_fee: i128,
    pub claimed_at: u64,
}

/// Event emitted when a new raffle is created.
///
/// This event provides all essential information about the raffle
/// for frontend indexing and real-time updates.
///
/// # Topics
/// - "RaffleCreated": Static event name for efficient filtering
/// - raffle_id: Indexed for quick raffle lookups
///
/// # Fields
/// * `raffle_id` - Unique identifier for the raffle
/// * `creator` - Address of the raffle creator
/// * `end_time` - Unix timestamp when the raffle ends
/// * `max_tickets` - Maximum number of tickets available
/// * `ticket_price` - Price per ticket in payment token units
/// * `payment_token` - Address of the token used for payments
/// * `description` - Human-readable description of the raffle
///
/// # Usage
/// Frontends can listen for this event to:
/// - Display newly created raffles immediately
/// - Index raffle data for search and filtering
/// - Trigger notifications to users
/// - Populate raffle lists without querying all storage
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
}

#[derive(Clone)]
#[contracttype]
pub struct TicketPurchased {
    pub raffle_id: u64,
    pub buyer: Address,
    pub ticket_ids: Vec<u32>,
    pub quantity: u32,
    pub total_paid: i128,
    pub timestamp: u64,
}

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

const MAX_PAGE_LIMIT: u32 = 100;

fn read_raffle(env: &Env, raffle_id: u64) -> Raffle {
    env.storage()
        .persistent()
        .get(&DataKey::Raffle(raffle_id))
        .unwrap_or_else(|| panic!("raffle_not_found"))
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

fn build_raffle_stats(raffle: &Raffle) -> RaffleStats {
    let tickets_remaining = raffle
        .max_tickets
        .checked_sub(raffle.tickets_sold)
        .unwrap_or_else(|| panic!("tickets_overflow"));
    let total_revenue = raffle
        .ticket_price
        .checked_mul(raffle.tickets_sold as i128)
        .unwrap_or_else(|| panic!("revenue_overflow"));

    RaffleStats {
        tickets_sold: raffle.tickets_sold,
        max_tickets: raffle.max_tickets,
        tickets_remaining,
        total_revenue,
    }
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

#[contractimpl]
impl Contract {
    /// Creates a new raffle with the specified parameters.
    ///
    /// # Arguments
    /// * `creator` - The address creating the raffle (must be authenticated)
    /// * `description` - Description of the raffle
    /// * `end_time` - Unix timestamp when the raffle ends
    /// * `max_tickets` - Maximum number of tickets that can be sold
    /// * `allow_multiple` - Whether a single buyer can purchase multiple tickets
    /// * `ticket_price` - Price per ticket in payment token units
    /// * `payment_token` - Address of the token contract for payments
    /// * `prize_amount` - Amount of prize in payment token units
    ///
    /// # Returns
    /// * `u64` - The ID of the newly created raffle
    ///
    /// # Panics
    /// * If end_time is in the past
    /// * If max_tickets is zero
    /// * If ticket_price is invalid (<= 0)
    /// * If prize_amount is invalid (<= 0)
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
    ) -> u64 {
        creator.require_auth();
        let now = env.ledger().timestamp();
        if end_time < now {
            panic!("end_time_in_past");
        }
        if max_tickets == 0 {
            panic!("max_tickets_zero");
        }
        if ticket_price <= 0 {
            panic!("ticket_price_invalid");
        }
        if prize_amount <= 0 {
            panic!("prize_amount_invalid");
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
        }.publish(&env);

        add_active_raffle(&env, raffle_id);
        raffle_id
    }

    /// Deposits the prize amount into the contract escrow.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    ///
    /// # Panics
    /// * If the caller is not the creator
    /// * If the raffle is inactive
    /// * If the prize has already been deposited
    pub fn deposit_prize(env: Env, raffle_id: u64) {
        let mut raffle = read_raffle(&env, raffle_id);
        raffle.creator.require_auth();
        if !raffle.is_active {
            panic!("raffle_inactive");
        }
        if raffle.prize_deposited {
            panic!("prize_already_deposited");
        }

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&raffle.creator, &contract_address, &raffle.prize_amount);

        raffle.prize_deposited = true;
        write_raffle(&env, &raffle);
    }

    /// Purchases a ticket for the specified raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    /// * `buyer` - The address purchasing the ticket (must be authenticated)
    ///
    /// # Returns
    /// * `u32` - The total number of tickets sold for this raffle after purchase
    ///
    /// # Panics
    /// * If the raffle is inactive
    /// * If the raffle has ended
    /// * If all tickets are sold out
    /// * If multiple tickets are not allowed and buyer already has a ticket
    pub fn buy_ticket(env: Env, raffle_id: u64, buyer: Address) -> u32 {
        buyer.require_auth();
        let mut raffle = read_raffle(&env, raffle_id);
        if !raffle.is_active {
            panic!("raffle_inactive");
        }
        if env.ledger().timestamp() > raffle.end_time {
            panic!("raffle_ended");
        }
        if raffle.tickets_sold >= raffle.max_tickets {
            panic!("tickets_sold_out");
        }

        let current_count = read_ticket_count(&env, raffle_id, &buyer);
        if !raffle.allow_multiple && current_count > 0 {
            panic!("multiple_tickets_not_allowed");
        }

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&buyer, &contract_address, &raffle.ticket_price);

        // Generate next ticket ID
        let ticket_id = next_ticket_id(&env, raffle_id);
        let timestamp = env.ledger().timestamp();

        // Create and store Ticket struct
        let ticket = Ticket {
            id: ticket_id,
            raffle_id,
            buyer: buyer.clone(),
            purchase_time: timestamp,
            ticket_number: raffle.tickets_sold + 1,
        };
        write_ticket(&env, raffle_id, &ticket);

        // Maintain backward compatibility with Vec<Address>
        let mut tickets = read_tickets(&env, raffle_id);
        tickets.push_back(buyer.clone());
        write_tickets(&env, raffle_id, &tickets);

        raffle.tickets_sold += 1;
        write_ticket_count(&env, raffle_id, &buyer, current_count + 1);
        write_raffle(&env, &raffle);

        // Create ticket_ids vector with single ticket ID
        let mut ticket_ids = Vec::new(&env);
        ticket_ids.push_back(ticket_id);

        // Emit TicketPurchased event
        env.events().publish(
            (symbol_short!("TktPurch"),),
            TicketPurchased {
                raffle_id,
                buyer: buyer.clone(),
                ticket_ids,
                quantity: 1u32,
                total_paid: raffle.ticket_price,
                timestamp,
            },
        );

        raffle.tickets_sold
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
    pub fn buy_tickets(env: Env, raffle_id: u64, buyer: Address, quantity: u32) -> u32 {
        buyer.require_auth();

        if quantity == 0 {
            panic!("quantity_zero");
        }

        let mut raffle = read_raffle(&env, raffle_id);
        if !raffle.is_active {
            panic!("raffle_inactive");
        }
        if env.ledger().timestamp() > raffle.end_time {
            panic!("raffle_ended");
        }

        let available_tickets = raffle.max_tickets - raffle.tickets_sold;
        if quantity > available_tickets {
            panic!("insufficient_tickets_available");
        }

        let current_count = read_ticket_count(&env, raffle_id, &buyer);
        if !raffle.allow_multiple {
            if current_count > 0 {
                panic!("multiple_tickets_not_allowed");
            }
            if quantity > 1 {
                panic!("multiple_tickets_not_allowed");
            }
        }

        // Calculate total cost: quantity × ticket_price
        let total_cost = raffle
            .ticket_price
            .checked_mul(quantity as i128)
            .unwrap_or_else(|| panic!("cost_overflow"));

        // Process single payment transfer
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&buyer, &contract_address, &total_cost);

        let timestamp = env.ledger().timestamp();
        let mut ticket_ids = Vec::new(&env);

        // Create individual Ticket structs for each purchase
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

        // Maintain backward compatibility with Vec<Address>
        let mut tickets = read_tickets(&env, raffle_id);
        for _ in 0..quantity {
            tickets.push_back(buyer.clone());
        }
        write_tickets(&env, raffle_id, &tickets);

        // Update raffle state
        raffle.tickets_sold += quantity;
        write_ticket_count(&env, raffle_id, &buyer, current_count + quantity);
        write_raffle(&env, &raffle);

        // Emit TicketPurchased event with all ticket IDs
        env.events().publish(
            (symbol_short!("TktPurch"),),
            TicketPurchased {
                raffle_id,
                buyer: buyer.clone(),
                ticket_ids,
                quantity,
                total_paid: total_cost,
                timestamp,
            },
        );

        raffle.tickets_sold
    }

    /// Finalizes a raffle and selects a winner.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle to finalize
    ///
    /// # Returns
    /// * `Address` - The address of the winner
    ///
    /// # Panics
    /// * If the caller is not the creator
    /// * If the raffle is inactive
    /// * If the raffle has not ended yet
    /// * If no tickets were sold

    pub fn finalize_raffle(env: Env, raffle_id: u64) -> Address {
        let mut raffle = read_raffle(&env, raffle_id);
        raffle.creator.require_auth();
        if !raffle.is_active {
            panic!("raffle_inactive");
        }
        if env.ledger().timestamp() < raffle.end_time {
            panic!("raffle_still_running");
        }
        if raffle.tickets_sold == 0 {
            panic!("no_tickets_sold");
        }

        let tickets = read_tickets(&env, raffle_id);
        let seed = env.ledger().timestamp() + env.ledger().sequence() as u64;
        let winner_index = (seed % tickets.len() as u64) as u32;
        let winner = tickets.get(winner_index).unwrap();

        raffle.is_active = false;
        raffle.winner = Some(winner.clone());
        write_raffle(&env, &raffle);
        remove_active_raffle(&env, raffle_id);
        winner
    }

    /// Claims the prize for the winner of a raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    /// * `winner` - The address of the winner (must be authenticated)
    ///
    /// # Returns
    /// * `i128` - The amount of prize claimed
    ///
    /// # Panics
    /// * If the caller is not the winner
    /// * If the prize has not been deposited
    /// * If the prize has already been claimed
    pub fn claim_prize(env: Env, raffle_id: u64, winner: Address) -> i128 {
        winner.require_auth();
        let mut raffle = read_raffle(&env, raffle_id);
        if raffle.winner != Some(winner.clone()) {
            panic!("not_winner");
        }
        if !raffle.prize_deposited {
            panic!("prize_not_deposited");
        }
        if raffle.prize_claimed {
            panic!("prize_already_claimed");
        }

        let gross_amount = raffle.prize_amount;
        let platform_fee = 0i128;
        let net_amount = gross_amount - platform_fee;
        let claimed_at = env.ledger().timestamp();

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&contract_address, &winner, &net_amount);

        env.events().publish(
            (symbol_short!("prize"), raffle_id),
            (
                winner.clone(),
                gross_amount,
                net_amount,
                platform_fee,
                claimed_at,
            ),
        );

        raffle.prize_claimed = true;
        write_raffle(&env, &raffle);
        net_amount
    }

    /// Retrieves raffle information by ID.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle to retrieve
    ///
    /// # Returns
    /// * `Raffle` - The raffle data structure
    ///
    /// # Panics
    /// * If the raffle does not exist
    pub fn get_raffle(env: Env, raffle_id: u64) -> Raffle {
        read_raffle(&env, raffle_id)
    }

    /// Retrieves raffle information by ID, including aggregated stats.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle to retrieve
    ///
    /// # Returns
    /// * `RaffleWithStats` - The raffle data structure and stats
    ///
    /// # Panics
    /// * If the raffle does not exist
    pub fn get_raffle_by_id(env: Env, raffle_id: u64) -> RaffleWithStats {
        let raffle = read_raffle(&env, raffle_id);
        let stats = build_raffle_stats(&raffle);
        RaffleWithStats { raffle, stats }
    }

    /// Retrieves the number of tickets owned by a user for a raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    /// * `user` - The address of the user
    ///
    /// # Returns
    /// * `u32` - Number of tickets owned by the user
    pub fn get_user_tickets(env: Env, raffle_id: u64, user: Address) -> u32 {
        read_ticket_count(&env, raffle_id, &user)
    }

    /// Retrieves the status for a raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    ///
    /// # Returns
    /// * `RaffleStatus` - Current status of the raffle
    ///
    /// # Panics
    /// * If the raffle does not exist
    pub fn get_raffle_status(env: Env, raffle_id: u64) -> RaffleStatus {
        let raffle = read_raffle(&env, raffle_id);
        build_raffle_status(&raffle)
    }

    /// Retrieves aggregated statistics for a raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    ///
    /// # Returns
    /// * `RaffleStats` - Aggregated statistics for the raffle
    ///
    /// # Panics
    /// * If the raffle does not exist
    /// Retrieves aggregated statistics for a raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    ///
    /// # Returns
    /// * `RaffleStats` - Aggregated statistics for the raffle
    ///
    /// # Panics
    /// * If the raffle does not exist
    pub fn get_raffle_stats(env: Env, raffle_id: u64) -> RaffleStats {
        let raffle = read_raffle(&env, raffle_id);
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

    /// Retrieves all tickets for a raffle as Ticket structs.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    ///
    /// # Returns
    /// * `Vec<Ticket>` - Vector of Ticket structs with full metadata
    pub fn get_tickets(env: Env, raffle_id: u64) -> Vec<Ticket> {
        let raffle = read_raffle(&env, raffle_id);
        let mut tickets = Vec::new(&env);
        
        // Read all tickets based on tickets_sold count
        for ticket_num in 1..=raffle.tickets_sold {
            if let Some(ticket) = read_ticket(&env, raffle_id, ticket_num) {
                tickets.push_back(ticket);
            }
        }
        
        tickets
    }

    /// Retrieves a specific ticket by its ID.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    /// * `ticket_id` - The ID of the ticket to retrieve
    ///
    /// # Returns
    /// * `Option<Ticket>` - The ticket if found, None otherwise
    pub fn get_ticket(env: Env, raffle_id: u64, ticket_id: u32) -> Option<Ticket> {
        read_ticket(&env, raffle_id, ticket_id)
    }

    /// Retrieves all tickets purchased by a specific buyer for a raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    /// * `buyer` - The address of the buyer
    ///
    /// # Returns
    /// * `Vec<Ticket>` - Vector of Ticket structs purchased by the buyer
    pub fn get_tickets_by_buyer(env: Env, raffle_id: u64, buyer: Address) -> Vec<Ticket> {
        let raffle = read_raffle(&env, raffle_id);
        let mut buyer_tickets = Vec::new(&env);
        
        // Iterate through all tickets and filter by buyer
        for ticket_num in 1..=raffle.tickets_sold {
            if let Some(ticket) = read_ticket(&env, raffle_id, ticket_num) {
                if ticket.buyer == buyer {
                    buyer_tickets.push_back(ticket);
                }
            }
        }
        
        buyer_tickets
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
            let raffle = read_raffle(&env, raffle_id);
            if raffle.is_active && raffle.end_time > current_time {
                total_active += 1;
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
            let raffle = read_raffle(&env, raffle_id);

            if raffle.is_active && raffle.end_time > current_time {
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                data.push_back(raffle_id);
                count += 1;
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
}

mod test;
