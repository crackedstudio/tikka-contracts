#![no_std]

use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contracterror, contractimpl, contracttype, token,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};

mod events;
mod randomness;

use raffle_shared::{
    CancelReason, FailureReason, FairnessData, RaffleConfig, RaffleStatus, RandomnessSource, RandomnessType,
    Ticket,
};

use self::randomness::{OracleSeedWinnerSelection, WinnerSelectionStrategy};

use crate::events::{
    ContractPaused, ContractUnpaused, DrawTriggered, EmergencyWithdrawn, FeesWithdrawn,
    OracleAddressUpdated, PrizeClaimed, PrizeDeposited, PrizeRefunded, ProtocolFeeUpdated,
    RaffleCancelled, RaffleCreated, RaffleFinalized, RaffleFailed, RaffleStatusChanged,
    RandomnessFallbackTriggered, RandomnessReceived, RandomnessRequested, SwapDeadlineUpdated,
    TicketPurchased, TicketRefunded, TicketSalesPaused, TicketSalesResumed, TokensRescued,
    WinnerDrawn,
};

const ORACLE_TIMEOUT_LEDGERS: u32 = 200;
pub const MAX_DESCRIPTION_LENGTH: u32 = 1000;
pub const MAX_TICKETS_LIMIT: u32 = 100_000;
pub const MAX_PRIZES: u32 = 100;
pub const MIN_TICKET_PRICE: i128 = 10_000;
pub const MAX_PRIZE_AMOUNT: i128 = 1_000_000_000_000_000_000_000; // 1e21
/// Default and bounds for the claim lockup delay (#259).
pub const DEFAULT_CLAIM_LOCKUP_SECONDS: u64 = 3_600;
pub const MAX_CLAIM_LOCKUP_SECONDS: u64 = 604_800; // 7 days
/// Default and bounds for swap deadline (network congestion tolerance).
pub const DEFAULT_SWAP_DEADLINE_SECONDS: u64 = 300; // 5 minutes
pub const MAX_SWAP_DEADLINE_SECONDS: u64 = 3_600; // 1 hour
/// Emergency withdraw delay (seconds). Set to 90 days.
pub const EMERGENCY_WITHDRAW_DELAY_SECONDS: u64 = 90 * 24 * 3600; // 7776000

#[contract]
pub struct Contract;
#[contracttype]
#[derive(Clone)]
pub struct Raffle {
    pub creator: Address,
    pub description: String,
    pub end_time: u64,
    pub no_deadline: bool,
    pub max_tickets: u32,
    pub max_tickets_per_tx: u32,
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
    /// Seconds after finalization before winners may claim (#259).
    pub claim_lockup_seconds: u64,
    /// Swap deadline window in seconds (added to current timestamp for token swaps).
    /// Defaults to 300 (5 minutes) if zero.
    pub swap_deadline_seconds: u64,
    /// When true, ticket purchases are blocked while the raffle remains Active.
    pub ticket_sales_paused: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct FairnessMetadata {
    pub seed: u64,
    pub randomness_source: RandomnessSource,
    pub winning_ticket_indices: Vec<u32>,
    pub draw_timestamp: u64,
    pub draw_sequence: u32,
}

#[soroban_sdk::contracttype]
#[derive(Clone)]
pub enum DataKey {
    Raffle,
    TicketCount(Address),
    Ticket(u32),
    TicketRefunded(u32),
    Factory,
    ReentrancyGuard,
    Paused,
    Admin,
    RandomnessSeed,
    RandomnessRequested,
    RandomnessRequestLedger,
    RandomnessRequestId,
    FinishTime,
    AccumulatedFees,
    CommitEntry(u32),
    DrawingLock,
    TicketBuyers,
}

/// A single participant commit recorded during the commit phase of a
/// commit-reveal draw.  Keyed by ticket ID so the record is not lost when
/// the ticket changes hands after the commit was submitted.
#[contracttype]
#[derive(Clone)]
pub struct CommitRevealEntry {
    /// The address that submitted the commit (the ticket owner *at commit
    /// time*).  Preserved for audit/fairness purposes.
    pub committer: Address,
    /// The hash the participant committed to (SHA-256 of their secret).
    pub hash: BytesN<32>,
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
    InvalidQuantity = 22,
    InvalidStatus = 23,
    ContractPaused = 24,
    InvalidStateTransition = 25,
    RaffleExpired = 26,
    InsufficientTickets = 31,
    MultipleTicketsNotAllowed = 32,
    NoTicketsSold = 33,
    NoActiveTickets = 46,
    TicketNotFound = 34,
    RaffleEnded = 35,
    ArithmeticOverflow = 41,
    AlreadyInitialized = 42,
    NotInitialized = 43,
    Reentrancy = 44,
    TokenTransferFailed = 45,
    DeadlinePassed = 47,
    SlippageExceeded = 48,
    InvalidIndex = 49,
    MorePrizesThanTickets = 50,
    ZeroPrize = 51,
    InvalidTokenAddress = 52,
    TooManyPrizes = 53,
    EmergencyTooEarly = 54,
    InvalidTicketRange = 55,
    InsufficientAccumulatedFees = 56,
    PrizeConfigurationLocked = 57,
    ExceedsMaxTicketsPerTx = 58,
    DrawingAlreadyInProgress = 59,
    InvalidStatusForDrawingTransition = 60,
    DrawingAlreadyComplete = 61,
    InvalidEndTime = 62,
    InvalidAdminAddress = 63,
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

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .ok_or(Error::NotAuthorized)?;
    admin.require_auth();
    Ok(admin)
}

/// Maximum protocol fee in basis points (20%) for per-raffle admin updates.
pub const MAX_PROTOCOL_FEE_BP: u32 = 2_000;

fn get_ticket_owner(env: &Env, ticket_id: u32) -> Option<Address> {
    env.storage()
        .persistent()
        .get::<_, Ticket>(&DataKey::Ticket(ticket_id))
        .map(|t| t.owner)
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

// Helper to enforce slippage and deadline guards for token swaps
// Uses the raffle's configurable swap_deadline_seconds to calculate the deadline
fn enforce_swap_guard(
    env: &Env,
    raffle: &Raffle,
    amount_out: i128,
    min_amount_out: i128,
) -> Result<(), Error> {
    // Calculate deadline based on current timestamp and raffle's configured deadline window
    let deadline = env.ledger().timestamp() + raffle.swap_deadline_seconds;
    
    // Check deadline
    if env.ledger().timestamp() > deadline {
        return Err(Error::DeadlinePassed);
    }
    // Check slippage (amount_out must be >= min_amount_out)
    if amount_out < min_amount_out {
        return Err(Error::SlippageExceeded);
    }
    Ok(())
}

fn release_guard(env: &Env) {
    env.storage().instance().remove(&DataKey::ReentrancyGuard);
}

struct Guard<'a> {
    env: &'a Env,
}

impl<'a> Guard<'a> {
    fn new(env: &'a Env) -> Result<Self, Error> {
        acquire_guard(env)?;
        Ok(Guard { env })
    }
}

impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        release_guard(self.env);
    }
}

