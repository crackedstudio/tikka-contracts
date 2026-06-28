#![no_std]

use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RaffleStatus {
    /// Raffle exists in storage but the creator has not yet deposited the prize.
    /// Ticket sales, draws, and finalization are all disallowed in this state.
    /// Added in #225 so off-chain indexers can observe the explicit transition
    /// to `Active` once the prize is funded.
    PendingPrize = 6,
    Active = 0,
    Drawing = 1,
    Finalized = 2,
    Cancelled = 3,
    Failed = 4,
    Claimed = 5,
    Finalizing = 7,
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
pub enum FailureReason {
    ZeroTicketsSold = 0,
    MinTicketsNotMet = 1,
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RandomnessSource {
    Internal = 0,
    External = 1,
    CommitReveal = 2,
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RandomnessType {
    Prng = 0,
    Vrf = 1,
    Fallback = 2,
}

#[derive(Clone)]
#[contracttype]
pub struct RaffleConfig {
    pub description: String,
    pub end_time: u64,
    pub no_deadline: bool,
    pub max_tickets: u32,
    /// Maximum tickets a single address may purchase per transaction.
    pub max_tickets_per_tx: u32,
    pub min_tickets: u32,
    pub allow_multiple: bool,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub prize_amount: i128,
    pub prizes: Vec<u32>,
    pub randomness_source: RandomnessSource,
    pub oracle_address: Option<Address>,
    pub protocol_fee_bp: u32,
    pub treasury_address: Option<Address>,
    pub swap_router: Option<Address>,
    pub tikka_token: Option<Address>,
    pub metadata_hash: BytesN<32>,
    /// Seconds after finalization before winners may claim.
    /// Must be in [0, 604800] (0 to 7 days). Defaults to 3600 if zero.
    pub claim_lockup_seconds: u64,
    /// Swap deadline window in seconds (added to current timestamp for token swaps).
    /// Defaults to 300 (5 minutes) if zero. Configurable to handle network congestion.
    pub swap_deadline_seconds: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct Ticket {
    pub id: u32,
    pub owner: Address,
    pub purchase_time: u64,
    pub ticket_number: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct FairnessData {
    pub seed: u64,
    pub randomness_source: RandomnessSource,
    pub ticket_ids: Vec<u32>,
    pub winning_ticket_indices: Vec<u32>,
    pub draw_timestamp: u64,
    pub draw_sequence: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct PaginationParams {
    pub limit: u32,
    pub offset: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct PageResultRaffles {
    pub items: Vec<Address>,
    pub total: u32,
    pub has_more: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct PageResultTickets {
    pub items: Vec<Ticket>,
    pub total: u32,
    pub has_more: bool,
}

#[derive(Clone)]
#[contracttype]
pub enum AdminOp {
    SetConfig(u32, Address),
    UpdateWasmHash(BytesN<32>),
}

pub const DEFAULT_PAGE_LIMIT: u32 = 100;
pub const MAX_PAGE_LIMIT: u32 = 200;

pub fn effective_limit(requested: u32) -> u32 {
    if requested == 0 {
        DEFAULT_PAGE_LIMIT
    } else if requested > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT
    } else {
        requested
    }
}

#[derive(Clone)]
#[contracttype]
pub struct RandomnessRequest {
    pub raffle_id: Address,
    pub request_id: u64,
    pub callback_address: Address,
}

#[soroban_sdk::contractclient(name = "RandomnessOracleClient")]
pub trait RandomnessOracleTrait {
    fn request_randomness(env: soroban_sdk::Env, request: RandomnessRequest);
}

#[soroban_sdk::contractclient(name = "RandomnessReceiverClient")]
pub trait RandomnessReceiverTrait {
    fn receive_randomness(env: soroban_sdk::Env, request_id: u64, random_seed: u64);
}
