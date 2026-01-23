#![no_std]
use core::cmp::min;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Vec,
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

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    NextRaffleId,
    Raffle(u64),
    Tickets(u64),
    TicketCount(u64, Address),
    ActiveRaffles,
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
    let mut active_raffles = read_active_raffles(env);
    let mut new_active = Vec::new(env);
    for i in 0..active_raffles.len() {
        let id = active_raffles.get(i).unwrap();
        if id != raffle_id {
            new_active.push_back(id);
        }
    }
    write_active_raffles(env, &new_active);
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
            creator,
            description,
            end_time,
            max_tickets,
            allow_multiple,
            ticket_price,
            payment_token,
            prize_amount,
            tickets_sold: 0,
            is_active: true,
            prize_deposited: false,
            prize_claimed: false,
            winner: None,
        };
        write_raffle(&env, &raffle);
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

        let mut tickets = read_tickets(&env, raffle_id);
        tickets.push_back(buyer.clone());
        write_tickets(&env, raffle_id, &tickets);

        raffle.tickets_sold += 1;
        write_ticket_count(&env, raffle_id, &buyer, current_count + 1);
        write_raffle(&env, &raffle);

        // Calculate ticket ID (1-indexed)
        let ticket_id = raffle.tickets_sold;
        let quantity = 1u32;
        let total_paid = raffle.ticket_price;
        let timestamp = env.ledger().timestamp();

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
                quantity,
                total_paid,
                timestamp,
            },
        );

        raffle.tickets_sold
    }

    pub fn buy_tickets(env: Env, raffle_id: u64, buyer: Address, quantity: u32) -> u32 {
        buyer.require_auth();
        let mut raffle = read_raffle(&env, raffle_id);
        
        // Validate quantity
        if quantity == 0 {
            panic!("quantity_zero");
        }
        
        if !raffle.is_active {
            panic!("raffle_inactive");
        }
        if env.ledger().timestamp() > raffle.end_time {
            panic!("raffle_ended");
        }
        
        // Check if we have enough tickets available
        let remaining_tickets = raffle.max_tickets - raffle.tickets_sold;
        if quantity > remaining_tickets {
            panic!("insufficient_tickets");
        }

        let current_count = read_ticket_count(&env, raffle_id, &buyer);
        if !raffle.allow_multiple && current_count > 0 {
            panic!("multiple_tickets_not_allowed");
        }

        // Calculate total payment
        let total_payment = raffle.ticket_price
            .checked_mul(quantity as i128)
            .unwrap_or_else(|| panic!("payment_overflow"));

        // Transfer tokens for all tickets
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&buyer, &contract_address, &total_payment);

        // Generate ticket IDs (1-indexed, sequential)
        let start_ticket_id = raffle.tickets_sold + 1;
        let mut ticket_ids = Vec::new(&env);
        for i in 0..quantity {
            ticket_ids.push_back(start_ticket_id + i);
        }

        // Update tickets list (add buyer address for each ticket)
        let mut tickets = read_tickets(&env, raffle_id);
        for _ in 0..quantity {
            tickets.push_back(buyer.clone());
        }
        write_tickets(&env, raffle_id, &tickets);

        // Update raffle state
        raffle.tickets_sold += quantity;
        write_ticket_count(&env, raffle_id, &buyer, current_count + quantity);
        write_raffle(&env, &raffle);

        // Get timestamp
        let timestamp = env.ledger().timestamp();

        // Emit TicketPurchased event with all ticket IDs
        env.events().publish(
            (symbol_short!("TktPurch"),),
            TicketPurchased {
                raffle_id,
                buyer: buyer.clone(),
                ticket_ids,
                quantity,
                total_paid: total_payment,
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

        // Add tickets to storage
        let mut tickets = read_tickets(&env, raffle_id);
        for _ in 0..quantity {
            tickets.push_back(buyer.clone());
        }
        write_tickets(&env, raffle_id, &tickets);

        // Update raffle state
        raffle.tickets_sold += quantity;
        write_ticket_count(&env, raffle_id, &buyer, current_count + quantity);
        write_raffle(&env, &raffle);

        raffle.tickets_sold
    }

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

    /// Retrieves all raffle IDs with pagination.
    ///
    /// Pagination is applied after sorting. `offset` is the index within the
    /// sorted list. `limit` is capped at 100.
    pub fn get_all_raffle_ids(
        env: Env,
        offset: u32,
        limit: u32,
        newest_first: bool,
    ) -> Vec<u64> {
        let total = env
            .storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u64);
        let capped_limit = min(limit, 100u32);
        let mut result = Vec::new(&env);

        if capped_limit == 0 || total == 0 {
            return result;
        }

        let offset_u64 = offset as u64;
        if offset_u64 >= total {
            return result;
        }

        let end = min(offset_u64 + capped_limit as u64, total);
        if newest_first {
            for position in offset_u64..end {
                let raffle_id = total - 1 - position;
                result.push_back(raffle_id);
            }
        } else {
            for raffle_id in offset_u64..end {
                result.push_back(raffle_id);
            }
        }

        result
    }

    /// Retrieves all ticket buyers for a raffle.
    ///
    /// # Arguments
    /// * `raffle_id` - The ID of the raffle
    ///
    /// # Returns
    /// * `Vec<Address>` - Vector of addresses representing ticket buyers
    pub fn get_tickets(env: Env, raffle_id: u64) -> Vec<Address> {
        read_tickets(&env, raffle_id)
    }

    pub fn get_active_raffle_ids(env: Env, offset: u32, limit: u32) -> Vec<u64> {
        let capped_limit = if limit > 100 { 100 } else { limit };
        let all_active = read_active_raffles(&env);
        let current_time = env.ledger().timestamp();
        let mut result = Vec::new(&env);
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
                result.push_back(raffle_id);
                count += 1;
            }
        }

        result
    }
}

mod test;