// Helper function to request randomness (used in both buy_tickets and finalize_raffle)
fn request_randomness(env: &Env) -> Result<u64, Error> {
    let already: bool = env
        .storage()
        .instance()
        .get(&DataKey::RandomnessRequested)
        .unwrap_or(false);
    if already {
        return Err(Error::RandomnessAlreadyRequested);
    }

    // Generate unique request ID
    let request_id_xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.current_contract_address().to_xdr(env),
    )
        .to_xdr(env);
    let request_id_hash: BytesN<32> = env.crypto().sha256(&request_id_xdr).into();
    let arr = request_id_hash.to_array();
    let mut id_bytes = [0u8; 8];
    id_bytes.copy_from_slice(&arr[..8]);
    let request_id = u64::from_be_bytes(id_bytes);

    env.storage()
        .instance()
        .set(&DataKey::RandomnessRequested, &true);
    env.storage()
        .instance()
        .set(&DataKey::RandomnessRequestLedger, &env.ledger().sequence());
    env.storage()
        .instance()
        .set(&DataKey::RandomnessRequestId, &request_id);

    Ok(request_id)
}

/// State machine for drawing entry:
/// - PendingPrize -> Active is the initial funded state.
/// - Active -> Drawing is the only valid transition that begins winner selection.
/// - Active -> Drawing is also used when buy_tickets fills the last ticket and the raffle
///   should enter the draw window.
/// - Drawing -> Finalized is the normal completion path after the oracle or fallback seed
///   produces winners.
/// - Drawing -> Cancelled/Failed is the error or refund path when the drawing flow is aborted.
///
/// Soroban contract calls are atomic per call frame, but the same ledger can still observe
/// overlapping state transitions via re-entrant or concurrent calls into the contract. The
/// DrawingLock is therefore the exclusive guard that makes the transition single-owner even
/// when two entry points race in the same ledger or during re-entry.
///
/// This helper is the single source of truth for entering Drawing and for setting the
/// DrawingLock. The lock prevents any second caller from entering Drawing while the first
/// draw flow is in progress, and it is cleared only after the callback or rollback path
/// finishes so the contract never stays permanently pinned in a half-drawn state.
fn transition_to_drawing(env: &Env, raffle: &mut Raffle, timestamp: u64) -> Result<(), Error> {
    // SECURITY: fast-path guard — if DrawingLock is true, another Drawing transition is
    // already in progress; reject without reading further state
    let drawing_lock: bool = env
        .storage()
        .instance()
        .get(&DataKey::DrawingLock)
        .unwrap_or(false);
    if drawing_lock {
        return Err(Error::DrawingAlreadyInProgress);
    }

    if raffle.status != RaffleStatus::Active {
        if raffle.status == RaffleStatus::Drawing {
            return Err(Error::DrawingAlreadyInProgress);
        }
        return Err(Error::InvalidStatusForDrawingTransition);
    }

    let old_status = raffle.status.clone();
    raffle.status = RaffleStatus::Drawing;
    write_raffle(env, raffle);
    RaffleStatusChanged {
        old_status,
        new_status: RaffleStatus::Drawing,
        timestamp,
    }
    .publish(env);

    // SECURITY: set the DrawingLock in the same contract call as the status transition
    env.storage().instance().set(&DataKey::DrawingLock, &true);
    Ok(())
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

fn validate_token_address(env: &Env, token_address: &Address) -> Result<(), Error> {
    let token_client = token::Client::new(env, token_address);
    let _ = token_client
        .try_decimals()
        .map_err(|_| Error::InvalidTokenAddress)?;
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
    let arr = hash.to_array();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&arr[..8]);
    u64::from_be_bytes(bytes)
}

