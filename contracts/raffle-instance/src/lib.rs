#![no_std]

use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contracterror, contractimpl, token, xdr::ToXdr, Address, Bytes, BytesN,
    Env, IntoVal, String, Symbol, Val, Vec,
};

mod randomness;
mod events;

use raffle_shared::{
    RaffleConfig, RaffleStatus, CancelReason, RandomnessSource, RandomnessType,
    Ticket, FairnessData,
};

use self::randomness::{
    OracleSeedWinnerSelection, WinnerSelectionStrategy,
};

use crate::events::{
    PrizeClaimed, PrizeDeposited, PrizeRefunded, RaffleCancelled, RaffleCreated,
    RaffleFinalized, RaffleStatusChanged, RandomnessReceived,
    RandomnessRequested, TicketPurchased, TicketRefunded,
    WinnerDrawn, RandomnessFallbackTriggered,
    ContractPaused, ContractUnpaused,
};

const ORACLE_TIMEOUT_LEDGERS: u32 = 200;
pub const MAX_DESCRIPTION_LENGTH: u32 = 1000;
pub const MAX_TICKETS_LIMIT: u32 = 100_000;
pub const MIN_TICKET_PRICE: i128 = 10_000;
/// Default and bounds for the claim lockup delay (#259).
pub const DEFAULT_CLAIM_LOCKUP_SECONDS: u64 = 3_600;
pub const MAX_CLAIM_LOCKUP_SECONDS: u64 = 604_800; // 7 days

#[contract]
pub struct Contract;

#[soroban_sdk::contracttype]
#[derive(Clone)]
#[soroban_sdk::contracttype]
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
    /// Seconds after finalization before winners may claim (#259).
    pub claim_lockup_seconds: u64,
}

#[soroban_sdk::contracttype]
#[derive(Clone)]
#[soroban_sdk::contracttype]
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
    RandomnessRequestId,
    FinishTime,
    TotalTickets,
    AccumulatedFees,
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
    DeadlinePassed = 46,
    SlippageExceeded = 47,
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
        .persistent()
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
        .persistent()
        .get(&DataKey::NextTicketId)
        .unwrap_or(0u32);
    let next = current + 1;
    env.storage().persistent().set(&DataKey::NextTicketId, &next);
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

