#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, Env, String, Vec,
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
pub enum DataKey {
    NextRaffleId,
    Raffle(u64),
    Tickets(u64),
    TicketCount(u64, Address),
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

        let prize_amount = raffle.prize_amount;
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&contract_address, &winner, &prize_amount);

        raffle.prize_claimed = true;
        write_raffle(&env, &raffle);
        prize_amount
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
}

mod test;
