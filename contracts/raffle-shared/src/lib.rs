#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod constants;
pub mod events;

#[cfg(test)]
mod nft_mint_test;
use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

/// Lifecycle state of a raffle instance.
///
/// Transitions are enforced by contract logic and represent the canonical
/// on-chain lifecycle used by indexers and clients.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

impl RaffleStatus {
    /// Returns true when no further lifecycle transitions are permitted.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RaffleStatus::Cancelled | RaffleStatus::Failed | RaffleStatus::Claimed
        )
    }

    /// Legal on-chain transitions from `self` to `target`.
    ///
    /// Internal rollbacks (e.g. `Drawing → Active` after a failed randomness
    /// request) are not part of the canonical graph and must use
    /// [`RaffleStatus::can_internal_revert_to`] instead.
    pub fn can_transition_to(self, target: RaffleStatus) -> bool {
        if self == target {
            return false;
        }
        self.allowed_targets().contains(&target)
    }

    /// Recovery transitions used only when rolling back a partially-applied draw.
    pub fn can_internal_revert_to(self, target: RaffleStatus) -> bool {
        self == RaffleStatus::Drawing && target == RaffleStatus::Active
    }

    fn allowed_targets(self) -> &'static [RaffleStatus] {
        match self {
            RaffleStatus::PendingPrize => &[RaffleStatus::Active],
            RaffleStatus::Active => &[
                RaffleStatus::Drawing,
                RaffleStatus::Failed,
                RaffleStatus::Cancelled,
            ],
            RaffleStatus::Drawing => &[RaffleStatus::Finalized, RaffleStatus::Cancelled],
            RaffleStatus::Finalized => &[RaffleStatus::Claimed],
            RaffleStatus::Cancelled | RaffleStatus::Failed | RaffleStatus::Claimed => &[],
        }
    }

    /// Every variant for exhaustive matrix tests.
    pub fn all() -> &'static [RaffleStatus] {
        &[
            RaffleStatus::PendingPrize,
            RaffleStatus::Active,
            RaffleStatus::Drawing,
            RaffleStatus::Finalized,
            RaffleStatus::Cancelled,
            RaffleStatus::Failed,
            RaffleStatus::Claimed,
        ]
    }
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

/// Configuration for the [`RandomnessSource::Quorum`] randomness mode.
#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub struct QuorumConfig {
    /// Number of oracle submissions required to reach quorum.
    pub k: u32,
    /// Ordered list of registered oracle addresses.
    pub oracles: Vec<soroban_sdk::Address>,
}