// Helper to enforce slippage and deadline guards for token swaps
fn enforce_swap_guard(env: &Env, amount_out: i128, min_amount_out: i128, deadline: u64) -> Result<(), Error> {
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

        if config.protocol_fee_bp > 10000 {
            return Err(Error::InvalidParameters);
        }

        if config.randomness_source == RandomnessSource::External && config.oracle_address.is_none()
        {
            return Err(Error::InvalidParameters);
        }

        if config.metadata_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(Error::InvalidParameters);
        }

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
            winner_ticket_id: None,
            claim_lockup_seconds,
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
        }.publish(&env);

        Ok(())
    }

    pub fn deposit_prize(env: Env) -> Result<(), Error> {
        require_not_paused(&env)?;
        let mut raffle = read_raffle(&env)?;
        raffle.creator.require_auth();

        if raffle.prize_deposited {
            return Err(Error::PrizeAlreadyDeposited);
        }

        let _old_status = raffle.status.clone();
        raffle.prize_deposited = true;
        write_raffle(&env, &raffle);
        let old_status = raffle.status.clone();

        // Move tokens first. If the transfer fails we want the contract state
        // (prize_deposited flag, raffle.status) to remain untouched.
        let token_client = token::Client::new(&env, &raffle.payment_token);
        let contract_address = env.current_contract_address();
        token_client
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
        }.publish(&env);

        RaffleStatusChanged {
            old_status,
            new_status: RaffleStatus::Active,
            timestamp,
        }.publish(&env);

        Ok(())
    }

    pub fn buy_tickets(env: Env, buyer: Address, quantity: u32) -> Result<u32, Error> {
        require!(quantity > 0, Error::InvalidQuantity);
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

        if raffle.tickets_sold + quantity > raffle.max_tickets {
            return Err(Error::TicketsSoldOut);
        }

        let current_count: u32 = env.storage().persistent().get(&DataKey::TicketCount(buyer.clone())).unwrap_or(0);
        if !raffle.allow_multiple && (current_count > 0 || quantity > 1) {
            return Err(Error::MultipleTicketsNotAllowed);
        }

        let mut ticket_ids = Vec::new(&env);
        let timestamp = env.ledger().timestamp();
        let total_price = raffle
            .ticket_price
            .checked_mul(quantity as i128)
            .ok_or(Error::InvalidParameters)?;

        let protocol_fee = total_price
            .checked_mul(raffle.protocol_fee_bp as i128)
            .ok_or(Error::ArithmeticOverflow)? / 10000;
        let net_amount = total_price - protocol_fee;

        for _ in 0..quantity {
            let ticket_id = next_ticket_id(&env);
            raffle.tickets_sold += 1;

            let ticket = Ticket {
                id: ticket_id,
                owner: buyer.clone(),
                purchase_time: timestamp,
                ticket_number: raffle.tickets_sold,
            };
            env.storage().persistent().set(&DataKey::Ticket(ticket_id), &ticket);
            ticket_ids.push_back(ticket_id);
        }

        if raffle.tickets_sold >= raffle.max_tickets {
            let old_status = raffle.status.clone();
            raffle.status = RaffleStatus::Drawing;
            RaffleStatusChanged {
                old_status,
                new_status: RaffleStatus::Drawing,
                timestamp,
            }.publish(&env);
        }

        env.storage().persistent().set(&DataKey::TicketCount(buyer.clone()), &(current_count + quantity));
        write_raffle(&env, &raffle);

        if let Some(factory_address) = env.storage().instance().get::<_, Address>(&DataKey::Factory) {
            let contract_address = env.current_contract_address();
            let record_volume_args: Vec<Val> =
                (contract_address.clone(), raffle.payment_token.clone(), total_price)
                    .into_val(&env);

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
        token_client
            .try_transfer(&buyer, &env.current_contract_address(), &total_price)
            .map_err(|_| Error::TokenTransferFailed)?;

        if protocol_fee > 0 {
            if let Some(treasury) = &raffle.treasury_address {
                token_client.transfer(&env.current_contract_address(), treasury, &protocol_fee);
            }
            let prev_fees: i128 = env.storage().instance().get(&DataKey::AccumulatedFees).unwrap_or(0);
            env.storage().instance().set(&DataKey::AccumulatedFees, &(prev_fees + protocol_fee));
        }

        TicketPurchased {
            buyer,
            ticket_ids,
            quantity,
            ticket_price: raffle.ticket_price,
            total_paid: total_price,
            protocol_fee,
            timestamp,
        }.publish(&env);

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
            }.publish(&env);
            return Ok(());
        }

        if raffle.randomness_source == RandomnessSource::External {
            let already: bool = env.storage().instance().get(&DataKey::RandomnessRequested).unwrap_or(false);
            if already {
                return Err(Error::RandomnessAlreadyRequested);
            }
            
            // Generate unique request ID to prevent replay attacks
            let request_id_seed = Bytes::from_array(&env, &(
                env.ledger().timestamp(),
                env.ledger().sequence(),
                env.current_contract_address().to_xdr(&env),
            ).to_xdr(&env));
            let request_id_hash: BytesN<32> = env.crypto().sha256(&request_id_seed).into();
            let mut id_bytes = [0u8; 8];
            for i in 0..8 {
                id_bytes[i] = request_id_hash.get(i as u32).unwrap();
            }
            let request_id = u64::from_be_bytes(id_bytes);
            
            env.storage().instance().set(&DataKey::RandomnessRequested, &true);
            env.storage().instance().set(&DataKey::RandomnessRequestLedger, &env.ledger().sequence());
            env.storage().instance().set(&DataKey::RandomnessRequestId, &request_id);

            RandomnessRequested {
                oracle: raffle.oracle_address.clone().unwrap_or(env.current_contract_address()),
                request_id,
                timestamp: now,
            }.publish(&env);
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
        request_id: u64,
    ) -> Result<Address, Error> {
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

        let request_pending: bool = env.storage().instance().get(&DataKey::RandomnessRequested).unwrap_or(false);
        if !request_pending {
            return Err(Error::NoRandomnessRequest);
        }

        // Verify request ID to prevent replay attacks
        let stored_request_id: u64 = env.storage().instance().get(&DataKey::RandomnessRequestId).ok_or(Error::NoRandomnessRequest)?;
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
        }.publish(&env);

        self::do_finalize_with_seed(&env, raffle, random_seed, RandomnessType::Vrf)?;
        Ok(env.current_contract_address())
    }

    pub fn trigger_randomness_fallback(env: Env, caller: Address, do_refund: bool) -> Result<(), Error> {
        caller.require_auth();
        let mut raffle = read_raffle(&env)?;

        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotAuthorized)?;
        if caller != raffle.creator && caller != admin {
            return Err(Error::NotAuthorized);
        }

        if raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStateTransition);
        }

        let request_pending: bool = env.storage().instance().get(&DataKey::RandomnessRequested).unwrap_or(false);
        if !request_pending {
            return Err(Error::NoRandomnessRequest);
        }

        let request_ledger: u32 = env.storage().instance().get(&DataKey::RandomnessRequestLedger).unwrap_or(0);
        if env.ledger().sequence() < request_ledger + ORACLE_TIMEOUT_LEDGERS {
            return Err(Error::FallbackTooEarly);
        }

        if do_refund {
            raffle.status = RaffleStatus::Cancelled;
            write_raffle(&env, &raffle);

            RaffleCancelled {
                creator: raffle.creator.clone(),
                reason: CancelReason::OracleTimeout,
                tickets_sold: raffle.tickets_sold,
                prize_refunded: raffle.prize_deposited,
                timestamp: env.ledger().timestamp(),
            }.publish(&env);
            return Ok(());
        }

        let seed = build_internal_seed_u64(&env);
        
        RandomnessFallbackTriggered {
            triggered_by: caller,
            seed_used: seed,
            request_ledger,
            fallback_ledger: env.ledger().sequence(),
            timestamp: env.ledger().timestamp(),
        }.publish(&env);

        self::do_finalize_with_seed(&env, raffle, seed, RandomnessType::Fallback)
    }

    pub fn claim_prize(env: Env, winner: Address, tier_index: u32) -> Result<i128, Error> {
        winner.require_auth();
        let _guard = Guard::new(&env)?;
        let mut raffle = read_raffle(&env)?;

        if raffle.status != RaffleStatus::Finalized {
            return Err(Error::InvalidStatus);
        }

        if tier_index >= raffle.winners.len() {
        // #259: enforce the configurable lockup delay.
        if let Some(finalized_at) = raffle.finalized_at {
            if env.ledger().timestamp() < finalized_at + raffle.claim_lockup_seconds {
                return Err(Error::ClaimTooEarly);
            }
        }

        if tier_index as usize >= raffle.winners.len() {
            return Err(Error::InvalidParameters);
        }

        if raffle.winners.get(tier_index).ok_or(Error::InvalidIndex)? != winner {
            return Err(Error::NotWinner);
        }

        if raffle.claimed_winners.get(tier_index).ok_or(Error::InvalidIndex)? {
            return Err(Error::PrizeAlreadyClaimed);
        }

        let amount = calculate_tier_prize(&raffle, tier_index)?;

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
            RaffleStatusChanged {
                old_status: RaffleStatus::Finalized,
                new_status: RaffleStatus::Claimed,
                timestamp: env.ledger().timestamp(),
            }.publish(&env);
        }
        write_raffle(&env, &raffle);

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client
            .try_transfer(&env.current_contract_address(), &winner, &net_amount)
            .map_err(|_| Error::TokenTransferFailed)?;

        if fee > 0 {
            if let Some(treasury) = &raffle.treasury_address {
                token_client
                    .try_transfer(&env.current_contract_address(), treasury, &fee)
                    .map_err(|_| Error::TokenTransferFailed)?;
            }
            let prev_fees: i128 = env.storage().instance().get(&DataKey::AccumulatedFees).unwrap_or(0);
            env.storage().instance().set(&DataKey::AccumulatedFees, &(prev_fees + fee));
        }

        PrizeClaimed {
            winner,
            tier_index,
            payment_token: raffle.payment_token.clone(),
            gross_amount: amount,
            net_amount,
            platform_fee: fee,
            claimed_at: env.ledger().timestamp(),
        }.publish(&env);

        Ok(net_amount)
    }

    pub fn cancel_raffle(env: Env, reason: CancelReason) -> Result<(), Error> {
        let mut raffle = read_raffle(&env)?;

        match reason {
            CancelReason::AdminCancelled => {
                let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotAuthorized)?;
                admin.require_auth();
            }
            _ => raffle.creator.require_auth(),
        }

        if raffle.status == RaffleStatus::Finalized || raffle.status == RaffleStatus::Cancelled || raffle.status == RaffleStatus::Claimed {
            return Err(Error::InvalidStatus);
        }

        let _old_status = raffle.status.clone();
        raffle.status = RaffleStatus::Cancelled;
        write_raffle(&env, &raffle);

        RaffleCancelled {
            creator: raffle.creator.clone(),
            reason,
            tickets_sold: raffle.tickets_sold,
            prize_refunded: raffle.prize_deposited,
            timestamp: env.ledger().timestamp(),
        }.publish(&env);

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
        token_client
            .try_transfer(&env.current_contract_address(), &raffle.creator, &raffle.prize_amount)
            .map_err(|_| Error::TokenTransferFailed)?;

        PrizeRefunded {
            creator: raffle.creator.clone(),
            amount: raffle.prize_amount,
            token: raffle.payment_token.clone(),
            timestamp: env.ledger().timestamp(),
        }.publish(&env);

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
        let ticket: Ticket = env.storage().persistent().get(&DataKey::Ticket(ticket_id)).ok_or(Error::TicketNotFound)?;
        ticket.owner.require_auth();

        // Check if already refunded
        let refund_key = (DataKey::Ticket(ticket_id), Symbol::new(&env, "refunded"));
        if env.storage().persistent().has(&refund_key) {
            return Err(Error::InvalidStatus); // Already refunded
        }

        env.storage().persistent().set(&refund_key, &true);

        let token_client = token::Client::new(&env, &raffle.payment_token);
        token_client
            .try_transfer(&env.current_contract_address(), &ticket.owner, &raffle.ticket_price)
            .map_err(|_| Error::TokenTransferFailed)?;

        TicketRefunded {
            buyer: ticket.owner,
            ticket_number: ticket.ticket_number,
            amount: raffle.ticket_price,
            timestamp: env.ledger().timestamp(),
        }.publish(&env);

        Ok(raffle.ticket_price)
    }

    pub fn get_raffle(env: Env) -> Result<Raffle, Error> {
        read_raffle(&env)
    }

    pub fn get_fairness_data(env: Env) -> Result<FairnessData, Error> {
        let metadata: FairnessMetadata = env.storage().instance().get(&DataKey::RandomnessSeed).ok_or(Error::InvalidStatus)?;
        let _raffle = read_raffle(&env)?;
        
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
        let factory: Address = env.storage().instance().get(&DataKey::Factory).ok_or(Error::NotAuthorized)?;
        factory.require_auth();

        let raffle = read_raffle(&env)?;
        if raffle.status != RaffleStatus::Cancelled && raffle.status != RaffleStatus::Claimed && raffle.status != RaffleStatus::Failed {
            return Err(Error::InvalidStatus);
        }

        // Wipe all storage
        env.storage().instance().remove(&DataKey::Raffle);
        // ... (other keys)
        Ok(())
    }

    pub fn pause(env: Env) -> Result<(), Error> {
        let factory: Address = env.storage().instance().get(&DataKey::Factory).ok_or(Error::NotAuthorized)?;
        factory.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);

        ContractPaused {
            paused_by: factory,
            timestamp: env.ledger().timestamp(),
        }.publish(&env);

        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), Error> {
        let factory: Address = env.storage().instance().get(&DataKey::Factory).ok_or(Error::NotAuthorized)?;
        factory.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);

        ContractUnpaused {
            unpaused_by: factory,
            timestamp: env.ledger().timestamp(),
        }.publish(&env);

        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotAuthorized)?;
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
    if raffle.prizes.len() as u32 > total_tickets {
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
        selector.select_winner_indices(env, total_tickets, raffle.prizes.len() as u32);
    let mut winners = Vec::new(env);

    for i in 0..winning_ticket_ids.len() {
        let winner_index = winning_ticket_ids.get(i).ok_or(Error::InvalidIndex)?;
        let ticket_id = winner_index + 1;
        let winner = get_ticket_owner(env, ticket_id).ok_or(Error::TicketNotFound)?;
        winners.push_back(winner.clone());

        WinnerDrawn {
            winner,
            ticket_id,
            tier_index: i,
            timestamp: env.ledger().timestamp(),
        }.publish(&env);
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

    // Clear pending randomness state
    env.storage().instance().remove(&DataKey::RandomnessRequested);
    env.storage().instance().remove(&DataKey::RandomnessRequestId);
    env.storage().instance().remove(&DataKey::RandomnessRequestLedger);

    RaffleFinalized {
        winners,
        winning_ticket_ids,
        total_tickets_sold: raffle.tickets_sold,
        randomness_source: raffle.randomness_source.clone(),
        randomness_type,
        finalized_at: env.ledger().timestamp(),
    }.publish(&env);

    Ok(())
}

#[cfg(test)]
mod test;
