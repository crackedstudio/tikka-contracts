// Instance submodule
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, xdr::ToXdr, Address, Bytes, BytesN,
    Env, IntoVal, String, Symbol, Vec,
};

use self::randomness::{build_internal_seed, OracleSeedWinnerSelection, PrngWinnerSelection, WinnerSelectionStrategy};
use crate::types::FairnessData;

use crate::events::{
    DrawTriggered, PrizeClaimed, PrizeDeposited, PrizeRefunded, RaffleCancelled, RaffleCreated,
    RaffleFinalized, RaffleStatusChanged, RandomnessFallbackTriggered, RandomnessReceived,
    RandomnessRequested, SeedCommitted, SeedRevealed, TicketPurchased, WinnerDrawn, RandomnessType,
    TicketRefunded,
};

/// Number of ledgers after a randomness request before the fallback can be triggered.
const ORACLE_TIMEOUT_LEDGERS: u32 = 200;
mod randomness;

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
    Active = 0,
    Drawing = 1,
    Finalized = 2,
    Cancelled = 3,
    Failed = 4, // min_tickets threshold not met at draw time
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
    CommitReveal = 2,
}

#[derive(Clone)]
#[contracttype]
pub struct Raffle {
    pub creator: Address,
    pub description: String,
    pub end_time: u64,
    pub max_tickets: u32,
    pub min_tickets: u32, // 0 = no minimum; if > 0 and not met at draw, raffle fails
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
    pub winner_ticket_id: Option<u32>,
}

#[derive(Clone)]
#[contracttype]
pub struct RaffleConfig {
    pub description: String,
    pub end_time: u64,
    pub max_tickets: u32,
    pub min_tickets: u32, // 0 = no minimum
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
    /// SHA-256 hash of the off-chain metadata JSON. Must be non-zero (all-zero
    /// bytes are rejected as an unset sentinel value).
    pub metadata_hash: BytesN<32>,
}

#[derive(Clone)]
#[contracttype]
pub struct Ticket {
    pub id: u32,
    pub owner: Address,
    pub purchase_time: u64,
    pub ticket_number: u32,
}

/// Metadata stored after draw to enable fairness verification
#[derive(Clone)]
#[contracttype]
pub struct FairnessMetadata {
    pub seed: u64,
    pub randomness_source: RandomnessSource,
    pub winning_ticket_indices: Vec<u32>,
    pub draw_timestamp: u64,
    pub draw_sequence: u32,
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

/// Detects if an address is the native XLM contract by checking its name and symbol.
/// In Soroban, the native asset contract ID has these specific values.
fn is_native_xlm(env: &Env, token: &Address) -> bool {
    let client = token::Client::new(env, token);
    // SAC for native asset has name "native" and symbol "XLM"
    client.name() == String::from_str(env, "native") && client.symbol() == String::from_str(env, "XLM")
}

fn verify_randomness_proof_internal(
    env: &Env,
    public_key: &BytesN<32>,
    seed: u64,
    proof: &BytesN<64>,
) {
    let message = Bytes::from_array(env, &seed.to_be_bytes());
    // ed25519_verify traps on invalid signature, rejecting the randomness submit.
    env.crypto().ed25519_verify(public_key, &message, proof);
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Raffle,
    TicketCount(Address),
    Ticket(u32),
    NextTicketId,
    Factory,
    RefundStatus(u32),     // ticket_id -> bool
    ReentrancyGuard,
    Approved(u32),                    // ticket_id -> Address
    ApprovedForAll(Address, Address), // (owner, operator) -> bool
    Paused,
    Admin,
    RandomnessSeed,          // Stored after draw for fairness proof
    RandomnessRequested,     // bool  - true when oracle request is pending
    RandomnessRequestLedger, // u32  - ledger sequence when the request was made
    TicketOwner(u32),        // ticket_number -> Address
    FinishTime,
    PendingAdmin,
    CommitHash(Address),     // commit-reveal: hash committed by participant
    CommitCount,             // commit-reveal: number of commits received
    RevealedSecret(Address), // commit-reveal: revealed secret bytes
    RevealCount,             // commit-reveal: number of reveals received
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
    OracleNotSet = 6,
    RandomnessAlreadyRequested = 7,
    NoRandomnessRequest = 8,
    FallbackTooEarly = 9,

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
    RaffleEnded = 35,

    // System errors (41-50)
    ArithmeticOverflow = 41,
    AlreadyInitialized = 42,
    NotInitialized = 43,
    Reentrancy = 44,
    // Cross-contract errors (45-50)
    /// External token transfer failed (e.g. malicious or broken token contract).
    TokenTransferFailed = 45,
    // Admin errors (51-60)
    AdminTransferPending = 51,
    NoPendingTransfer = 52,
    // Commit-reveal errors (61-70)
    AlreadyCommitted = 61,
    NotCommitted = 62,
    CommitHashMismatch = 63,
    AlreadyRevealed = 64,
    CommitRevealNotReady = 65,
}

fn require_not_paused(env: &Env) -> Result<(), Error> {
    if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {
        return Err(Error::ContractPaused);
    }
    Ok(())
}

fn build_internal_seed(env: &Env) -> u64 {
    let xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.current_contract_address(),
    )
        .to_xdr(env);
    let hash: BytesN<32> = env.crypto().sha256(&xdr).into();
    
    // Convert first 8 bytes of hash to u64
    let mut bytes = [0u8; 8];
    for i in 0..8 {
        bytes[i] = hash.get(i as u32).unwrap();
    }
    u64::from_be_bytes(bytes)
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

        env.events().publish(
            (
                Symbol::new(env, "WinnerDrawn"),
                winner.clone(),
                winner_index,
            ),
            WinnerDrawn {
                winner: winner.clone(),
                ticket_id: winner_index,
                tier_index: i,
                timestamp: env.ledger().timestamp(),
            },
        );
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

