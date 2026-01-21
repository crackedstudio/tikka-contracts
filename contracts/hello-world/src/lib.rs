#![no_std]
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

// Requirement: Define TicketPurchased event struct
#[derive(Clone)]
#[contracttype]
pub struct TicketPurchasedEvent {
    pub raffle_id: u64,
    pub buyer: Address,
    pub ticket_ids: Vec<u32>, // Indices of the tickets in the raffle
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
}

// --- Internal Helper Functions ---

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
        token_client.transfer(
            &raffle.creator,
            &env.current_contract_address(),
            &raffle.prize_amount,
        );

        raffle.prize_deposited = true;
        write_raffle(&env, &raffle);
    }

    // Single ticket purchase
    pub fn buy_ticket(env: Env, raffle_id: u64, buyer: Address) -> u32 {
        Self::buy_tickets_batch(env, raffle_id, buyer, 1)
    }

    // Batch ticket purchase
    pub fn buy_tickets_batch(env: Env, raffle_id: u64, buyer: Address, quantity: u32) -> u32 {
        buyer.require_auth();
        if quantity == 0 {
            panic!("quantity_zero");
        }

        let mut raffle = read_raffle(&env, raffle_id);
        let now = env.ledger().timestamp();

        if !raffle.is_active {
            panic!("raffle_inactive");
        }
        if now > raffle.end_time {
            panic!("raffle_ended");
        }
        if raffle.tickets_sold + quantity > raffle.max_tickets {
            panic!("not_enough_tickets_left");
        }

        let current_count = read_ticket_count(&env, raffle_id, &buyer);
        if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
            panic!("multiple_tickets_not_allowed");
        }

        // Handle Payment
        let total_paid = raffle.ticket_price * (quantity as i128);
        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client.transfer(&buyer, &env.current_contract_address(), &total_paid);

        // Record Tickets and Collect IDs for Event
        let mut tickets = read_tickets(&env, raffle_id);
        let mut ticket_ids: Vec<u32> = Vec::new(&env);

        for _ in 0..quantity {
            let new_ticket_id = raffle.tickets_sold;
            tickets.push_back(buyer.clone());
            ticket_ids.push_back(new_ticket_id);
            raffle.tickets_sold += 1;
        }

        // Update State
        write_tickets(&env, raffle_id, &tickets);
        write_ticket_count(&env, raffle_id, &buyer, current_count + quantity);
        write_raffle(&env, &raffle);

        // Requirement: Emit Event
        env.events().publish(
            (symbol_short!("ticket"), symbol_short!("purchased")),
            TicketPurchasedEvent {
                raffle_id,
                buyer: buyer.clone(),
                ticket_ids,
                quantity,
                total_paid,
                timestamp: now,
            },
        );

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
        winner
    }

    pub fn claim_prize(env: Env, raffle_id: u64, winner: Address) {
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

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client.transfer(
            &env.current_contract_address(),
            &winner,
            &raffle.prize_amount,
        );

        raffle.prize_claimed = true;
        write_raffle(&env, &raffle);
    }

    pub fn get_raffle(env: Env, raffle_id: u64) -> Raffle {
        read_raffle(&env, raffle_id)
    }
    pub fn get_tickets(env: Env, raffle_id: u64) -> Vec<Address> {
        read_tickets(&env, raffle_id)
    }
}

mod test;