/// Source used to generate randomness for winner selection.
#[derive(Clone, PartialEq, Eq, Debug)]
#[contracttype]
pub enum RandomnessSource {
    /// Internal pseudo-randomness generated on-chain.
    Internal,
    /// External oracle-provided randomness (single oracle).
    External,
    /// Commit-reveal based randomness source.
    CommitReveal,
    /// K-of-N quorum of oracles: aggregate seeds from at least `k` of `n`
    /// registered oracles before finalizing.  Eliminates single-oracle trust.
    Quorum(QuorumConfig),
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

/// Configuration for a recurring (subscription) raffle.
///
/// Enables automatic creation of new raffle instances at a fixed interval
/// without manual re-deployment.  Designed for weekly / monthly raffles.
#[derive(Clone)]
#[contracttype]
pub struct RecurringRaffleConfig {
    /// The base raffle configuration reused for every round.
    pub base_config: RaffleConfig,
    /// Seconds between successive raffle rounds (e.g. 604800 for weekly).
    pub interval_seconds: u64,
    /// Maximum number of rounds (0 = infinite).
    pub max_rounds: u32,
    /// If true, the creator must pre-authorise the prize funds (not yet
    /// implemented — reserved for future use).
    pub auto_fund: bool,
}

/// Configuration payload used when creating a new raffle.
///
/// Values are validated by contract initialization before the raffle becomes
/// active and represent the complete raffle policy surface.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct RaffleConfig {
    /// Human-readable raffle description.
    pub description: String,
    /// Unix timestamp when ticket sales close (ignored when `no_deadline` is
    /// true). Exclusive boundary: sales are open while
    /// `ledger_timestamp < end_time`; the deadline is reached starting at
    /// `ledger_timestamp == end_time`. Validated at `init` to be strictly in
    /// the future. See `docs/GLOSSARY.md` § "End Time".
    pub end_time: u64,
    /// If true, raffle can remain open without a hard end timestamp.
    pub no_deadline: bool,
    /// Maximum number of tickets that can ever be sold.
    pub max_tickets: u32,
    /// Maximum tickets a single address may purchase per transaction.
    /// Zero is invalid and rejected during initialization with `Error::InvalidParameters` (must be in 1..=max_tickets).
    pub max_tickets_per_tx: u32,
    /// Reserved per-address ticket cap (0 = unlimited). Enforcement and
    /// initialization validation are not implemented yet.
    pub max_tickets_per_address: u32,
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
    /// Protocol fee in basis points (100 = 1%). Currently charged at ticket
    /// purchase only. See docs/FEE_MODEL.md for the implemented fee model.
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
    /// Must be in [0, 604800] (0 to 7 days). Defaults to 3600 (1 hour) if not provided (None).
    pub claim_lockup_seconds: Option<u64>,
    /// Seconds after finalization before unclaimed prizes may be swept to the
    /// treasury.  Must be at least [`MIN_CLAIM_EXPIRY_SECONDS`] and strictly
    /// greater than `claim_lockup_seconds`.  Defaults to 30 days if not set.
    pub claim_expiry_seconds: Option<u64>,
    /// Swap deadline window in seconds (added to current timestamp for token swaps).
    /// Defaults to 300 (5 minutes) if not provided (None). Configurable to handle network congestion.
    pub swap_deadline_seconds: Option<u64>,
    /// The percentage of max_tickets covered by the early bird discount (0 to disable).
    pub early_bird_ticket_percentage: u32,
    /// The discount amount specified in basis points.
    pub early_bird_discount_bp: u32,
    /// Optional on-chain category/tag used for frontend filtering (e.g. "gaming",
    /// "charity", "art"). When set it must be at most `MAX_CATEGORY_LENGTH` bytes
    /// and contain only ASCII alphanumerics and hyphens. Validated in the raffle
    /// instance's `init`; the factory maintains a per-category index so clients
    /// can query raffles by category without an off-chain indexer. See #439.
    pub category: Option<String>,
    /// When true, each address may win at most one prize tier (#485).
    pub unique_winners: bool,
    /// Optional tiered bundle pricing for ticket purchases.
    pub bundles: Vec<TicketBundle>,
    /// Optional prize token override. The raffle-instance initializer does not
    /// currently apply this field and always uses `payment_token`.
    pub prize_token: Option<Address>,
    /// Optional NFT contract for ticket receipts.
    pub nft_contract: Option<Address>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct BuyQuote {
    pub gross: i128,
    pub discount: i128,
    pub fee: i128,
    pub net_to_pay: i128,
    pub effective_ticket_price: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct RaffleStats {
    pub tickets_sold: u32,
    pub unique_buyers: u32,
    pub gross_revenue: i128,
    pub fees_accrued: i128,
    pub prize_funded: bool,
    pub status: RaffleStatus,
    pub time_remaining: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct TicketBundle {
    pub quantity: u32,
    pub price_per_ticket: i128,
}

impl RaffleConfig {
    pub fn resolve_defaults(mut self) -> Self {
        if self.claim_lockup_seconds.is_none() {
            self.claim_lockup_seconds = Some(DEFAULT_CLAIM_LOCKUP_SECONDS);
        }
        if self.swap_deadline_seconds.is_none() {
            self.swap_deadline_seconds = Some(DEFAULT_SWAP_DEADLINE_SECONDS);
        }
        if self.claim_expiry_seconds.is_none() {
            self.claim_expiry_seconds = Some(DEFAULT_CLAIM_EXPIRY_SECONDS);
        }
        self
    }
}

/// Ticket record stored for each purchased raffle ticket.
///
/// `id` is the monotonic, storage-scoped identifier used for lookups and draw
/// indexing. `ticket_number` is the human-facing number exposed in UX and
/// refund events. In the current contract implementation, every ticket is
/// created with `ticket_number == id`, and that relationship is pinned by tests.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct Ticket {
    /// Monotonic ticket identifier scoped to a raffle.
    pub id: u32,
    /// Address that owns this ticket.
    pub owner: Address,
    /// Unix timestamp when the ticket was purchased.
    pub purchase_time: u64,
    /// Human-facing ticket number used in draw/result UX.
    /// It is kept equal to `id` for the current contract implementation.
    pub ticket_number: u32,
    /// The address that paid for this ticket.
    pub payer: Address,
    /// Price actually paid for this ticket (in token base units).
    /// Records the effective price including any early-bird discount.
    pub price_paid: i128,
}

impl Ticket {
    /// Create a ticket with the canonical invariant that the human-facing ticket
    /// number matches the monotonic storage id.
    pub fn new(id: u32, owner: Address, purchase_time: u64, price_paid: i128) -> Self {
        Self {
            id,
            owner: owner.clone(),
            purchase_time,
            ticket_number: id,
            payer: owner,
            price_paid,
        }
    }
}

/// A single drawn winner and their claim state.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct Winner {
    /// Address that owns the winning ticket at draw time.
    pub address: Address,
    /// True once this tier's prize has been paid out or swept.
    pub claimed: bool,
    /// Index into `Raffle::prizes` identifying the tier won.
    pub tier_index: u32,
}

/// Audit data proving how a draw outcome was derived.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Ledger sequence number of the block in which the draw was finalized.
    /// Recorded as `env.ledger().sequence()` at finalization so auditors can
    /// cross-reference the draw with the canonical on-chain ledger.
    pub draw_sequence: u32,
    /// Whether unique-address winner fairness was enabled for this draw (#485).
    pub unique_winners: bool,
    /// The quorum oracles that contributed and their submitted seeds, if applicable.
    pub quorum_contributions: Option<Vec<(Address, u64)>>,
}