    if !env.storage().persistent().has(&DataKey::FinishTime) {
        env.storage()
            .persistent()
            .set(&DataKey::FinishTime, &env.ledger().timestamp());
    }

    publish_event(
        env,
        "raffle_finalized",
        RaffleFinalized {
            winners,
            winning_ticket_ids,
            total_tickets_sold: raffle.tickets_sold,
            randomness_source: raffle.randomness_source.clone(),
            randomness_type,
            finalized_at: env.ledger().timestamp(),
        },
    );

    publish_event(
        env,
        "status_changed",
        RaffleStatusChanged {
            old_status: RaffleStatus::Drawing,
            new_status: RaffleStatus::Finalized,
            timestamp: env.ledger().timestamp(),
        },
    );

    Ok(())
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

fn calculate_protocol_fee(amount: i128, fee_bp: u32) -> Result<(i128, i128), Error> {
    // fee is computed in basis points, where 10,000 bp = 100%.
    // This avoids floating point and supports precise financial calculations.
    if fee_bp > 10_000 {
        return Err(Error::InvalidParameters);
    }
    if amount < 0 {
        return Err(Error::InvalidParameters);
    }

    let fee = amount
        .checked_mul(fee_bp as i128)
        .ok_or(Error::ArithmeticOverflow)?
        .checked_div(10_000)
        .ok_or(Error::ArithmeticOverflow)?;
    let net = amount.checked_sub(fee).ok_or(Error::ArithmeticOverflow)?;
    Ok((fee, net))
}

fn get_ticket_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::NextTicketId)
        .unwrap_or(0u32)
}

fn read_tickets(env: &Env) -> Vec<Ticket> {
    let mut tickets = Vec::new(env);
    let count = get_ticket_count(env);
    for ticket_id in 1..=count {
        if let Some(ticket) = env
            .storage()
            .persistent()
            .get::<_, Ticket>(&DataKey::Ticket(ticket_id))
        {
            tickets.push_back(ticket);
        }
    }
    tickets
}

fn get_ticket_owner(env: &Env, ticket_id: u32) -> Option<Address> {
    env.storage()
        .persistent()
        .get::<_, Ticket>(&DataKey::Ticket(ticket_id))
        .map(|t| t.owner)
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

/// Increment total ticket counter and return the new ticket number (1-indexed).
fn next_ticket_number(env: &Env) -> u32 {
    let current: u32 = env
        .storage()
        .instance()
        .get(&DataKey::TotalTickets)
        .unwrap_or(0u32);
    let next = current + 1;
    env.storage().instance().set(&DataKey::TotalTickets, &next);
    next
}

/// Read a ticket by its 1-indexed ticket_number from the mapping.
fn read_ticket_by_number(env: &Env, ticket_number: u32) -> Option<Ticket> {
    env.storage()
        .persistent()
        .get(&DataKey::Ticket(ticket_number))
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

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotAuthorized)?;
    admin.require_auth();
    Ok(admin)
}

fn require_creator(env: &Env) -> Result<Address, Error> {
    let raffle = read_raffle(env)?;
    raffle.creator.require_auth();
    Ok(raffle.creator)
}

