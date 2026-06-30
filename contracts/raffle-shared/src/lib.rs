#![no_std]

use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

/// Lifecycle state of a raffle instance.
///
/// Transitions are enforced by contract logic and represent the canonical
/// on-chain lifecycle used by indexers and clients.
#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RaffleStatus {
    /// Raffle exists in storage but the creator has not yet deposited the prize.
    /// Ticket sales, draws, and finalization are all disallowed in this state.
    /// Added in #225 so off-chain indexers can observe the explicit transition
    /// to `Active` once the prize is funded.
    PendingPrize = 6,
    /// Prize is funded and ticket sales are open.
    Active = 0,
    /// Draw has started and randomness is pending or being processed.
    Drawing = 1,
    /// Winners are selected and claims can be processed.
    Finalized = 2,
    /// Raffle was cancelled before successful completion.
    Cancelled = 3,
    /// Raffle failed terminally (for example, minimum ticket requirements unmet).
    Failed = 4,
    /// Finalized raffle where all winners have completed claims.
    Claimed = 5,
}

/// Canonical reason explaining why a raffle entered `Cancelled`.
#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum CancelReason {
    /// Cancellation requested by the raffle creator.
    CreatorCancelled = 0,
    /// Administrative cancellation by protocol governance/admin.
    AdminCancelled = 1,
    /// Oracle did not return randomness in time.
    OracleTimeout = 2,
    /// Raffle cancelled because minimum ticket threshold was not met.
    MinTicketsNotMet = 3,
}

/// Canonical reason explaining why a raffle entered `Failed`.
#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum FailureReason {
    /// No tickets were sold before finalization.
    ZeroTicketsSold = 0,
    /// Tickets sold were below the configured minimum requirement.
    MinTicketsNotMet = 1,
}

/// Source used to generate randomness for winner selection.
#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RandomnessSource {
    /// Internal pseudo-randomness generated on-chain.
    Internal = 0,
    /// External oracle-provided randomness.
    External = 1,
    /// Commit-reveal based randomness source.
    CommitReveal = 2,
}

/// Type/classification of randomness mechanism requested or received.
#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RandomnessType {
    /// Pseudo-random sequence generated deterministically from chain context.
    Prng = 0,
    /// Verifiable random function backed randomness.
    Vrf = 1,
    /// Fallback path used when preferred randomness path is unavailable.
    Fallback = 2,
}

/// Configuration payload used when creating a new raffle.
///
/// Values are validated by contract initialization before the raffle becomes
/// active and represent the complete raffle policy surface.
#[derive(Clone)]
#[contracttype]
pub struct RaffleConfig {
    /// Human-readable raffle description.
    pub description: String,
    /// Unix timestamp when ticket sales close (ignored when `no_deadline` is true).
    pub end_time: u64,
    /// If true, raffle can remain open without a hard end timestamp.
    pub no_deadline: bool,
    /// Maximum number of tickets that can ever be sold.
    pub max_tickets: u32,
    /// Maximum tickets a single address may purchase per transaction.
    pub max_tickets_per_tx: u32,
    /// Minimum number of tickets required for a successful draw.
    pub min_tickets: u32,
    /// Whether one address may own multiple tickets.
    pub allow_multiple: bool,
    /// Price per ticket denominated in the payment token's base units.
    pub ticket_price: i128,
    /// Soroban address for the token used to buy tickets.
    pub payment_token: Address,
    /// Total prize amount denominated in the same payment token.
    pub prize_amount: i128,
    /// Prize distribution vector; each value maps to winner allocation units.
    pub prizes: Vec<u32>,
    /// Randomness source strategy selected for the raffle.
    pub randomness_source: RandomnessSource,
    /// Optional oracle contract address for external randomness flows.
    pub oracle_address: Option<Address>,
    /// Protocol fee in basis points (100 = 1%). Charged on ticket purchase only.
    pub protocol_fee_bp: u32,
    /// Optional treasury recipient address for protocol fees.
    pub treasury_address: Option<Address>,
    /// Optional router contract used when swap-based flows are enabled.
    pub swap_router: Option<Address>,
    /// Optional protocol token used in incentive/swap features.
    pub tikka_token: Option<Address>,
    /// SHA-256 hash of immutable off-chain metadata content.
    pub metadata_hash: BytesN<32>,
    /// Seconds after finalization before winners may claim.
    /// Must be in [0, 604800] (0 to 7 days). Defaults to 3600 if zero.
    pub claim_lockup_seconds: u64,
    /// Swap deadline window in seconds (added to current timestamp for token swaps).
    /// Defaults to 300 (5 minutes) if zero. Configurable to handle network congestion.
    pub swap_deadline_seconds: u64,
}