/// Generic pagination request for list queries.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct PaginationParams {
    /// Maximum number of items requested by caller.
    pub limit: u32,
    /// Number of items to skip from the beginning of result set.
    pub offset: u32,
}

/// Paginated raffle address query result.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct PageResultTickets {
    /// Returned tickets for the current page.
    pub items: Vec<Ticket>,
    /// Total number of tickets matching the query.
    pub total: u32,
    /// True when more records are available after this page.
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ConfigKey {
    Treasury,
    Oracle,
    SwapRouter,
}

/// Administrative operations that can be timelocked or proposed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub enum AdminOp {
    /// Update a protocol configuration address.
    SetConfig(ConfigKey, Address),
    /// Update the protocol fee basis points.
    SetProtocolFeeBP(u32),
    /// Rotate target contract WASM hash for upgrades.
    UpdateWasmHash(BytesN<32>),
    /// Add an oracle address to the factory's approved-oracle allowlist.
    ApproveOracle(Address),
    /// Remove an oracle address from the factory's approved-oracle allowlist.
    RemoveOracle(Address),
}

/// Pricing breakdown for a prospective ticket purchase.
///
/// Returned by `calculate_buy_quote` and surfaced via `preview_buy`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct BuyQuote {
    /// Gross total before discount: `ticket_price × quantity`.
    pub gross: i128,
    /// Total early-bird discount applied.
    pub discount: i128,
    /// Protocol fee computed on the discounted total.
    pub fee: i128,
    /// Amount the buyer actually pays: `gross - discount`.
    pub net_to_pay: i128,
    /// Per-ticket price after discount: `net_to_pay / quantity`.
    pub effective_ticket_price: i128,
}

