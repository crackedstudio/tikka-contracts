// Instance submodule
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env,
    String, Symbol, Vec,
};

use crate::events::{
    DrawTriggered, PrizeClaimed, PrizeDeposited, RaffleCancelled, RaffleCreated,
    RaffleFinalized, RandomnessReceived, RandomnessRequested, StatusChanged, TicketPurchased,
};

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
    pub tickets_sold: u32,
    pub status: RaffleStatus,
    pub prize_deposited: bool,
    pub winner: Option<Address>,
    pub randomness_source: RandomnessSource,
    pub oracle_address: Option<Address>,
    pub protocol_fee_bp: u32,
    pub treasury_address: Option<Address>,
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
    pub randomness_source: RandomnessSource,
    pub oracle_address: Option<Address>,
    pub protocol_fee_bp: u32,
    pub treasury_address: Option<Address>,
}

#[derive(Clone)]
#[contracttype]
pub struct Ticket {
    pub id: u32,
    pub buyer: Address,
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
}

// --- Error Types ---

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Error {
    RaffleNotFound = 1,
    RaffleInactive = 2,
    TicketsSoldOut = 3,
    InsufficientPayment = 4,
    NotAuthorized = 5,
    PrizeNotDeposited = 6,
    PrizeAlreadyClaimed = 7,
    InvalidParameters = 8,
    ContractPaused = 9,
    InsufficientTickets = 10,
    RaffleEnded = 11,
    RaffleStillRunning = 12,
    NoTicketsSold = 13,
    MultipleTicketsNotAllowed = 14,
    PrizeAlreadyDeposited = 15,
    NotWinner = 16,
    ArithmeticOverflow = 17,
    AlreadyInitialized = 18,
    NotInitialized = 19,
    InvalidStateTransition = 20,
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

fn read_tickets(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Tickets)
        .unwrap_or_else(|| Vec::new(env))
}

fn write_tickets(env: &Env, tickets: &Vec<Address>) {
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

#[contractimpl]
impl Contract {
    pub fn init(
        env: Env,
        factory: Address,
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
            tickets_sold: 0,
            status: RaffleStatus::Proposed,
            prize_deposited: false,
            winner: None,
            randomness_source: config.randomness_source.clone(),
            oracle_address: config.oracle_address,
            protocol_fee_bp: config.protocol_fee_bp,
            treasury_address: config.treasury_address,
        };
        write_raffle(&env, &raffle);
        env.storage().instance().set(&DataKey::Factory, &factory);

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
                description: config.description,
                randomness_source: config.randomness_source,
            },
        );

        Ok(())
    }

    pub fn deposit_prize(env: Env) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.status != RaffleStatus::Proposed {
            return Err(Error::InvalidStateTransition);
        }
        if raffle.prize_deposited {
            return Err(Error::PrizeAlreadyDeposited);
        }

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&raffle.creator, &contract_address, &raffle.prize_amount);

        raffle.prize_deposited = true;
        raffle.status = RaffleStatus::Active;
        write_raffle(&env, &raffle);

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

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&buyer, &contract_address, &raffle.ticket_price);

        let ticket_id = next_ticket_id(&env);
        let timestamp = env.ledger().timestamp();

        let ticket = Ticket {
            id: ticket_id,
            buyer: buyer.clone(),
            purchase_time: timestamp,
            ticket_number: raffle.tickets_sold + 1,
        };
        write_ticket(&env, &ticket);

        let mut tickets = read_tickets(&env);
        tickets.push_back(buyer.clone());
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
                .expect("Oracle missing")
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
        let seed = env.ledger().timestamp() + env.ledger().sequence() as u64;
        let winner_index = (seed % tickets.len() as u64) as u32;
        let winner = tickets.get(winner_index).expect("Ticket out of bounds");

        raffle.status = RaffleStatus::Finalized;
        raffle.winner = Some(winner.clone());
        write_raffle(&env, &raffle);

        publish_event(
            &env,
            "raffle_finalized",
            RaffleFinalized {
                winner: winner.clone(),
                winning_ticket_id: winner_index,
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
        let winner_index = (random_seed % tickets.len() as u64) as u32;
        let winner = tickets
            .get(winner_index)
            .expect("Ticket out of bounds callback");

        raffle.status = RaffleStatus::Finalized;
        raffle.winner = Some(winner.clone());
        write_raffle(&env, &raffle);

        publish_event(
            &env,
            "randomness_received",
            RandomnessReceived {
                oracle: raffle.oracle_address.clone().unwrap(),
                seed: random_seed,
                timestamp: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "raffle_finalized",
            RaffleFinalized {
                winner: winner.clone(),
                winning_ticket_id: winner_index,
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

        Ok(winner)
    }

    pub fn claim_prize(env: Env, winner: Address) -> Result<i128, Error> {
        winner.require_auth();
        let mut raffle = read_raffle(&env)?;

        if raffle.status != RaffleStatus::Finalized {
            return Err(Error::InvalidStateTransition);
        }
        if raffle.winner != Some(winner.clone()) {
            return Err(Error::NotWinner);
        }
        if !raffle.prize_deposited {
            return Err(Error::PrizeNotDeposited);
        }

        let mut platform_fee = 0i128;
        if raffle.protocol_fee_bp > 0 {
            platform_fee = (raffle.prize_amount * raffle.protocol_fee_bp as i128) / 10000;
        }
        let net_amount = raffle.prize_amount - platform_fee;
        let claimed_at = env.ledger().timestamp();

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();

        // Transfer net prize to winner
        token_client.transfer(&contract_address, &winner, &net_amount);

        // Transfer fee to treasury if applicable
        if platform_fee > 0 && raffle.treasury_address.is_some() {
            token_client.transfer(
                &contract_address,
                &raffle.treasury_address.clone().unwrap(),
                &platform_fee,
            );
        }

        raffle.status = RaffleStatus::Claimed;
        write_raffle(&env, &raffle);

        publish_event(
            &env,
            "prize_claimed",
            PrizeClaimed {
                winner: winner.clone(),
                gross_amount: raffle.prize_amount,
                net_amount,
                platform_fee,
                claimed_at,
            },
        );

        publish_event(
            &env,
            "status_changed",
            StatusChanged {
                old_status: RaffleStatus::Finalized,
                new_status: RaffleStatus::Claimed,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(net_amount)
    }

    pub fn cancel_raffle(env: Env) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.status == RaffleStatus::Finalized
            || raffle.status == RaffleStatus::Claimed
            || raffle.status == RaffleStatus::Cancelled
        {
            return Err(Error::InvalidStateTransition);
        }

        let old_status = raffle.status.clone();
        raffle.status = RaffleStatus::Cancelled;

        if raffle.prize_deposited {
            let token_client = token::Client::new(&env, &raffle.payment_token);
            let contract_address = env.current_contract_address();
            token_client.transfer(&contract_address, &raffle.creator, &raffle.prize_amount);
            raffle.prize_deposited = false;
        }

        write_raffle(&env, &raffle);

        publish_event(
            &env,
            "raffle_cancelled",
            RaffleCancelled {
                creator: raffle.creator.clone(),
                reason: String::from_str(&env, "Creator cancelled"),
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

    pub fn get_raffle(env: Env) -> Result<Raffle, Error> {
        read_raffle(&env)
    }
}

#[cfg(test)]
mod test;