impl RaffleConfig {
    pub fn resolve_defaults(mut self) -> Self {
        if self.claim_lockup_seconds == 0 {
            self.claim_lockup_seconds = DEFAULT_CLAIM_LOCKUP_SECONDS;
        }
        if self.swap_deadline_seconds == 0 {
            self.swap_deadline_seconds = DEFAULT_SWAP_DEADLINE_SECONDS;
        }
        self
    }
}

#[derive(Clone)]
#[contracttype]
pub struct Ticket {
    /// Monotonic ticket identifier scoped to a raffle.
    pub id: u32,
    /// Address that owns this ticket.
    pub owner: Address,
    /// Unix timestamp when the ticket was purchased.
    pub purchase_time: u64,
    /// Human-facing ticket number used in draw/result UX.
    pub ticket_number: u32,
}

/// Audit data proving how a draw outcome was derived.
#[derive(Clone)]
#[contracttype]
pub struct FairnessData {
    /// Seed value used to derive final winner indices.
    pub seed: u64,
    /// Source used to generate the randomness seed.
    pub randomness_source: RandomnessSource,
    /// Ordered ticket identifiers considered in the draw.
    pub ticket_ids: Vec<u32>,
    /// Computed winning indices into `ticket_ids`.
    pub winning_ticket_indices: Vec<u32>,
    /// Unix timestamp when draw resolution occurred.
    pub draw_timestamp: u64,
    /// Sequence counter for draws/re-draws within the raffle.
    pub draw_sequence: u32,
}

/// Generic pagination request for list queries.
#[derive(Clone)]
#[contracttype]
pub struct PaginationParams {
    /// Maximum number of items requested by caller.
    pub limit: u32,
    /// Number of items to skip from the beginning of result set.
    pub offset: u32,
}

/// Paginated raffle address query result.
#[derive(Clone)]
#[contracttype]
pub struct PageResultRaffles {
    /// Returned raffle addresses for the current page.
    pub items: Vec<Address>,
    /// Total number of raffles matching the query.
    pub total: u32,
    /// True when more records are available after this page.
    pub has_more: bool,
}

/// Paginated ticket query result.
#[derive(Clone)]
#[contracttype]
pub struct PageResultTickets {
    /// Returned tickets for the current page.
    pub items: Vec<Ticket>,
    /// Total number of tickets matching the query.
    pub total: u32,
    /// True when more records are available after this page.
    pub has_more: bool,
}

/// Administrative operations that can be timelocked or proposed.
#[derive(Clone)]
#[contracttype]
pub enum AdminOp {
    /// Update protocol configuration entry `u32` with a new address value.
    SetConfig(u32, Address),
    /// Rotate target contract WASM hash for upgrades.
    UpdateWasmHash(BytesN<32>),
}

/// Default page size when callers request zero items.
pub const DEFAULT_PAGE_LIMIT: u32 = 100;
/// Hard maximum page size accepted by query helpers.
pub const MAX_PAGE_LIMIT: u32 = 200;
pub const DEFAULT_CLAIM_LOCKUP_SECONDS: u64 = 3_600;
pub const DEFAULT_SWAP_DEADLINE_SECONDS: u64 = 300;

/// Returns a safe pagination limit clamped to supported bounds.
pub fn effective_limit(requested: u32) -> u32 {
    if requested == 0 {
        DEFAULT_PAGE_LIMIT
    } else if requested > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT
    } else {
        requested
    }
}

/// Oracle randomness request payload sent to an oracle contract.
#[derive(Clone)]
#[contracttype]
pub struct RandomnessRequest {
    /// Target raffle contract identifier.
    pub raffle_id: Address,
    /// Unique request id used to correlate callback responses.
    pub request_id: u64,
    /// Callback contract address expected to receive randomness.
    pub callback_address: Address,
}

/// Client trait for randomness oracle contracts.
#[soroban_sdk::contractclient(name = "RandomnessOracleClient")]
pub trait RandomnessOracleTrait {
    /// Requests randomness from the oracle for a raffle draw.
    fn request_randomness(env: soroban_sdk::Env, request: RandomnessRequest);
}

/// Client trait implemented by contracts that receive oracle callbacks.
#[soroban_sdk::contractclient(name = "RandomnessReceiverClient")]
pub trait RandomnessReceiverTrait {
    /// Delivers a randomness response to the callback contract.
    fn receive_randomness(env: soroban_sdk::Env, request_id: u64, random_seed: u64);
}