// Re-export constants from the single source of truth
pub use constants::{
    DEFAULT_CLAIM_EXPIRY_SECONDS, DEFAULT_CLAIM_LOCKUP_SECONDS, DEFAULT_PAGE_LIMIT,
    DEFAULT_SWAP_DEADLINE_SECONDS, MAX_PAGE_LIMIT, MAX_SWEEP_UNCLAIMED_PER_CALL,
    MIN_CLAIM_EXPIRY_SECONDS,
};

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
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// Cross-contract interface for an NFT ticket contract.
///
/// The raffle-instance calls `mint` on this contract immediately after a
/// successful ticket purchase.  The NFT contract is responsible for its own
/// authorisation model; the raffle-instance supplies the raffle's own address
/// as the `minter` so the NFT contract can restrict minting to known raffle
/// contracts.
///
/// Parameters
/// ----------
/// * `recipient`  – the address that receives the NFT (the ticket buyer).
/// * `ticket_id`  – the unique ticket ID within this raffle (1-indexed, u32).
/// * `raffle_id`  – the raffle instance contract address, used as a namespace
///   so a single NFT contract can serve multiple raffles.
///
/// Failure semantics
/// ------------------
/// Callers are expected to invoke this hook via the generated
/// `NftTicketClient::mint` (not `try_mint`). A panic in the NFT contract's
/// `mint` therefore propagates and aborts the calling transaction — the
/// ticket purchase and the NFT mint succeed or roll back atomically. This is
/// a deliberate choice: it keeps ticket state and NFT ownership from ever
/// diverging, at the cost of ticket sales being unavailable if the
/// configured NFT contract is broken. A caller that instead wants purchases
/// to survive a broken NFT contract must explicitly use `try_mint` and
/// handle the `Result`; see the failure-path tests in `nft_mint_test` for
/// both behaviors pinned side by side.
#[soroban_sdk::contractclient(name = "NftTicketClient")]
pub trait NftTicketTrait {
    fn mint(env: soroban_sdk::Env, recipient: Address, ticket_id: u32, raffle_id: Address);
}

/// Generate a `require_admin` helper that loads `DataKey::Admin` from persistent
/// storage at the call site and enforces `require_auth`.
#[macro_export]
macro_rules! impl_require_admin {
    ($error_type:ty, $not_authorized:expr) => {
        fn require_admin(env: &soroban_sdk::Env) -> Result<soroban_sdk::Address, $error_type> {
            let admin: soroban_sdk::Address = env
                .storage()
                .persistent()
                .get(&DataKey::Admin)
                .ok_or($not_authorized)?;
            admin.require_auth();
            Ok(admin)
        }
    };
}

/// Generate a not-paused guard that reads instance `DataKey::Paused`.
#[macro_export]
macro_rules! impl_require_not_paused {
    ($error_type:ty, $contract_paused:expr, $fn_name:ident) => {
        fn $fn_name(env: &soroban_sdk::Env) -> Result<(), $error_type> {
            if env
                .storage()
                .instance()
                .get(&DataKey::Paused)
                .unwrap_or(false)
            {
                return Err($contract_paused);
            }
            Ok(())
        }
    };
}