fn calculate_tier_prize(raffle: &Raffle, tier_index: u32) -> Result<i128, Error> {
    let last_tier_index = raffle.prizes.len() - 1;

    if tier_index == last_tier_index {
        let mut allocated_before_last = 0i128;
        for i in 0..last_tier_index {
            let prize_bp = raffle.prizes.get(i).unwrap();
            let amount = raffle
                .prize_amount
                .checked_mul(prize_bp as i128)
                .ok_or(Error::ArithmeticOverflow)?
                / 10000;
            allocated_before_last = allocated_before_last
                .checked_add(amount)
                .ok_or(Error::ArithmeticOverflow)?;
        }

        return raffle
            .prize_amount
            .checked_sub(allocated_before_last)
            .ok_or(Error::ArithmeticOverflow);
    }

    let prize_bp = raffle.prizes.get(tier_index).unwrap();
    raffle
        .prize_amount
        .checked_mul(prize_bp as i128)
        .ok_or(Error::ArithmeticOverflow)
        .map(|amount| amount / 10000)
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
        if config.no_deadline && config.end_time != 0 {
            return Err(Error::InvalidParameters);
        }
        if !config.no_deadline && config.end_time <= now {
            return Err(Error::InvalidParameters);
        }
        // Explicit check: end_time must be either 0 (no deadline) or in the future
        if config.end_time != 0 && config.end_time <= now {
            return Err(Error::InvalidEndTime);
        }
        if config.max_tickets == 0 || config.max_tickets > MAX_TICKETS_LIMIT {
            return Err(Error::InvalidParameters);
        }
        if config.max_tickets < config.min_tickets {
            return Err(Error::InvalidTicketRange);
        }
        if config.max_tickets_per_tx == 0 || config.max_tickets_per_tx > config.max_tickets {
            return Err(Error::InvalidParameters);
        }

        if config.ticket_price < MIN_TICKET_PRICE {
            return Err(Error::InvalidParameters);
        }
        if config.prize_amount < config.ticket_price {
            return Err(Error::InvalidParameters);
        }
        if config.prize_amount > MAX_PRIZE_AMOUNT {
            return Err(Error::InvalidParameters);
        }
        if config.prizes.is_empty() {
            return Err(Error::InvalidParameters);
        }
        if config.prizes.len() > MAX_PRIZES {
            return Err(Error::TooManyPrizes);
        }
        let mut total_prizes_bp = 0u32;
        for prize_bp in config.prizes.iter() {
            total_prizes_bp += prize_bp;
        }
        if total_prizes_bp != 10000 {
            return Err(Error::InvalidParameters);
        }

        if config.protocol_fee_bp > 10000 {
            return Err(Error::InvalidParameters);
        }

        if config.randomness_source == RandomnessSource::External {
            match config.oracle_address {
                None => return Err(Error::InvalidParameters),
                Some(ref addr) if *addr == env.current_contract_address() => {
                    return Err(Error::InvalidParameters);
                }
                Some(_) => {}
            }
        }

        if config.randomness_source != RandomnessSource::External && config.oracle_address.is_some()
        {
            return Err(Error::InvalidParameters);
        }

        if config.metadata_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(Error::InvalidParameters);
        }

        // Validate that the payment_token is a valid token contract
        validate_token_address(&env, &config.payment_token)?;

        // #259: claim_lockup_seconds must be within [0, MAX_CLAIM_LOCKUP_SECONDS].
        // Zero is interpreted as "use the default".
        let claim_lockup_seconds = if config.claim_lockup_seconds == 0 {
            DEFAULT_CLAIM_LOCKUP_SECONDS
        } else {
            config.claim_lockup_seconds
        };
        if claim_lockup_seconds > MAX_CLAIM_LOCKUP_SECONDS {
            return Err(Error::InvalidParameters);
        }

        // Swap deadline must be within [0, MAX_SWAP_DEADLINE_SECONDS].
        // Zero is interpreted as "use the default".
        let swap_deadline_seconds = if config.swap_deadline_seconds == 0 {
            DEFAULT_SWAP_DEADLINE_SECONDS
        } else {
            config.swap_deadline_seconds
        };
        if swap_deadline_seconds > MAX_SWAP_DEADLINE_SECONDS {
            return Err(Error::InvalidParameters);
        }

        let raffle = Raffle {
            creator: creator.clone(),
            description: config.description.clone(),
            end_time: config.end_time,
            no_deadline: config.no_deadline,
            max_tickets: config.max_tickets,
            max_tickets_per_tx: config.max_tickets_per_tx,
            min_tickets: config.min_tickets,
            allow_multiple: config.allow_multiple,
            ticket_price: config.ticket_price,
            payment_token: config.payment_token.clone(),
            prize_amount: config.prize_amount,
            prizes: config.prizes.clone(),
            tickets_sold: 0,
            status: RaffleStatus::PendingPrize,
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
            claim_lockup_seconds,
            swap_deadline_seconds,
            ticket_sales_paused: false,
        };
        write_raffle(&env, &raffle);
        env.storage().instance().set(&DataKey::Factory, &factory);
        env.storage().instance().set(&DataKey::Admin, &admin);

        RaffleCreated {
            raffle_id: env.current_contract_address(),
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

        // Move tokens first. If the transfer fails we want the contract state
        // (prize_deposited flag, raffle.status) to remain untouched.
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        let _ = token_client
            .try_transfer(&raffle.creator, &contract_address, &raffle.prize_amount)
            .map_err(|_| Error::TokenTransferFailed)?;

        // Transfer succeeded — flip the prize_deposited flag and transition the
        // raffle into Active so ticket sales can begin. This is the explicit
        // status transition #225 asks for: previously the raffle was created
        // directly in Active and `deposit_prize` only flipped a boolean, which
        // left off-chain indexers without a clear signal that the raffle had
        // become buyable.
        raffle.prize_deposited = true;
        raffle.status = RaffleStatus::Active;
        write_raffle(&env, &raffle);

        let timestamp = env.ledger().timestamp();

        PrizeDeposited {
            creator: raffle.creator.clone(),
            amount: raffle.prize_amount,
            token: raffle.payment_token.clone(),
            timestamp,
        }
        .publish(&env);

        RaffleStatusChanged {
            old_status,
            new_status: RaffleStatus::Active,
            timestamp,
        }
        .publish(&env);

        Ok(())
    }

    pub fn buy_tickets(env: Env, buyer: Address, quantity: u32) -> Result<u32, Error> {
        // SECURITY: Fast path guard for DrawingLock!
        let drawing_lock: bool = env
            .storage()
            .instance()
            .get(&DataKey::DrawingLock)
            .unwrap_or(false);
        if drawing_lock {
            return Err(Error::DrawingAlreadyInProgress);
        }
        if quantity == 0 {
            return Err(Error::InvalidQuantity);
        }
        let mut raffle = read_raffle(&env)?;
        if quantity > raffle.max_tickets_per_tx {
            return Err(Error::ExceedsMaxTicketsPerTx);
        }
        buyer.require_auth();
        require_not_paused(&env)?;

        if raffle.status != RaffleStatus::Active {
            return Err(Error::RaffleInactive);
        }
        if raffle.ticket_sales_paused {
            return Err(Error::ContractPaused);
        }
        if !raffle.prize_deposited {
            return Err(Error::InvalidStateTransition);
        }
        if !raffle.no_deadline && env.ledger().timestamp() > raffle.end_time {
            return Err(Error::RaffleExpired);
        }

        // SECURITY: Snapshot initial state for optimistic concurrency control
        let snapshot_sold = raffle.tickets_sold;
        let current_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TicketCount(buyer.clone()))
            .unwrap_or(0);

        if snapshot_sold + quantity > raffle.max_tickets {
            return Err(Error::TicketsSoldOut);
        }
        if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
            return Err(Error::MultipleTicketsNotAllowed);
        }

        let timestamp = env.ledger().timestamp();
        let total_price = raffle
            .ticket_price
            .checked_mul(quantity as i128)
            .ok_or(Error::InvalidParameters)?;

        let protocol_fee = total_price
            .checked_mul(raffle.protocol_fee_bp as i128)
            .ok_or(Error::ArithmeticOverflow)?
            / 10000;
        let _net_amount = total_price - protocol_fee;

        // SECURITY: Re-read persisted state and verify no concurrent changes
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

        // Track unique buyer addresses for later storage cleanup
        if current_count == 0 {
            let mut buyers: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::TicketBuyers)
                .unwrap_or_else(|| Vec::new(&env));
            buyers.push_back(buyer.clone());
            env.storage()
                .persistent()
                .set(&DataKey::TicketBuyers, &buyers);
        }

        // Now commit all changes atomically
        let mut ticket_ids = Vec::new(&env);
        for i in 0..quantity {
            let ticket_id = snapshot_sold + i + 1;
            let ticket = Ticket {
                id: ticket_id,
                owner: buyer.clone(),
                purchase_time: timestamp,
                ticket_number: ticket_id,
            };
            env.storage()
                .persistent()
                .set(&DataKey::Ticket(ticket_id), &ticket);
            ticket_ids.push_back(ticket_id);
        }

        // Update ticket count and raffle sold
        env.storage().persistent().set(
            &DataKey::TicketCount(buyer.clone()),
            &(current_count + quantity),
        );
        raffle.tickets_sold = snapshot_sold + quantity;

        if raffle.tickets_sold >= raffle.max_tickets {
            transition_to_drawing(&env, &mut raffle, timestamp)?;
            // SECURITY: Atomically request randomness after transitioning to Drawing
            if raffle.randomness_source == RandomnessSource::External {
                let request_id = request_randomness(&env)?;
                DrawTriggered {
                    caller: buyer.clone(),
                    total_tickets_sold: raffle.tickets_sold,
                    timestamp,
                }
                .publish(&env);

                RandomnessRequested {
                    oracle: raffle
                        .oracle_address
                        .clone()
                        .unwrap_or(env.current_contract_address()),
                    request_id,
                    timestamp,
                }
                .publish(&env);
            }
        }

        write_raffle(&env, &raffle);

        if let Some(factory_address) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Factory)
        {
            let record_volume_args: Vec<Val> =
                (raffle.payment_token.clone(), total_price).into_val(&env);

            env.authorize_as_current_contract(Vec::from_array(
                &env,
                [InvokerContractAuthEntry::Contract(SubContractInvocation {
                    context: ContractContext {
                        contract: factory_address.clone(),
                        fn_name: Symbol::new(&env, "record_volume"),
                        args: record_volume_args.clone(),
                    },
                    sub_invocations: Vec::new(&env),
                })],
            ));
            env.invoke_contract::<()>(
                &factory_address,
                &Symbol::new(&env, "record_volume"),
                record_volume_args,
            );
            env.invoke_contract::<()>(
                &factory_address,
                &Symbol::new(&env, "track_participant"),
                (buyer.clone(),).into_val(&env),
            );
        }

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let _ = token_client
            .try_transfer(&buyer, env.current_contract_address(), &total_price)
            .map_err(|_| Error::TokenTransferFailed)?;

        if protocol_fee > 0 {
            if let Some(treasury) = &raffle.treasury_address {
                token_client.transfer(&env.current_contract_address(), treasury, &protocol_fee);
            }
            let prev_fees: i128 = env
                .storage()
                .instance()
                .get(&DataKey::AccumulatedFees)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::AccumulatedFees, &(prev_fees + protocol_fee));
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

    /// Submit a commit during the commit phase of a commit-reveal draw.
    ///
    /// The entry is stored under `CommitEntry(ticket_id)` — keyed by the
    /// ticket ID rather than the caller's address — so the entropy
    /// contribution is preserved even if the ticket is subsequently
    /// transferred to a different address before finalization.  This fixes
    /// the silent entropy-drop identified in issue #311.
    pub fn submit_commit(env: Env, ticket_id: u32, hash: BytesN<32>) -> Result<(), Error> {
        let raffle = read_raffle(&env)?;

        if raffle.randomness_source != RandomnessSource::CommitReveal {
            return Err(Error::InvalidParameters);
        }

        // Commits may only be submitted while the raffle is still active.
        if raffle.status != RaffleStatus::Active && raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStatus);
        }

        // The ticket must exist.
        let ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .ok_or(Error::TicketNotFound)?;

        // Only the current ticket owner may submit (or update) a commit.
        ticket.owner.require_auth();

        // Store keyed by ticket ID so a later transfer does not orphan the
        // entropy contribution.
        let entry = CommitRevealEntry {
            committer: ticket.owner,
            hash,
        };
        env.storage()
            .persistent()
            .set(&DataKey::CommitEntry(ticket_id), &entry);

        Ok(())
    }

    pub fn finalize_raffle(env: Env) -> Result<(), Error> {
        // SECURITY: fast-path guard — if DrawingLock is true, another Drawing transition is
        // already in progress; reject without reading further state
        let drawing_lock: bool = env
            .storage()
            .instance()
            .get(&DataKey::DrawingLock)
            .unwrap_or(false);
        if drawing_lock {
            return Err(Error::DrawingAlreadyInProgress);
        }
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.status != RaffleStatus::Active && raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStatus);
        }

        let now = env.ledger().timestamp();
        let time_ended = !raffle.no_deadline && now >= raffle.end_time;
        let tickets_full = raffle.tickets_sold >= raffle.max_tickets;

        if raffle.status == RaffleStatus::Active && !time_ended && !tickets_full {
            return Err(Error::InvalidStateTransition);
        }

        // #169: zero tickets sold is always a failure regardless of min_tickets,
        // ensuring the creator can recover their deposited prize via refund_prize.
        if raffle.tickets_sold == 0 || raffle.tickets_sold < raffle.min_tickets {
            let old_status = raffle.status.clone();
            raffle.status = RaffleStatus::Failed;
            write_raffle(&env, &raffle);

            let failure_reason = if raffle.tickets_sold == 0 {
                FailureReason::ZeroTicketsSold
            } else {
                FailureReason::MinTicketsNotMet
            };

            RaffleFailed {
                creator: raffle.creator.clone(),
                reason: failure_reason,
                tickets_sold: raffle.tickets_sold,
                timestamp: now,
            }
            .publish(&env);
            return Ok(());
        }

        let caller = raffle.creator.clone();
        let pre_drawing_status = raffle.status.clone();

        transition_to_drawing(&env, &mut raffle, now)?;

        if raffle.randomness_source == RandomnessSource::External {
            match request_randomness(&env) {
                Ok(request_id) => {
                    DrawTriggered {
                        caller: caller.clone(),
                        total_tickets_sold: raffle.tickets_sold,
                        timestamp: now,
                    }
                    .publish(&env);

                    RandomnessRequested {
                        oracle: raffle
                            .oracle_address
                            .clone()
                            .unwrap_or(env.current_contract_address()),
                        request_id,
                        timestamp: now,
                    }
                    .publish(&env);
                    return Ok(());
                }
                Err(err) => {
                    // SECURITY: lock rollback — oracle dispatch failed after status transition;
                    // clear DrawingLock and revert status so the contract is not permanently
                    // locked
                    raffle.status = pre_drawing_status;
                    write_raffle(&env, &raffle);
                    env.storage().instance().set(&DataKey::DrawingLock, &false);
                    return Err(err);
                }
            }
        }

        DrawTriggered {
            caller: caller.clone(),
            total_tickets_sold: raffle.tickets_sold,
            timestamp: now,
        }
        .publish(&env);

        if raffle.randomness_source == RandomnessSource::CommitReveal {
            // Collect entropy from all commit entries stored by ticket ID.
            //
            // We iterate over ticket IDs 1..=tickets_sold and read the
            // CommitEntry for each one.  Keying by ticket ID (rather than by
            // current owner address) is what makes the fix for #311: a
            // participant who committed and then transferred their ticket
            // still has their CommitEntry present under the original ticket
            // ID, so their entropy is never silently discarded.
            let mut combined = Bytes::new(&env);
            let mut commits_found: u32 = 0;
            for ticket_id in 1..=raffle.tickets_sold {
                if let Some(entry) = env
                    .storage()
                    .persistent()
                    .get::<_, CommitRevealEntry>(&DataKey::CommitEntry(ticket_id))
                {
                    combined.extend_from_array(&entry.hash.to_array());
                    commits_found += 1;
                }
            }

            // If no commits were submitted at all fall through to the
            // internal PRNG so the raffle can still be finalised.
            if commits_found > 0 {
                let hash: BytesN<32> = env.crypto().sha256(&combined).into();
                let arr = hash.to_array();
                let mut seed_bytes = [0u8; 8];
                seed_bytes.copy_from_slice(&arr[..8]);
                let seed = u64::from_be_bytes(seed_bytes);
                return self::do_finalize_with_seed(&env, raffle, seed, RandomnessType::Prng);
            }
        }

        let seed = build_internal_seed_u64(&env);
        self::do_finalize_with_seed(&env, raffle, seed, RandomnessType::Prng)
    }

    pub fn provide_randomness(
        env: Env,
        random_seed: u64,
        public_key: BytesN<32>,
        proof: BytesN<64>,
        request_id: u64,
    ) -> Result<Address, Error> {
        // # SECURITY: DrawingAlreadyComplete guard — DrawingLock=false means either winner
        // selection completed or the lock was never legitimately set; reject to prevent double
        // winner write
        let drawing_lock: bool = env
            .storage()
            .instance()
            .get(&DataKey::DrawingLock)
            .unwrap_or(false);
        if !drawing_lock {
            return Err(Error::DrawingAlreadyComplete);
        }

        let raffle = read_raffle(&env)?;

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

        // Verify request ID to prevent replay attacks
        let stored_request_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequestId)
            .ok_or(Error::NoRandomnessRequest)?;
        if stored_request_id != request_id {
            return Err(Error::InvalidParameters);
        }

        let message = Bytes::from_array(&env, &random_seed.to_be_bytes());
        env.crypto().ed25519_verify(&public_key, &message, &proof);

        RandomnessReceived {
            oracle,
            seed: random_seed,
            request_id,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        self::do_finalize_with_seed(&env, raffle, random_seed, RandomnessType::Vrf)?;
        Ok(env.current_contract_address())
    }

    pub fn trigger_randomness_fallback(
        env: Env,
        caller: Address,
        do_refund: bool,
    ) -> Result<(), Error> {
        // # SECURITY: fast-path guard — if DrawingLock is true, another Drawing transition is
        // already in progress; reject without reading further state
        let drawing_lock: bool = env
            .storage()
            .instance()
            .get(&DataKey::DrawingLock)
            .unwrap_or(false);
        if drawing_lock {
            return Err(Error::DrawingAlreadyInProgress);
        }

        caller.require_auth();
        let mut raffle = read_raffle(&env)?;

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

        if do_refund {
            raffle.status = RaffleStatus::Cancelled;
            write_raffle(&env, &raffle);

            // Clear pending randomness and DrawingLock when cancelling
            env.storage()
                .instance()
                .remove(&DataKey::RandomnessRequested);
            env.storage()
                .instance()
                .remove(&DataKey::RandomnessRequestId);
            env.storage()
                .instance()
                .remove(&DataKey::RandomnessRequestLedger);
            env.storage().instance().set(&DataKey::DrawingLock, &false);

            RaffleCancelled {
                creator: raffle.creator.clone(),
                reason: CancelReason::OracleTimeout,
                tickets_sold: raffle.tickets_sold,
                prize_refunded: raffle.prize_deposited,
                timestamp: env.ledger().timestamp(),
            }
            .publish(&env);
            return Ok(());
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
        let _guard = Guard::new(&env)?;
        let mut raffle = read_raffle(&env)?;

        if raffle.status != RaffleStatus::Finalized {
            return Err(Error::InvalidStatus);
        }

        // #259: enforce the configurable lockup delay.
        if let Some(finalized_at) = raffle.finalized_at {
            if env.ledger().timestamp() < finalized_at + raffle.claim_lockup_seconds {
                return Err(Error::ClaimTooEarly);
            }
        }

        if tier_index >= raffle.winners.len() {
            return Err(Error::InvalidParameters);
        }

        if raffle.winners.get(tier_index).ok_or(Error::InvalidIndex)? != winner {
            return Err(Error::NotWinner);
        }

        if raffle
            .claimed_winners
            .get(tier_index)
            .ok_or(Error::InvalidIndex)?
        {
            return Err(Error::PrizeAlreadyClaimed);
        }

        let amount = calculate_tier_prize(&raffle, tier_index)?;

        let fee = amount * (raffle.protocol_fee_bp as i128) / 10000;
        let net_amount = amount - fee;
        if net_amount <= 0 {
            return Err(Error::ZeroPrize);
        }

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
            RaffleStatusChanged {
                old_status: RaffleStatus::Finalized,
                new_status: RaffleStatus::Claimed,
                timestamp: env.ledger().timestamp(),
            }
            .publish(&env);
        }
        write_raffle(&env, &raffle);

        let token_client = token::Client::new(&env, &raffle.payment_token);
        let _ = token_client
            .try_transfer(&env.current_contract_address(), &winner, &net_amount)
            .map_err(|_| Error::TokenTransferFailed)?;

        if fee > 0 {
            if let Some(treasury) = &raffle.treasury_address {
                let _ = token_client
                    .try_transfer(&env.current_contract_address(), treasury, &fee)
                    .map_err(|_| Error::TokenTransferFailed)?;
            }
            let prev_fees: i128 = env
                .storage()
                .instance()
                .get(&DataKey::AccumulatedFees)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::AccumulatedFees, &(prev_fees + fee));
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

        Ok(net_amount)
    }

    pub fn withdraw_fees(env: Env, recipient: Address, amount: i128) -> Result<(), Error> {
        let _admin = require_admin(&env)?;

        let raffle = read_raffle(&env)?;
        if raffle.status != RaffleStatus::Finalized && raffle.status != RaffleStatus::Claimed {
            return Err(Error::InvalidStatus);
        }

        if amount <= 0 {
            return Err(Error::InvalidParameters);
        }

        let accumulated: i128 = env
            .storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0);
        if amount > accumulated {
            return Err(Error::InsufficientAccumulatedFees);
        }

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client.transfer(&env.current_contract_address(), &recipient, &amount);

        env.storage()
            .instance()
            .set(&DataKey::AccumulatedFees, &(accumulated - amount));

        FeesWithdrawn {
            recipient,
            amount,
            token: raffle.payment_token.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn get_accumulated_fees(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::AccumulatedFees)
            .unwrap_or(0)
    }

    pub fn cancel_raffle(env: Env, reason: CancelReason) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;

        match reason {
            CancelReason::AdminCancelled => {
                let admin: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::Admin)
                    .ok_or(Error::NotAuthorized)?;
                admin.require_auth();
            }
            _ => raffle.creator.require_auth(),
        }

        if raffle.status == RaffleStatus::Finalized
            || raffle.status == RaffleStatus::Cancelled
            || raffle.status == RaffleStatus::Claimed
        {
            return Err(Error::InvalidStatus);
        }

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
        let _ = token_client
            .try_transfer(
                &env.current_contract_address(),
                &raffle.creator,
                &raffle.prize_amount,
            )
            .map_err(|_| Error::TokenTransferFailed)?;

        PrizeRefunded {
            creator: raffle.creator.clone(),
            amount: raffle.prize_amount,
            token: raffle.payment_token.clone(),
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn emergency_withdraw(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let mut raffle = read_raffle(&env)?;

        if !raffle.prize_deposited {
            return Err(Error::PrizeNotDeposited);
        }

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotAuthorized)?;
        if caller != raffle.creator && caller != admin {
            return Err(Error::NotAuthorized);
        }

        let now = env.ledger().timestamp();

        // Allow emergency withdraw only after a long timeout.
        match raffle.status {
            RaffleStatus::Finalized => {
                if let Some(finalized_at) = raffle.finalized_at {
                    if now < finalized_at + EMERGENCY_WITHDRAW_DELAY_SECONDS {
                        return Err(Error::EmergencyTooEarly);
                    }
                } else {
                    return Err(Error::EmergencyTooEarly);
                }
            }
            RaffleStatus::Drawing => {
                if raffle.end_time == 0 || now < raffle.end_time + EMERGENCY_WITHDRAW_DELAY_SECONDS
                {
                    return Err(Error::EmergencyTooEarly);
                }
            }
            _ => return Err(Error::InvalidStatus),
        }

        // Mark prize as withdrawn and transfer back to creator
        raffle.prize_deposited = false;
        raffle.status = RaffleStatus::Cancelled;
        write_raffle(&env, &raffle);

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client.transfer(
            &env.current_contract_address(),
            &raffle.creator,
            &raffle.prize_amount,
        );

        EmergencyWithdrawn {
            withdrawn_by: caller,
            to: raffle.creator.clone(),
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

        // #258: status check BEFORE require_auth to prevent double-spend on
        // status transitions that occur between auth and the gate.
        if raffle.status != RaffleStatus::Cancelled && raffle.status != RaffleStatus::Failed {
            return Err(Error::InvalidStatus);
        }

        let _guard = Guard::new(&env)?;
        let ticket: Ticket = env
            .storage()
            .persistent()
            .get(&DataKey::Ticket(ticket_id))
            .ok_or(Error::TicketNotFound)?;
        ticket.owner.require_auth();

        // Check if already refunded
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
        let _ = token_client
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

    pub fn get_raffle(env: Env) -> Result<Raffle, Error> {
        read_raffle(&env)
    }

    pub fn get_fairness_data(env: Env) -> Result<FairnessData, Error> {
        let metadata: FairnessMetadata = env
            .storage()
            .persistent()
            .get(&DataKey::RandomnessSeed)
            .ok_or(Error::InvalidStatus)?;
        let raffle = read_raffle(&env)?;
        let mut ticket_ids = Vec::new(&env);
        let count = raffle.tickets_sold;
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

        // Wipe ticket storage
        for i in 1..=raffle.tickets_sold {
            env.storage().persistent().remove(&DataKey::Ticket(i));
            env.storage()
                .persistent()
                .remove(&DataKey::TicketRefunded(i));
            env.storage()
                .persistent()
                .remove(&DataKey::CommitEntry(i));
        }

        let buyers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::TicketBuyers)
            .unwrap_or_else(|| Vec::new(&env));
        for buyer in buyers.iter() {
            env.storage()
                .persistent()
                .remove(&DataKey::TicketCount(buyer.clone()));
        }
        env.storage()
            .persistent()
            .remove(&DataKey::TicketBuyers);

        // Wipe instance storage
        env.storage().instance().remove(&DataKey::Raffle);
        env.storage().instance().remove(&DataKey::Factory);
        env.storage().instance().remove(&DataKey::Admin);
        env.storage().instance().remove(&DataKey::Paused);
        env.storage().instance().remove(&DataKey::ReentrancyGuard);
        env.storage().instance().remove(&DataKey::AccumulatedFees);
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequested);
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequestLedger);
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequestId);
        env.storage().instance().remove(&DataKey::DrawingLock);
        env.storage().instance().remove(&DataKey::FinishTime);

        // Wipe persistent instance-level keys
        env.storage()
            .persistent()
            .remove(&DataKey::RandomnessSeed);
        env.storage().persistent().remove(&DataKey::Admin);

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

    pub fn pause_ticket_sales(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let mut raffle = read_raffle(&env)?;
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotAuthorized)?;
        if caller != raffle.creator && caller != admin {
            return Err(Error::NotAuthorized);
        }
        if raffle.status != RaffleStatus::Active {
            return Err(Error::InvalidStatus);
        }
        raffle.ticket_sales_paused = true;
        write_raffle(&env, &raffle);

        TicketSalesPaused {
            paused_by: caller,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn resume_ticket_sales(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let mut raffle = read_raffle(&env)?;
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotAuthorized)?;
        if caller != raffle.creator && caller != admin {
            return Err(Error::NotAuthorized);
        }
        if raffle.status != RaffleStatus::Active {
            return Err(Error::InvalidStatus);
        }
        raffle.ticket_sales_paused = false;
        write_raffle(&env, &raffle);

        TicketSalesResumed {
            resumed_by: caller,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn is_ticket_sales_paused(env: Env) -> bool {
        read_raffle(&env)
            .map(|raffle| raffle.ticket_sales_paused)
            .unwrap_or(false)
    }

    /// Sweep tokens that were accidentally sent to this contract.
    /// The raffle's own payment_token cannot be swept while a prize is held in escrow,
    /// ensuring active raffle funds are never at risk.
    pub fn rescue_tokens(
        env: Env,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotAuthorized)?;
        admin.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidParameters);
        }

        // Protect active escrow: block sweeping the raffle payment token while
        // the prize is deposited (i.e. the escrow is live).
        if let Ok(raffle) = read_raffle(&env) {
            if token == raffle.payment_token && raffle.prize_deposited {
                return Err(Error::InvalidParameters);
            }
        }

        let token_client = token::Client::new(&env, &token);
        let _ = token_client
            .try_transfer(&env.current_contract_address(), &recipient, &amount)
            .map_err(|_| Error::TokenTransferFailed)?;

        TokensRescued {
            rescued_by: admin,
            token,
            recipient,
            amount,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let _old_admin = require_admin(&env)?;

        // Block setting admin to the zero address (all-zero contract id).
        // Block setting admin to the zero address (all-zero contract id) or any non-existent address.
        if !new_admin.exists() {
            return Err(Error::InvalidAdminAddress);
        }

        // Block assigning the admin to this contract's own address.
        if new_admin == env.current_contract_address() {
            return Err(Error::InvalidAdminAddress);
        }

        env.storage().persistent().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Admin-only migration hook for the trusted randomness oracle contract.
    pub fn update_oracle_address(env: Env, new_oracle: Address) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        let mut raffle = read_raffle(&env)?;

        if raffle.randomness_source != RandomnessSource::External {
            return Err(Error::InvalidParameters);
        }
        if new_oracle == env.current_contract_address() {
            return Err(Error::InvalidParameters);
        }
        if raffle.status == RaffleStatus::Finalized
            || raffle.status == RaffleStatus::Claimed
            || raffle.status == RaffleStatus::Cancelled
        {
            return Err(Error::InvalidStatus);
        }

        let old_oracle = raffle.oracle_address.clone();
        raffle.oracle_address = Some(new_oracle.clone());
        write_raffle(&env, &raffle);

        OracleAddressUpdated {
            old_oracle,
            new_oracle,
            updated_by: admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    /// Admin-only adjustment of the per-raffle protocol fee before ticket sales begin.
    pub fn set_protocol_fee_bp(env: Env, new_fee_bp: u32) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        if new_fee_bp > MAX_PROTOCOL_FEE_BP {
            return Err(Error::InvalidParameters);
        }

        let mut raffle = read_raffle(&env)?;
        if raffle.tickets_sold > 0 {
            return Err(Error::InvalidStatus);
        }

        let old_fee_bp = raffle.protocol_fee_bp;
        raffle.protocol_fee_bp = new_fee_bp;
        write_raffle(&env, &raffle);

        ProtocolFeeUpdated {
            old_fee_bp,
            new_fee_bp,
            updated_by: admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn set_swap_deadline(env: Env, new_deadline_seconds: u64) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        if new_deadline_seconds > MAX_SWAP_DEADLINE_SECONDS {
            return Err(Error::InvalidParameters);
        }

        let mut raffle = read_raffle(&env)?;
        if raffle.tickets_sold > 0 {
            return Err(Error::InvalidStatus);
        }

        let old_deadline_seconds = raffle.swap_deadline_seconds;
        raffle.swap_deadline_seconds = new_deadline_seconds;
        write_raffle(&env, &raffle);

        SwapDeadlineUpdated {
            old_deadline_seconds,
            new_deadline_seconds,
            updated_by: admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }
}

fn do_finalize_with_seed(
    env: &Env,
    mut raffle: Raffle,
    seed: u64,
    randomness_type: RandomnessType,
) -> Result<(), Error> {
    let total_tickets = raffle.tickets_sold;
    if total_tickets == 0 {
        return Err(Error::NoTicketsSold);
    }
    if raffle.prizes.len() > total_tickets {
        return Err(Error::MorePrizesThanTickets);
    }

    // #256: Guard against all tickets being refunded after the draw window
    // opened but before finalize runs, which would make the winners Vec empty
    // and cause a panic on the winner_index lookup.
    let active_count = raffle.tickets_sold;
    if active_count == 0 {
        return Err(Error::NoActiveTickets);
    }

    let selector = OracleSeedWinnerSelection::new(seed);
    let winning_ticket_ids =
        selector.select_winner_indices(env, total_tickets, raffle.prizes.len());
    let mut winners = Vec::new(env);

    for i in 0..winning_ticket_ids.len() {
        let winner_index = winning_ticket_ids.get(i).ok_or(Error::InvalidIndex)?;
        let ticket_id = winner_index + 1;
        let winner = get_ticket_owner(env, ticket_id).ok_or(Error::TicketNotFound)?;
        winners.push_back(winner.clone());

        WinnerDrawn {
            winner,
            ticket_id: winner_index,
            tier_index: i,
            timestamp: env.ledger().timestamp(),
        }
        .publish(env);
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
    write_raffle(env, &raffle);

    // Clear pending randomness state
    env.storage()
        .instance()
        .remove(&DataKey::RandomnessRequested);
    env.storage()
        .instance()
        .remove(&DataKey::RandomnessRequestId);
    env.storage()
        .instance()
        .remove(&DataKey::RandomnessRequestLedger);
    // # SECURITY: Clear DrawingLock LAST, after all winner state is committed
    env.storage().instance().set(&DataKey::DrawingLock, &false);

    RaffleFinalized {
        raffle_id: env.current_contract_address(),
        winners,
        winning_ticket_ids,
        total_tickets_sold: raffle.tickets_sold,
        randomness_source: raffle.randomness_source.clone(),
        randomness_type,
        finalized_at: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use raffle_shared::RaffleConfig;
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{vec, Address, BytesN, Env, String};

    // Deploy a Stellar Asset Contract we control, return (token_client, admin_client).
    fn create_token<'a>(env: &Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        let addr = sac.address();
        (addr.clone(), token::StellarAssetClient::new(env, &addr))
    }
    #[contract]
    pub struct MockFactory;

    #[contractimpl]
    impl MockFactory {
        pub fn record_volume(_env: Env, _token: Address, _amount: i128) {}
        pub fn track_participant(_env: Env, _participant: Address) {}
    }

    #[test]
    fn non_winner_cannot_claim() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        // Players
        let factory = env.register(MockFactory, ());
        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let buyer = Address::generate(&env);
        let attacker = Address::generate(&env);

        // Payment token, funded
        let token_admin = Address::generate(&env);
        let (token_addr, token_mint) = create_token(&env, &token_admin);
        token_mint.mint(&creator, &1_000_000);
        token_mint.mint(&buyer, &1_000_000);

        // One prize tier worth 100% (10000 bp)
        let config = RaffleConfig {
            description: String::from_str(&env, "test raffle"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 1,
            max_tickets_per_tx: 1,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: MIN_TICKET_PRICE,
            payment_token: token_addr.clone(),
            prize_amount: MIN_TICKET_PRICE * 10,
            prizes: vec![&env, 10000u32],
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
            claim_lockup_seconds: 0, // => DEFAULT_CLAIM_LOCKUP_SECONDS (3600)
            swap_deadline_seconds: 0,
        };

        client.init(&factory, &admin, &creator, &config);
        client.deposit_prize();
        client.buy_tickets(&buyer, &1);
        client.finalize_raffle();

        // Sanity: a winner is now recorded, and it is NOT the attacker.
        let raffle = client.get_raffle();
        assert_eq!(raffle.winners.len(), 1);
        assert!(raffle.winners.get(0).unwrap() != attacker);

        // Advance past the claim lockup so we reach the winner check, not ClaimTooEarly.
        env.ledger()
            .set_timestamp(1_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);

        // Attacker authenticates fine (mock_all_auths) but is not the winner.
        let result = client.try_claim_prize(&attacker, &0u32);
        assert_eq!(result, Err(Ok(Error::NotWinner)));
    }

    #[test]
    fn buy_tickets_rejects_quantity_above_per_tx_cap() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        let factory = env.register(MockFactory, ());
        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let buyer = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let (token_addr, token_mint) = create_token(&env, &token_admin);
        token_mint.mint(&creator, &1_000_000);
        token_mint.mint(&buyer, &1_000_000);

        let config = RaffleConfig {
            description: String::from_str(&env, "Per-tx cap"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 100,
            max_tickets_per_tx: 5,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: MIN_TICKET_PRICE,
            payment_token: token_addr.clone(),
            prize_amount: MIN_TICKET_PRICE * 100,
            prizes: vec![&env, 10000u32],
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(&env, &[5u8; 32]),
            claim_lockup_seconds: 0,
            swap_deadline_seconds: 0,
        };

        client.init(&factory, &admin, &creator, &config);
        client.deposit_prize();

        assert_eq!(
            client.try_buy_tickets(&buyer, &6),
            Err(Ok(Error::ExceedsMaxTicketsPerTx))
        );
        assert_eq!(client.buy_tickets(&buyer, &5), 5);
    }

    fn setup_active_raffle(
        env: &Env,
    ) -> (
        ContractClient<'_>,
        Address,
        Address,
        Address,
        Address,
        token::StellarAssetClient<'_>,
    ) {
        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(env, &contract_id);

        let factory = env.register(MockFactory, ());
        let admin = Address::generate(env);
        let creator = Address::generate(env);
        let buyer = Address::generate(env);

        let token_admin = Address::generate(env);
        let (token_addr, token_mint) = create_token(env, &token_admin);
        token_mint.mint(&creator, &1_000_000);
        token_mint.mint(&buyer, &1_000_000);

        let config = RaffleConfig {
            description: String::from_str(env, "ticket sales pause"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 100,
            max_tickets_per_tx: 10,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: MIN_TICKET_PRICE,
            payment_token: token_addr,
            prize_amount: MIN_TICKET_PRICE * 100,
            prizes: vec![env, 10000u32],
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(env, &[7u8; 32]),
            claim_lockup_seconds: 0,
            swap_deadline_seconds: 0,
        };

        client.init(&factory, &admin, &creator, &config);
        client.deposit_prize();

        (client, admin, creator, buyer, factory, token_mint)
    }

    #[test]
    fn pause_resume_ticket_sales_controls_buy_tickets() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let (client, _admin, creator, buyer, _factory, _token_mint) = setup_active_raffle(&env);

        assert_eq!(client.get_raffle().status, RaffleStatus::Active);
        assert!(!client.is_ticket_sales_paused());

        client.pause_ticket_sales(&creator);
        assert!(client.is_ticket_sales_paused());
        assert_eq!(client.get_raffle().status, RaffleStatus::Active);
        assert_eq!(
            client.try_buy_tickets(&buyer, &1),
            Err(Ok(Error::ContractPaused))
        );

        client.resume_ticket_sales(&creator);
        assert!(!client.is_ticket_sales_paused());
        assert_eq!(client.get_raffle().status, RaffleStatus::Active);
        assert_eq!(client.buy_tickets(&buyer, &1), 1);
    }

    #[test]
    fn admin_can_pause_and_resume_ticket_sales() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let (client, admin, _creator, buyer, _factory, _token_mint) = setup_active_raffle(&env);

        client.pause_ticket_sales(&admin);
        assert!(client.is_ticket_sales_paused());
        assert_eq!(
            client.try_buy_tickets(&buyer, &1),
            Err(Ok(Error::ContractPaused))
        );

        client.resume_ticket_sales(&admin);
        assert!(!client.is_ticket_sales_paused());
        assert_eq!(client.buy_tickets(&buyer, &1), 1);
    }

    #[test]
    fn test_wipe_storage_removes_all_keys() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

        let factory = env.register(MockFactory, ());
        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let buyer_a = Address::generate(&env);
        let buyer_b = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let (token_addr, token_mint) = create_token(&env, &token_admin);
        token_mint.mint(&creator, &1_000_000);
        token_mint.mint(&buyer_a, &1_000_000);
        token_mint.mint(&buyer_b, &1_000_000);

        let config = RaffleConfig {
            description: String::from_str(&env, "wipe test"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 10,
            max_tickets_per_tx: 10,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: MIN_TICKET_PRICE,
            payment_token: token_addr,
            prize_amount: MIN_TICKET_PRICE * 10,
            prizes: vec![&env, 10000u32],
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(&env, &[9u8; 32]),
            claim_lockup_seconds: 0,
            swap_deadline_seconds: 0,
        };

        client.init(&factory, &admin, &creator, &config);
        client.deposit_prize();
        client.buy_tickets(&buyer_a, &3);
        client.buy_tickets(&buyer_b, &2);

        client.cancel_raffle(&raffle_shared::CancelReason::AdminCancelled);

        assert_eq!(client.get_raffle().status, RaffleStatus::Cancelled);

        client.wipe_storage();

        env.as_contract(&contract_id, || {
            for i in 1..=5 {
                assert!(!env
                    .storage()
                    .persistent()
                    .has(&DataKey::Ticket(i)));
                assert!(!env
                    .storage()
                    .persistent()
                    .has(&DataKey::TicketRefunded(i)));
                assert!(!env
                    .storage()
                    .persistent()
                    .has(&DataKey::CommitEntry(i)));
            }
            assert!(!env
                .storage()
                .persistent()
                .has(&DataKey::TicketCount(buyer_a.clone())));
            assert!(!env
                .storage()
                .persistent()
                .has(&DataKey::TicketCount(buyer_b.clone())));
            assert!(!env
                .storage()
                .persistent()
                .has(&DataKey::TicketBuyers));

            assert!(!env.storage().instance().has(&DataKey::Raffle));
            assert!(!env.storage().instance().has(&DataKey::Factory));
            assert!(!env.storage().instance().has(&DataKey::Admin));
            assert!(!env.storage().instance().has(&DataKey::Paused));
            assert!(!env
                .storage()
                .instance()
                .has(&DataKey::ReentrancyGuard));
            assert!(!env
                .storage()
                .instance()
                .has(&DataKey::AccumulatedFees));
            assert!(!env
                .storage()
                .instance()
                .has(&DataKey::RandomnessRequested));
            assert!(!env
                .storage()
                .instance()
                .has(&DataKey::RandomnessRequestLedger));
            assert!(!env
                .storage()
                .instance()
                .has(&DataKey::RandomnessRequestId));
            assert!(!env
                .storage()
                .instance()
                .has(&DataKey::DrawingLock));
            assert!(!env
                .storage()
                .instance()
                .has(&DataKey::FinishTime));
            assert!(!env
                .storage()
                .persistent()
                .has(&DataKey::RandomnessSeed));
            assert!(!env
                .storage()
                .persistent()
                .has(&DataKey::Admin));
        });
    }
}