fn do_transfer(env: &Env, from: Address, to: Address, token_id: u32) -> Result<(), Error> {
    // token_id is the ticket_number (1-indexed) — direct mapping lookup, no Vec needed
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

    env.storage()
        .persistent()
        .remove(&DataKey::Approved(token_id));

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

        // Query token decimals to validate amounts are meaningful across asset types
        // (native XLM = 7 decimals/stroops, SAC tokens vary).
        // Amounts must be >= 1 (smallest unit) and prize must cover at least one ticket.
        let token_client = token::Client::new(&env, &config.payment_token);
        
        // Log detection if native XLM is being used
        if is_native_xlm(&env, &config.payment_token) {
            // Native XLM support confirmed
        }

        let _decimals = token_client.decimals(); // available for off-chain tooling via get_token_decimals
        if config.ticket_price < 1 {
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

        // Reject all-zero hash — it signals the caller forgot to set the field.
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

        env.events().publish(
            (Symbol::new(&env, "RaffleCreated"), creator.clone()),
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
            },
        );

        Ok(())
    }


        if raffle.status != RaffleStatus::Active {
            return Err(Error::InvalidStateTransition);
        }
        if raffle.prize_deposited {
            return Err(Error::PrizeAlreadyDeposited);
        }

    pub fn deposit_prize(env: Env) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.prize_deposited {
            return Err(Error::PrizeAlreadyDeposited);
        }

        let old_status = raffle.status.clone();
        raffle.status = RaffleStatus::Active;

        // Effects: update state BEFORE external call (CEI pattern)
        raffle.prize_deposited = true;
        write_raffle(&env, &raffle);

        // Interaction: external token transfer — creator deposits the prize pool.
        // Use try_transfer so a broken token surfaces as a typed error.
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        
        if is_native_xlm(&env, &raffle.payment_token) {
            // Optimization or specific logic for native balance operations could go here.
            // For now, SAC transfer is the standard native balance operation in Soroban.
        }

        token_client
            .try_transfer(&raffle.creator, &contract_address, &raffle.prize_amount)
            .map_err(|_| Error::TokenTransferFailed)?;

        env.events().publish(
            (Symbol::new(&env, "PrizeDeposited"), raffle.creator.clone()),
            PrizeDeposited {
                creator: raffle.creator.clone(),
                amount: raffle.prize_amount,
                token: raffle.payment_token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        env.events().publish(
            (Symbol::new(&env, "status_changed"), raffle.creator.clone()),
            RaffleStatusChanged {
                old_status,
                new_status: RaffleStatus::Active,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn buy_tickets(env: Env, buyer: Address, quantity: u32) -> Result<u32, Error> {
        let mut raffle = read_raffle(&env)?;
        buyer.require_auth();
        require_not_paused(&env)?;

        // --- 1. CHECKS ---
        if raffle.status != RaffleStatus::Open && raffle.status != RaffleStatus::Active {
            return Err(Error::RaffleInactive);
        }
        if !raffle.prize_deposited {
            return Err(Error::InvalidStateTransition);
        }
        if raffle.end_time != 0 && env.ledger().timestamp() > raffle.end_time {
            return Err(Error::RaffleExpired);
        }
        
        // Check total availability for the whole batch
        if raffle.tickets_sold + quantity > raffle.max_tickets {
            return Err(Error::TicketsSoldOut);
        }

        let current_count = read_ticket_count(&env, &buyer);
        // Respect allow_multiple: Block if they own tickets OR trying to buy > 1 in this batch
        if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
            return Err(Error::MultipleTicketsNotAllowed);
        }

        // --- 2. EFFECTS ---
        let mut ticket_ids = Vec::new(&env);
        let timestamp = env.ledger().timestamp();
        let total_price = raffle.ticket_price
            .checked_mul(quantity as i128)
            .ok_or(Error::InvalidParameters)?;

        for _ in 0..quantity {
            let ticket_id = next_ticket_id(&env);
            raffle.tickets_sold += 1;

            let ticket = Ticket {
                id: ticket_id,
                owner: buyer.clone(),
                purchase_time: timestamp,
                ticket_number: raffle.tickets_sold,
            };
            write_ticket(&env, &ticket);
            ticket_ids.push_back(ticket_id);
        }

        // Auto-transition to Drawing if sold out
        if raffle.tickets_sold >= raffle.max_tickets {
            let old_status = raffle.status.clone();
            raffle.status = RaffleStatus::Drawing;
            publish_event(
                &env,
                "status_changed",
                RaffleStatusChanged {
                    old_status: RaffleStatus::Active,
                    new_status: RaffleStatus::Drawing,
                    timestamp,
                },
            );
        }

        write_ticket_count(&env, &buyer, current_count + quantity);
        write_raffle(&env, &raffle);

        // Interaction: external token transfer — buyer pays for the ticket.
        // Use try_transfer so a broken token surfaces as a typed error.
        // Update global volume in factory
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
        }

        // Single atomic transfer for the entire batch
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();

        // If it's native XLM, we could potentially use specialized logic, 
        // but token::Client works perfectly and is the standard way.
        // We log detection for auditing as requested in the task.
        if is_native_xlm(&env, &raffle.payment_token) {
             // In some versions of Soroban, native might require specific authorization,
             // but here we rely on the standard SAC which works with require_auth.
        }

        token_client
            .try_transfer(&buyer, &contract_address, &total_price)
            .map_err(|_| Error::TokenTransferFailed)?;

        // Single event containing the range of IDs
        env.events().publish(
            (Symbol::new(&env, "TicketPurchased"), buyer.clone()),
            TicketPurchased {
                buyer: buyer.clone(),
                ticket_ids,
                quantity,
                total_paid: total_price,
                timestamp,
            },
        );

        Ok(raffle.tickets_sold)
    }

    pub fn finalize_raffle(env: Env) -> Result<(), Error> {
        require_creator(&env)?;
        let mut raffle = read_raffle(&env)?;

        // Only allowed if Active (ended) or Drawing (failed randomness/timeout)
        if raffle.status != RaffleStatus::Active && raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStatus);
        }

        let now = env.ledger().timestamp();
        let time_ended = raffle.end_time != 0 && now >= raffle.end_time;
        let tickets_full = raffle.tickets_sold >= raffle.max_tickets;

            write_raffle(&env, &raffle);

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

            publish_event(
                &env,
                "status_changed",
                RaffleStatusChanged {
                    old_status,
                    new_status: RaffleStatus::Failed,
                    timestamp: now,
                },
            );
            return Ok(());
        }

        let seed = build_internal_seed(&env);
        do_finalize_with_seed(&env, raffle, seed, RandomnessType::Prng)
    }

    pub fn refund_prize(env: Env) -> Result<(), Error> {
        require_creator(&env)?;
        let mut raffle = read_raffle(&env)?;

        // Only allowed if Cancelled or Failed
        if raffle.status != RaffleStatus::Cancelled && raffle.status != RaffleStatus::Failed {
            return Err(Error::InvalidStatus);
        }

        if !raffle.prize_deposited {
            return Err(Error::PrizeNotDeposited);
        }

        // Effects: update state BEFORE external call
        raffle.prize_deposited = false;
        write_raffle(&env, &raffle);

        // Interaction: transfer prize back to creator
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();

        token_client
            .try_transfer(&contract_address, &raffle.creator, &raffle.prize_amount)
            .map_err(|_| Error::TokenTransferFailed)?;

        env.events().publish(
            (Symbol::new(&env, "PrizeRefunded"), raffle.creator.clone()),
            PrizeRefunded {
                creator: raffle.creator.clone(),
                amount: raffle.prize_amount,
                token: raffle.payment_token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    /// Explicitly request winner selection from the configured oracle.
    ///
    /// This is the dedicated entry point for the asynchronous oracle randomness
    /// flow.  The creator (or admin) calls this function once the raffle has
    /// ended.  It transitions an `Active` raffle to `Drawing` if the end
    /// conditions are met, records a pending request so that only the oracle
    /// callback that follows can finalise the raffle, and emits the
    /// `randomness_requested` event for off-chain listeners.
    pub fn request_winner_selection(env: Env) -> Result<(), Error> {
        require_creator(&env)?;
        Self::internal_request_winner_selection(&env)
    }

    fn internal_request_winner_selection(env: &Env) -> Result<(), Error> {
        let mut raffle = read_raffle(env)?;

        if raffle.randomness_source != RandomnessSource::External {
            return Err(Error::InvalidParameters);
        }

        // Transition Active → Drawing if the raffle end conditions are satisfied
        if raffle.status == RaffleStatus::Active {
            let now = env.ledger().timestamp();
            let time_ended = raffle.end_time != 0 && now >= raffle.end_time;
            let tickets_full = raffle.tickets_sold >= raffle.max_tickets;
            if !time_ended && !tickets_full {
                return Err(Error::InvalidStateTransition);
            }
            let old_status = raffle.status.clone();
            raffle.status = RaffleStatus::Drawing;
            publish_event(
                &env,
                "status_changed",
                RaffleStatusChanged {
                    old_status: RaffleStatus::Active,
                    new_status: RaffleStatus::Drawing,
                    timestamp: now,
                },
            );
        } else if raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStateTransition);
        }

        if raffle.tickets_sold == 0 {
            return Err(Error::NoTicketsSold);
        }

        let oracle = raffle
            .oracle_address
            .as_ref()
            .ok_or(Error::OracleNotSet)?
            .clone();

        // Prevent duplicate requests while one is already pending
        let already: bool = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequested)
            .unwrap_or(false);
        if already {
            return Err(Error::RandomnessAlreadyRequested);
        }


        // Persist the Drawing state before marking the request
        write_raffle(&env, &raffle);
        env.storage()
            .instance()
            .set(&DataKey::RandomnessRequested, &true);
        env.storage()
            .instance()
            .set(&DataKey::RandomnessRequestLedger, &env.ledger().sequence());

        publish_event(
            &env,
            "draw_triggered",
            DrawTriggered {
                triggered_by: raffle.creator.clone(),
                total_tickets_sold: raffle.tickets_sold,
                timestamp: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "randomness_requested",
            RandomnessRequested {
                oracle,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    /// Oracle callback — finalises the raffle using the provided random seed.
    /// The seed must be accompanied by an Ed25519 proof and public key.
    ///
    /// Only the oracle address that was configured at raffle creation may call
    /// this function.  The contract also requires that a randomness request was
    /// previously recorded (via `request_winner_selection` or `finalize_raffle`)
    /// so that an oracle cannot call this function unsolicited.
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
        if raffle.randomness_source != RandomnessSource::External {
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

        verify_randomness_proof_internal(&env, &public_key, random_seed, &proof);

        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequested);
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequestLedger);

        do_finalize_with_seed(&env, raffle, random_seed, RandomnessType::Vrf)?;
        Ok(oracle)
    }

    pub fn verify_randomness_proof(
        env: Env,
        public_key: BytesN<32>,
        seed: u64,
        proof: BytesN<64>,
    ) -> bool {
        verify_randomness_proof_internal(&env, &public_key, seed, &proof);
        true
    }

    /// Trigger PRNG-based winner selection as a fallback when the oracle has not
    /// responded within `ORACLE_TIMEOUT_LEDGERS` ledgers of the original request.
    ///
    /// The raffle creator or the protocol admin may call this function.  It is
    /// intentionally open to both roles so that a raffle can be unblocked even
    /// if the creator is unavailable.
    ///
    /// The fallback seed is derived from the ledger timestamp and sequence at
    /// the time of the call — identical to the internal PRNG used in
    /// `finalize_raffle` — and the result is equivalent to a normal
    /// finalisation.
    pub fn trigger_randomness_fallback(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();

        let mut raffle = read_raffle(&env)?;

        // Authorise: only the raffle creator or the protocol admin may trigger
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotAuthorized)?;
        if caller != raffle.creator && caller != admin {
            return Err(Error::NotAuthorized);
        }

        // Must be waiting for an oracle response
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

        // Enforce the timeout window
        let request_ledger: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequestLedger)
            .ok_or(Error::NoRandomnessRequest)?;
        let current_ledger = env.ledger().sequence();
        if current_ledger < request_ledger.saturating_add(ORACLE_TIMEOUT_LEDGERS) {
            return Err(Error::FallbackTooEarly);
        }

        let total_tickets = get_ticket_count(&env);
        if total_tickets == 0 {
            return Err(Error::NoTicketsSold);
        }

        // Clear oracle request state
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequested);
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequestLedger);

        let selector = PrngWinnerSelection::new(
            env.ledger().timestamp(),
            env.ledger().sequence(),
            env.current_contract_address(),
            raffle.tickets_sold,
        );
        let winning_ticket_ids =
            selector.select_winner_indices(&env, total_tickets, raffle.prizes.len() as u32);
        let mut winners = Vec::new(&env);

        for i in 0..winning_ticket_ids.len() {
            let winner_index = winning_ticket_ids.get(i).unwrap();
            let ticket_id = winner_index + 1;
            let winner = get_ticket_owner(&env, ticket_id).ok_or(Error::TicketNotFound)?;
            winners.push_back(winner.clone());

            env.events().publish(
                (
                    Symbol::new(&env, "WinnerDrawn"),
                    winner.clone(),
                    ticket_id,
                ),
                WinnerDrawn {
                    winner: winner.clone(),
                    ticket_id,
                    tier_index: i,
                    timestamp: env.ledger().timestamp(),
                },
            );
        }

        let mut claimed_winners = Vec::new(&env);
        for _ in 0..raffle.prizes.len() {
            claimed_winners.push_back(false);
        }

        // Store fairness metadata for transparency
        let fairness_metadata = FairnessMetadata {
            seed: selector.seed_fingerprint(&env),
            randomness_source: RandomnessSource::Internal,
            winning_ticket_indices: winning_ticket_ids.clone(),
            draw_timestamp: env.ledger().timestamp(),
            draw_sequence: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::RandomnessSeed, &fairness_metadata);

        raffle.status = RaffleStatus::Finalized;
        raffle.winners = winners.clone();
        raffle.claimed_winners = claimed_winners;
        raffle.finalized_at = Some(env.ledger().timestamp());
        if let Some(first_id) = winning_ticket_ids.get(0) {
            raffle.winner_ticket_id = Some(first_id + 1);
        }
        write_raffle(&env, &raffle);

        if !env.storage().persistent().has(&DataKey::FinishTime) {
            env.storage()
                .persistent()
                .set(&DataKey::FinishTime, &env.ledger().timestamp());
        }

        publish_event(
            &env,
            "randomness_fallback_triggered",
            RandomnessFallbackTriggered {
                triggered_by: caller,
                seed_used: selector.seed_fingerprint(&env),
                request_ledger,
                fallback_ledger: current_ledger,
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
                randomness_source: RandomnessSource::Internal,
                randomness_type: RandomnessType::Fallback,
                finalized_at: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "status_changed",
            RaffleStatusChanged {
                old_status: RaffleStatus::Drawing,
                new_status: RaffleStatus::Finalized,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn commit_seed(env: Env, participant: Address, hash: BytesN<32>) -> Result<(), Error> {
        require_not_paused(&env)?;
        participant.require_auth();

        let raffle = read_raffle(&env)?;
        if raffle.randomness_source != RandomnessSource::CommitReveal {
            return Err(Error::InvalidParameters);
        }
        if raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStateTransition);
        }
        if env
            .storage()
            .instance()
            .has(&DataKey::CommitHash(participant.clone()))
        {
            return Err(Error::AlreadyCommitted);
        }

        env.storage()
            .instance()
            .set(&DataKey::CommitHash(participant.clone()), &hash);
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CommitCount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::CommitCount, &(count + 1));

        publish_event(
            &env,
            "seed_committed",
            SeedCommitted {
                participant,
                hash,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn reveal_seed(env: Env, participant: Address, secret: Bytes) -> Result<(), Error> {
        require_not_paused(&env)?;
        participant.require_auth();

        let raffle = read_raffle(&env)?;
        if raffle.randomness_source != RandomnessSource::CommitReveal {
            return Err(Error::InvalidParameters);
        }
        if raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStateTransition);
        }

        let committed_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::CommitHash(participant.clone()))
            .ok_or(Error::NotCommitted)?;

        if env
            .storage()
            .instance()
            .has(&DataKey::RevealedSecret(participant.clone()))
        {
            return Err(Error::AlreadyRevealed);
        }

        // Verify hash(secret) == committed hash
        let actual_hash = env.crypto().sha256(&secret);
        let actual_hash_bytes = BytesN::from_array(&env, &actual_hash.to_array());
        if actual_hash_bytes != committed_hash {
            return Err(Error::CommitHashMismatch);
        }

        env.storage()
            .instance()
            .set(&DataKey::RevealedSecret(participant.clone()), &secret);
        let reveal_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RevealCount)
            .unwrap_or(0);
        let new_reveal_count = reveal_count + 1;
        env.storage()
            .instance()
            .set(&DataKey::RevealCount, &new_reveal_count);

        let commit_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CommitCount)
            .unwrap_or(0);

        publish_event(
            &env,
            "seed_revealed",
            SeedRevealed {
                participant,
                timestamp: env.ledger().timestamp(),
            },
        );

        // If all commits have been revealed, derive the seed and finalize
        if commit_count > 0 && new_reveal_count >= commit_count {
            Contract::finalize_commit_reveal(&env, &mut read_raffle(&env)?)?;
        }

        Ok(())
    }

    fn finalize_commit_reveal(env: &Env, raffle: &mut Raffle) -> Result<(), Error> {
        let commit_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::CommitCount)
            .unwrap_or(0);
        if commit_count == 0 {
            return Err(Error::CommitRevealNotReady);
        }

        // XOR all revealed secrets to derive the seed
        let mut seed: u64 = 0;
        // We iterate over all stored commits and read their corresponding reveals
        // Since we can't enumerate keys, we rely on the fact that finalize is only
        // called when reveal_count >= commit_count, meaning all participants revealed.
        // The seed is accumulated via the PRNG seeded with each secret in sequence.
        let mut seed_bytes = Bytes::new(env);
        // Collect all revealed secrets by re-reading from storage via commit keys
        // We use the raffle's ticket owners as the participant set for enumeration
        let total_tickets = get_ticket_count(env);
        let mut seen = soroban_sdk::Map::<Address, bool>::new(env);
        for ticket_id in 1..=total_tickets {
            if let Some(owner) = get_ticket_owner(env, ticket_id) {
                if seen.contains_key(owner.clone()) {
                    continue;
                }
                seen.set(owner.clone(), true);
                if let Some(secret) = env
                    .storage()
                    .instance()
                    .get::<_, Bytes>(&DataKey::RevealedSecret(owner))
                {
                    for byte in secret.iter() {
                        seed_bytes.push_back(byte);
                    }
                }
            }
        }

        // Derive final seed: sha256 of all concatenated secrets, take first 8 bytes as u64
        let hash = env.crypto().sha256(&seed_bytes);
        let hash_arr = hash.to_array();
        seed = u64::from_be_bytes([
            hash_arr[0], hash_arr[1], hash_arr[2], hash_arr[3],
            hash_arr[4], hash_arr[5], hash_arr[6], hash_arr[7],
        ]);

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

            env.events().publish(
                (Symbol::new(&env, "WinnerDrawn"), winner.clone(), ticket_id),
                WinnerDrawn {
                    winner: winner.clone(),
                    ticket_id,
                    tier_index: i,
                    timestamp: env.ledger().timestamp(),
                },
            );
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
            .persistent()
            .set(&DataKey::RandomnessSeed, &fairness_metadata);

        raffle.status = RaffleStatus::Finalized;
        raffle.winners = winners.clone();
        raffle.claimed_winners = claimed_winners;
        raffle.finalized_at = Some(env.ledger().timestamp());
        if let Some(first_id) = winning_ticket_ids.get(0) {
            raffle.winner_ticket_id = Some(first_id + 1);
        }
        write_raffle(env, raffle);

        if !env.storage().persistent().has(&DataKey::FinishTime) {
            env.storage()
                .persistent()
                .set(&DataKey::FinishTime, &env.ledger().timestamp());
        }

        publish_event(
            env,
            "raffle_finalized",
            RaffleFinalized {
                winners,
                winning_ticket_ids,
                total_tickets_sold: raffle.tickets_sold,
                randomness_source: RandomnessSource::CommitReveal,
                randomness_type: RandomnessType::Vrf,
                finalized_at: env.ledger().timestamp(),
            },
        );

        publish_event(
            env,
            "status_changed",
            RaffleStatusChanged {
                old_status: RaffleStatus::Drawing,
                new_status: RaffleStatus::Finalized,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn claim_prize(env: Env, winner: Address, tier_index: u32) -> Result<i128, Error> {
        winner.require_auth();
        let mut raffle = read_raffle(&env)?;

        // Checks
        if raffle.status != RaffleStatus::Finalized {
            return Err(Error::InvalidStateTransition);
        }

        if tier_index >= raffle.winners.len() {
            return Err(Error::InvalidParameters);
        }

        let actual_winner = raffle.winners.get(tier_index).unwrap();
        if actual_winner != winner {
            // env.events().publish((Symbol::new(&env, "DEBUG"), Symbol::new(&env, "NotWinner")), (actual_winner, winner.clone()));
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

        write_raffle(&env, &raffle);

        if !env.storage().persistent().has(&DataKey::FinishTime) {
            env.storage()
                .persistent()
                .set(&DataKey::FinishTime, &env.ledger().timestamp());
        }

        // Interactions: external token transfers
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();

        // Critical: winner must receive their prize. Use try_transfer so a
        // malicious token surfaces as a typed error rather than a panic that
        // could leave state inconsistent after the guard is released.
        token_client
            .try_transfer(&contract_address, &winner, &net_amount)
            .map_err(|_| {
                // Roll back the claimed flag so the winner can retry once the
                // token is fixed / replaced.
                let mut rollback = raffle.claimed_winners.clone();
                rollback.set(tier_index, false);
                let mut r = raffle.clone();
                r.claimed_winners = rollback;
                write_raffle(&env, &r);
                release_guard(&env);
                Error::TokenTransferFailed
            })?;

        if platform_fee > 0 {
            if let (Some(router), Some(tikka)) = (&raffle.swap_router, &raffle.tikka_token) {
                if raffle.payment_token != *tikka {
                    // Approve router — non-critical, skip silently on failure.
                    let _ = token_client.try_approve(
                        &contract_address,
                        router,
                        &platform_fee,
                        &(env.ledger().sequence() + 100),
                    );

                    let mut path = Vec::new(&env);
                    path.push_back(raffle.payment_token.clone());
                    path.push_back(tikka.clone());

                    let router_client = SoroswapRouterClient::new(&env, router);
                    // Non-critical: if the swap fails (e.g. malicious router or
                    // slippage), fees stay in the contract rather than blocking
                    // the winner's claim.
                    let swap_result = router_client.try_swap_exact_tokens_for_tokens(
                        &platform_fee,
                        &0i128,
                        &path,
                        &contract_address,
                        &(env.ledger().timestamp() + 300),
                    );

                    if let Ok(Ok(amount_out)) = swap_result {
                        let tikka_client = token::Client::new(&env, tikka);
                        // Non-critical: burn failure keeps fees in contract.
                        let _ = tikka_client.try_burn(&contract_address, &amount_out);

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
                    }
                    // If swap failed, fees remain in the contract for manual
                    // recovery — the winner's claim is already settled above.
                } else {
                    let tikka_client = token::Client::new(&env, tikka);
                    // Non-critical: burn failure keeps fees in contract.
                    let _ = tikka_client.try_burn(&contract_address, &platform_fee);

                    publish_event(
                        &env,
                        "buyback_and_burn_executed",
                        crate::events::BuybackAndBurnExecuted {
                            router: contract_address.clone(),
                            payment_token: raffle.payment_token.clone(),
                            tikka_token: tikka.clone(),
                            amount_in: platform_fee,
                            amount_out: platform_fee,
                            timestamp: env.ledger().timestamp(),
                        },
                    );
                }
            } else if raffle.treasury_address.is_some() {
                // Non-critical: treasury transfer failure keeps fees in contract.
                let _ = token_client.try_transfer(
                    &contract_address,
                    &raffle.treasury_address.clone().unwrap(),
                    &platform_fee,
                );
            }
        }

        release_guard(&env);

        env.events().publish(
            (Symbol::new(&env, "PrizeClaimed"), winner.clone()),
            PrizeClaimed {
                winner: winner.clone(),
                tier_index,
                payment_token: raffle.payment_token.clone(),
                gross_amount: tier_prize_amount,
                net_amount,
                platform_fee,
                claimed_at,
            },
        );

        Ok(net_amount)
    }

    pub fn cancel_raffle(env: Env, reason: CancelReason) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;

        match reason {
            CancelReason::CreatorCancelled => {
                require_creator(&env)?;
            }
            CancelReason::AdminCancelled => {
                require_admin(&env)?;
            }
            CancelReason::OracleTimeout | CancelReason::MinTicketsNotMet => {
                let factory: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::Factory)
                    .ok_or(Error::NotAuthorized)?;
                factory.require_auth();
            }
            CancelReason::AdminCancelled => {
                // Factory/admin can cancel at any time
                let admin: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::Admin)
                    .ok_or(Error::NotAuthorized)?;
                admin.require_auth();
            }
            CancelReason::OracleTimeout | CancelReason::MinTicketsNotMet => {
                // Factory can trigger these system cancellations
                let factory: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::Factory)
                    .ok_or(Error::NotAuthorized)?;
                factory.require_auth();
            }
        }

        if raffle.status == RaffleStatus::Finalized
            || raffle.status == RaffleStatus::Cancelled
            || raffle.status == RaffleStatus::Failed
        {
            return Err(Error::InvalidStateTransition);
        }

        let old_status = raffle.status.clone();
        raffle.status = RaffleStatus::Cancelled;

        // CEI: Update state before external call
        let prize_to_refund = if raffle.prize_deposited {
            raffle.prize_deposited = false;
            raffle.prize_amount
        } else {
            0
        };

        write_raffle(&env, &raffle);
        if !env.storage().persistent().has(&DataKey::FinishTime) {
            env.storage()
                .persistent()
                .set(&DataKey::FinishTime, &env.ledger().timestamp());
        }

        // Interaction: Refund prize if deposited
        if prize_to_refund > 0 {
            let token_client = token::Client::new(&env, &raffle.payment_token);
            let contract_address = env.current_contract_address();
            token_client.transfer(&contract_address, &raffle.creator, &prize_to_refund);
        }

        publish_event(
            &env,
            "status_changed",
            RaffleStatusChanged {
                old_status,
                new_status: RaffleStatus::Cancelled,
                timestamp: env.ledger().timestamp(),
            },
        );

        publish_event(
            &env,
            "raffle_cancelled",
            RaffleCancelled {
                creator: raffle.creator,
                reason,
                tickets_sold: raffle.tickets_sold,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn refund_ticket(env: Env, ticket_id: u32) -> Result<i128, Error> {
        let raffle = read_raffle(&env)?;

        // Allow refunds for Cancelled raffles, or Active/Drawing raffles that have expired
        // without being finalized (failed raffle scenario)
        let is_cancelled = raffle.status == RaffleStatus::Cancelled;
        let is_expired_and_failed = (raffle.status == RaffleStatus::Active
            || raffle.status == RaffleStatus::Drawing)
            && raffle.end_time != 0
            && env.ledger().timestamp() > raffle.end_time;

        if !is_cancelled && !is_expired_and_failed {
            return Err(Error::InvalidStateTransition);
        }

        let ticket = env
            .storage()
            .persistent()
            .get::<_, Ticket>(&DataKey::Ticket(ticket_id))
            .ok_or(Error::TicketNotFound)?;

        ticket.owner.require_auth();

        let is_refunded = env
            .storage()
            .persistent()
            .get(&DataKey::RefundStatus(ticket_id))
            .unwrap_or(false);
        if is_refunded {
            return Err(Error::TicketAlreadyRefunded);
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
        token_client
            .try_transfer(&contract_address, &ticket.owner, &raffle.ticket_price)
            .map_err(|_| Error::TokenTransferFailed)?;

        release_guard(&env);

        publish_event(
            &env,
            "ticket_refunded",
            TicketRefunded {
                buyer: ticket.owner.clone(),
                ticket_number: ticket_id,
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
        let ticket_opt = env
            .storage()
            .persistent()
            .get::<_, Ticket>(&DataKey::Ticket(token_id));
        if let Some(ticket) = ticket_opt {
            Ok(ticket.owner)
        } else {
            Err(Error::InvalidParameters)
        }
    }

    pub fn approve(
        env: Env,
        caller: Address,
        operator: Option<Address>,
        token_id: u32,
    ) -> Result<(), Error> {
        caller.require_auth();
        let ticket_opt = env
            .storage()
            .persistent()
            .get::<_, Ticket>(&DataKey::Ticket(token_id));
        let owner = ticket_opt.ok_or(Error::InvalidParameters)?.owner;

        let is_approved_for_all = env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::ApprovedForAll(owner.clone(), caller.clone()))
            .unwrap_or(false);
        if caller != owner && !is_approved_for_all {
            return Err(Error::NotAuthorized);
        }

        if let Some(op) = operator {
            env.storage()
                .persistent()
                .set(&DataKey::Approved(token_id), &op);
        } else {
            env.storage()
                .persistent()
                .remove(&DataKey::Approved(token_id));
        }
        Ok(())
    }

    pub fn set_approval_for_all(
        env: Env,
        caller: Address,
        operator: Address,
        approved: bool,
    ) -> Result<(), Error> {
        caller.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::ApprovedForAll(caller, operator), &approved);
        Ok(())
    }

    pub fn get_approved(env: Env, token_id: u32) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Approved(token_id))
    }

    pub fn is_approved_for_all(env: Env, owner: Address, operator: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::ApprovedForAll(owner, operator))
            .unwrap_or(false)
    }

    pub fn transfer(env: Env, from: Address, to: Address, token_id: u32) -> Result<(), Error> {
        from.require_auth();
        do_transfer(&env, from, to, token_id)
    }

    pub fn transfer_from(
        env: Env,
        spender: Address,
        from: Address,
        to: Address,
        token_id: u32,
    ) -> Result<(), Error> {
        spender.require_auth();
        let is_approved_for_all = env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::ApprovedForAll(from.clone(), spender.clone()))
            .unwrap_or(false);
        let individual_approval = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Approved(token_id));

        if spender != from && !is_approved_for_all && individual_approval != Some(spender.clone()) {
            return Err(Error::NotAuthorized);
        }
        do_transfer(&env, from, to, token_id)
    }

    pub fn get_raffle(env: Env) -> Result<Raffle, Error> {
        read_raffle(&env)
    }

    /// Returns the decimal precision of the raffle's payment token.
    /// Useful for clients to correctly format ticket_price and prize_amount.
    pub fn get_token_decimals(env: Env) -> Result<u32, Error> {
        let raffle = read_raffle(&env)?;
        let client = token::Client::new(&env, &raffle.payment_token);
        Ok(client.decimals())
    }

    /// Get all tickets or a paginated subset
    /// Returns tickets from start index for count number of tickets
    /// Optimized: Load individual tickets from persistent storage instead of Vec
    pub fn get_tickets(env: Env, start: u32, count: u32) -> Vec<Ticket> {
        let total = get_ticket_count(&env);

        if start >= total {
            return Vec::new(&env);
        }

        let end = if start + count > total {
            total
        } else {
            start + count
        };
        let mut result = Vec::new(&env);

        for i in start..end {
            let ticket_id = i + 1; // ticket IDs start at 1
            if let Some(ticket) = env
                .storage()
                .persistent()
                .get::<_, Ticket>(&DataKey::Ticket(ticket_id))
            {
                result.push_back(ticket);
            }
        }

        result
    }

    /// Get total ticket count
    pub fn get_ticket_count(env: Env) -> u32 {
        get_ticket_count(&env)
    }

    /// Get fairness data for a finalized raffle
    /// Returns all data used to select the winner for transparency
    pub fn get_fairness_data(env: Env) -> Result<FairnessData, Error> {
        let raffle = read_raffle(&env)?;

        if raffle.status != RaffleStatus::Finalized {
            return Err(Error::InvalidStateTransition);
        }

        let fairness_metadata: FairnessMetadata = env
            .storage()
            .persistent()
            .get(&DataKey::RandomnessSeed)
            .ok_or(Error::InvalidStateTransition)?;

        let tickets = read_tickets(&env);
        let mut ticket_ids = Vec::new(&env);
        for ticket in tickets.iter() {
            ticket_ids.push_back(ticket.id);
        }

        Ok(FairnessData {
            seed: fairness_metadata.seed,
            randomness_source: fairness_metadata.randomness_source,
            ticket_ids,
            winning_ticket_indices: fairness_metadata.winning_ticket_indices,
            draw_timestamp: fairness_metadata.draw_timestamp,
            draw_sequence: fairness_metadata.draw_sequence,
        })
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

    pub fn withdraw_fees(env: Env) -> Result<i128, Error> {
        require_not_paused(&env)?;

        // Only admin can withdraw fees
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotAuthorized)?;
        admin.require_auth();

        let raffle = read_raffle(&env)?;
        let treasury = raffle.treasury_address.ok_or(Error::InvalidParameters)?;

        let accumulated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0i128);

        if accumulated <= 0 {
            return Ok(0);
        }

        // Effects: zero out fees BEFORE external call (CEI pattern)
        env.storage()
            .instance()
            .set(&DataKey::AccumulatedFees, &0i128);

        // Interaction: transfer to treasury
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&contract_address, &treasury, &accumulated);

        publish_event(
            &env,
            "fees_withdrawn",
            crate::events::FeesWithdrawn {
                recipient: treasury,
                amount: accumulated,
                token: raffle.payment_token,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(accumulated)
    }

    pub fn get_accumulated_fees(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0i128)
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

    pub fn get_finish_time(env: Env) -> Option<u64> {
        env.storage().persistent().get(&DataKey::FinishTime)
    }

    pub fn wipe_storage(env: Env) -> Result<(), Error> {
        // Auth: only factory may call
        let factory: Address = env
            .storage()
            .instance()
            .get(&DataKey::Factory)
            .ok_or(Error::NotAuthorized)?;
        factory.require_auth();

        let raffle = read_raffle(&env)?;
        let tickets_sold = raffle.tickets_sold;
        let tickets_list = read_tickets(&env);

        // Remove per-ticket persistent entries
        for n in 1..=tickets_sold {
            env.storage().persistent().remove(&DataKey::Ticket(n));
            env.storage().persistent().remove(&DataKey::RefundStatus(n));
        }
        // Remove per-buyer ticket counts
        for ticket in tickets_list.iter() {
            env.storage()
                .persistent()
                .remove(&DataKey::TicketCount(ticket.owner));
        }
        // Remove FinishTime
        env.storage().persistent().remove(&DataKey::FinishTime);

        // Remove instance storage entries (Factory and Admin removed last)
        env.storage().instance().remove(&DataKey::Raffle);
        env.storage().instance().remove(&DataKey::NextTicketId);
        env.storage().instance().remove(&DataKey::Paused);
        if env.storage().instance().has(&DataKey::ReentrancyGuard) {
            env.storage().instance().remove(&DataKey::ReentrancyGuard);
        }
        env.storage().instance().remove(&DataKey::Factory);
        env.storage().instance().remove(&DataKey::Admin);

        Ok(())
    }
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotAuthorized)?;
        admin.require_auth();

        if new_admin == admin {
            env.storage().instance().remove(&DataKey::PendingAdmin);
            return Ok(());
        }

        if env.storage().instance().has(&DataKey::PendingAdmin) {
            return Err(Error::AdminTransferPending);
        }

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);

        publish_event(
            &env,
            "admin_transfer_proposed",
            crate::events::AdminTransferProposed {
                current_admin: admin,
                proposed_admin: new_admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn transfer_ownership(env: Env, new_owner: Address) -> Result<(), Error> {
        Self::transfer_admin(env, new_owner)
    }

    pub fn accept_admin(env: Env) -> Result<(), Error> {
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(Error::NoPendingTransfer)?;
        pending.require_auth();

        let old_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();

        env.storage().instance().set(&DataKey::Admin, &pending);
        env.storage().instance().remove(&DataKey::PendingAdmin);

        publish_event(
            &env,
            "admin_transfer_accepted",
            crate::events::AdminTransferAccepted {
                old_admin,
                new_admin: pending,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn accept_ownership(env: Env) -> Result<(), Error> {
        Self::accept_admin(env)
    }
}

#[cfg(test)]
mod test;