/// Unit tests for RaffleConfig defaults and resolution (#734).
#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{Env, String, Address, BytesN, Vec};
    /// Helper to construct a canonical test configuration with explicit documented defaults.
    fn default_config(env: &Env) -> RaffleConfig {
        let payment_token = Address::from_string(&String::from_str(
            env,
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
        ));
        RaffleConfig {
            description: String::from_str(env, "Test"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 10,
            max_tickets_per_tx: 10,
            // 0 = unlimited per-address ticket purchases in default test setup
            max_tickets_per_address: 0,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: 10_000,
            payment_token,
            prize_amount: 10_000,
            prizes: Vec::new(env),
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(env, &[0; 32]),
            claim_lockup_seconds: None,
            claim_expiry_seconds: None,
            swap_deadline_seconds: None,
            early_bird_ticket_percentage: 0,
            early_bird_discount_bp: 0,
            category: None,
            // Default to false so an address can win multiple tiers unless unique winner mode is enabled
            unique_winners: false,
            // Default to an empty vector; bundle pricing is optional and disabled by default
            bundles: Vec::new(env),
            // Default to None; prize payout defaults to payment_token when prize_token is not overridden
            prize_token: None,
            // Default to None; receipt NFT minting is opt-in and disabled by default
            nft_contract: None,
        }
    }

    #[test]
    fn test_default_config_has_expected_defaults() {
        let env = Env::default();
        let config = default_config(&env);

        assert_eq!(config.unique_winners, false);
        assert!(config.bundles.is_empty());
        assert_eq!(config.prize_token, None);
        assert_eq!(config.nft_contract, None);
        assert_eq!(config.max_tickets_per_address, 0);
    }

    #[test]
    fn test_resolve_defaults_none_inputs() {
        let env = Env::default();
        let mut config = default_config(&env);
        config.claim_lockup_seconds = None;
        config.swap_deadline_seconds = None;

        let resolved = config.resolve_defaults();
        assert_eq!(resolved.claim_lockup_seconds, Some(DEFAULT_CLAIM_LOCKUP_SECONDS));
        assert_eq!(resolved.swap_deadline_seconds, Some(DEFAULT_SWAP_DEADLINE_SECONDS));
    }

    #[test]
    fn test_resolve_defaults_zero_inputs() {
        let env = Env::default();
        let mut config = default_config(&env);
        config.claim_lockup_seconds = Some(0);
        config.swap_deadline_seconds = Some(0);

        let resolved = config.resolve_defaults();
        assert_eq!(resolved.claim_lockup_seconds, Some(0));
        assert_eq!(resolved.swap_deadline_seconds, Some(0));
    }

    #[test]
    fn test_resolve_defaults_nonzero_inputs() {
        let env = Env::default();
        let mut config = default_config(&env);
        config.claim_lockup_seconds = Some(42);
        config.swap_deadline_seconds = Some(99);

        let resolved = config.resolve_defaults();
        assert_eq!(resolved.claim_lockup_seconds, Some(42));
        assert_eq!(resolved.swap_deadline_seconds, Some(99));
    }

    #[test]
    fn test_resolve_defaults_independent_fields() {
        let env = Env::default();

        // Lockup is Some(0), swap is None
        let mut config1 = default_config(&env);
        config1.claim_lockup_seconds = Some(0);
        config1.swap_deadline_seconds = None;
        let resolved1 = config1.resolve_defaults();
        assert_eq!(resolved1.claim_lockup_seconds, Some(0));
        assert_eq!(resolved1.swap_deadline_seconds, Some(DEFAULT_SWAP_DEADLINE_SECONDS));

        // Lockup is None, swap is Some(0)
        let mut config2 = default_config(&env);
        config2.claim_lockup_seconds = None;
        config2.swap_deadline_seconds = Some(0);
        let resolved2 = config2.resolve_defaults();
        assert_eq!(resolved2.claim_lockup_seconds, Some(DEFAULT_CLAIM_LOCKUP_SECONDS));
        assert_eq!(resolved2.swap_deadline_seconds, Some(0));
    }
}

#[cfg(test)]
mod effective_limit_tests {
    use super::{effective_limit, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};

    #[test]
    fn zero_requests_default_page_limit() {
        assert_eq!(effective_limit(0), DEFAULT_PAGE_LIMIT);
    }

    #[test]
    fn one_is_unchanged() {
        assert_eq!(effective_limit(1), 1);
    }

    #[test]
    fn ninety_nine_is_unchanged() {
        assert_eq!(effective_limit(99), 99);
    }

    #[test]
    fn default_page_limit_is_unchanged() {
        assert_eq!(effective_limit(DEFAULT_PAGE_LIMIT), DEFAULT_PAGE_LIMIT);
    }

    #[test]
    fn one_below_max_is_unchanged() {
        assert_eq!(effective_limit(MAX_PAGE_LIMIT - 1), MAX_PAGE_LIMIT - 1);
    }

    #[test]
    fn max_page_limit_is_unchanged() {
        assert_eq!(effective_limit(MAX_PAGE_LIMIT), MAX_PAGE_LIMIT);
    }

    #[test]
    fn one_above_max_clamps_to_max() {
        assert_eq!(effective_limit(MAX_PAGE_LIMIT + 1), MAX_PAGE_LIMIT);
    }

    #[test]
    fn u32_max_clamps_to_max() {
        assert_eq!(effective_limit(u32::MAX), MAX_PAGE_LIMIT);
    }
}

#[cfg(test)]
mod raffle_status_tests {
    use super::RaffleStatus;

    #[test]
    fn terminal_states_have_no_outgoing_transitions() {
        for status in [RaffleStatus::Cancelled, RaffleStatus::Failed, RaffleStatus::Claimed] {
            assert!(status.is_terminal());
            for target in RaffleStatus::all() {
                if *target != status {
                    assert!(!status.can_transition_to(*target));
                }
            }
        }
    }

    #[test]
    fn pending_prize_only_moves_to_active() {
        assert!(RaffleStatus::PendingPrize.can_transition_to(RaffleStatus::Active));
        assert!(!RaffleStatus::PendingPrize.can_transition_to(RaffleStatus::Drawing));
    }
}
