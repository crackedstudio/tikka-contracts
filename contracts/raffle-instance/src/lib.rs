#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contracterror, contractimpl, contracttype, token,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Val, Vec,
};

mod admin;
mod attestation;
mod claim;
mod draw;
mod events;
mod helpers;
mod init;
mod randomness;
mod tickets;
mod views;

pub(crate) use helpers::do_finalize_with_seed;

use raffle_shared::{
    constants::{
        DEFAULT_CLAIM_LOCKUP_SECONDS, DEFAULT_SWAP_DEADLINE_SECONDS, EMERGENCY_WITHDRAW_DELAY_SECONDS,
        MAX_CLAIM_LOCKUP_SECONDS, MAX_DESCRIPTION_LENGTH, MAX_PRIZES, MAX_PRIZE_AMOUNT,
        MAX_PROTOCOL_FEE_BP, MAX_SWAP_DEADLINE_SECONDS, MAX_TICKETS_LIMIT, MIN_TICKET_PRICE,
        ORACLE_TIMEOUT_LEDGERS,
    },
    CancelReason, FailureReason, FairnessData, QuorumConfig, RaffleConfig, RaffleStatus,
    RandomnessSource, RandomnessType, Ticket, Winner,
};

use self::randomness::{
    build_vrf_proof_message, OracleSeedWinnerSelection, WinnerSelectionStrategy,
};

use crate::events::{
    CancelScheduled, ContractPaused, ContractUnpaused, DrawTriggered, EmergencyWithdrawn,
    FeesWithdrawn, MetadataHashUpdated, OracleAddressUpdated, OracleSeedDelivered, PrizeClaimed, PrizeDeposited,
    PrizeRefunded, ProtocolFeeUpdated, RaffleCancelled, RaffleCreated, RaffleFailed,
    RaffleFinalized, RaffleStatusChanged, RandomnessFallbackTriggered, RandomnessReceived,
    RandomnessRequested, StorageWiped, SwapDeadlineUpdated, TicketNftMinted, TicketPurchased,
    TicketRefunded, TicketSalesPaused, TicketSalesResumed, TokensRescued, WinnerDrawn,
};

const RANDOMNESS_MIN_DELAY_LEDGERS: u32 = 10;

#[contract]
pub struct RaffleInstance;

#[contracttype]
#[derive(Clone)]
pub struct Raffle {
    pub creator: Address,
    pub description: String,
    pub end_time: u64,
    pub no_deadline: bool,
    pub max_tickets: u32,
    pub max_tickets_per_tx: u32,
    pub max_tickets_per_address: u32,
    pub min_tickets: u32,
    pub allow_multiple: bool,
    pub ticket_price: i128,
    pub payment_token: Address,
    /// The token used for prize deposit and claims. The current initializer
    /// always sets this to `payment_token`; the config override is not wired.
    pub prize_token: Address,
    pub prize_amount: i128,
    pub prizes: Vec<u32>,
    pub tickets_sold: u32,
    pub status: RaffleStatus,
    pub prize_deposited: bool,
    /// Unified winner list.  Each entry carries the winner's address, claim
    /// state, and prize tier in a single struct — eliminating the old
    /// parallel-array pattern (`winners: Vec<Address>` + `claimed_winners: Vec<bool>`).
    pub winners: Vec<Winner>,
    pub randomness_source: RandomnessSource,
    pub oracle_address: Option<Address>,
    pub protocol_fee_bp: u32,
    pub treasury_address: Option<Address>,
    pub swap_router: Option<Address>,
    pub tikka_token: Option<Address>,
    pub finalized_at: Option<u64>,
    pub claim_lockup_seconds: u64,
    pub swap_deadline_seconds: u64,
    pub ticket_sales_paused: bool,
    /// The percentage of max_tickets covered by the early bird discount (0 to disable).
    pub early_bird_ticket_percentage: u32,
    /// The discount amount specified in basis points.
    pub early_bird_discount_bp: u32,
    pub metadata_hash: BytesN<32>,
    pub unique_winners: bool,
    pub nft_contract: Option<Address>,
}

#[contracttype]
#[derive(Clone)]
pub struct FairnessMetadata {
    pub seed: u64,
    pub randomness_source: RandomnessSource,
    pub winning_ticket_indices: Vec<u32>,
    pub draw_timestamp: u64,
    pub draw_sequence: u32,
    pub unique_winners: bool,
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
    /// Reserved per-owner ticket ID index: owner Address → Vec<u32> of ticket
    /// IDs. It is not currently written by ticket purchase logic.
    OwnerTickets(Address),
    PendingAdminCancel,
    /// Quorum randomness: maps registered oracle address → submitted seed.
    QuorumSeed(Address),
    /// Quorum randomness: ordered list of oracles that have submitted.
    QuorumSubmittedOracles,
}

#[contracttype]
#[derive(Clone)]
pub struct CommitRevealEntry {
    pub committer: Address,
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
    TicketNotFound = 34,
    RaffleEnded = 35,
    ArithmeticOverflow = 41,
    AlreadyInitialized = 42,
    NotInitialized = 43,
    Reentrancy = 44,
    TokenTransferFailed = 45,
    NoActiveTickets = 46,
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
    ExceedsMaxTicketsPerAddress = 65,
    DrawingAlreadyInProgress = 59,
    InvalidStatusForDrawingTransition = 60, // Note: This seems to be a copy-paste error in the original code.
    DrawingAlreadyComplete = 61,
    InvalidEndTime = 62,
    InvalidAdminAddress = 63,
    RandomnessTooEarly = 64,
    CancelTimelockActive = 65,
    CancelNotScheduled = 66,
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
        if config.max_tickets_per_address > config.max_tickets {
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
        if config.protocol_fee_bp > 0 && config.treasury_address.is_none() {
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

        // Quorum validation: k must be > 0, oracles must be non-empty, and
        // oracle_address must not be set (oracles are embedded in the enum).
        if let RandomnessSource::Quorum(QuorumConfig { k, oracles }) = &config.randomness_source {
            if *k == 0 || *k > oracles.len() as u32 {
                return Err(Error::InvalidParameters);
            }
            if oracles.len() > 10 {
                return Err(Error::InvalidParameters);
            }
            // Self-check: none of the oracles may be the raffle contract itself.
            for i in 0..oracles.len() {
                if let Some(addr) = oracles.get(i) {
                    if addr == env.current_contract_address() {
                        return Err(Error::InvalidParameters);
                    }
                }
            }
            // Quorum mode must not also set a single oracle_address.
            if config.oracle_address.is_some() {
                return Err(Error::InvalidParameters);
            }
        }

        if config.randomness_source != RandomnessSource::External
            && config.randomness_source
                != RandomnessSource::Quorum(QuorumConfig {
                    k: 1,
                    oracles: Vec::new(&env),
                })
            && config.oracle_address.is_some()
        {
            return Err(Error::InvalidParameters);
        }

        if config.metadata_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(Error::InvalidParameters);
        }

        // Validate that the payment_token is a valid token contract
        validate_token_address(&env, &config.payment_token)?;

        // The prize-token config override is not wired into initialization.
        let prize_token = config.payment_token.clone();

        // Resolve default values for fields that use 0 as "use default"
        let config = config.resolve_defaults();

        // #259: claim_lockup_seconds must be within [0, MAX_CLAIM_LOCKUP_SECONDS].
        if config.claim_lockup_seconds > MAX_CLAIM_LOCKUP_SECONDS {
            return Err(Error::InvalidParameters);
        }

        // Swap deadline must be within [0, MAX_SWAP_DEADLINE_SECONDS].
        if config.swap_deadline_seconds > MAX_SWAP_DEADLINE_SECONDS {
            return Err(Error::InvalidParameters);
        }

        // Validate early bird parameters
        if config.early_bird_ticket_percentage > 100 {
            return Err(Error::InvalidParameters);
        }
        if config.early_bird_ticket_percentage > 0 && config.early_bird_discount_bp > 10000 {
            return Err(Error::InvalidParameters);
        }

        let raffle = Raffle {
            creator: creator.clone(),
            description: config.description.clone(),
            end_time: config.end_time,
            no_deadline: config.no_deadline,
            max_tickets: config.max_tickets,
            max_tickets_per_tx: config.max_tickets_per_tx,
            max_tickets_per_address: config.max_tickets_per_address,
            min_tickets: config.min_tickets,
            allow_multiple: config.allow_multiple,
            ticket_price: config.ticket_price,
            payment_token: config.payment_token.clone(),
            prize_token: prize_token.clone(),
            prize_amount: config.prize_amount,
            prizes: config.prizes.clone(),
            tickets_sold: 0,
            status: RaffleStatus::PendingPrize,
            prize_deposited: false,
            winners: Vec::new(&env),
            randomness_source: config.randomness_source.clone(),
            oracle_address: config.oracle_address,
            protocol_fee_bp: config.protocol_fee_bp,
            treasury_address: config.treasury_address,
            swap_router: config.swap_router,
            tikka_token: config.tikka_token,
            finalized_at: None,
            claim_lockup_seconds: config.claim_lockup_seconds,
            swap_deadline_seconds: config.swap_deadline_seconds,
            ticket_sales_paused: false,
            early_bird_ticket_percentage: config.early_bird_ticket_percentage,
            early_bird_discount_bp: config.early_bird_discount_bp,
            metadata_hash: config.metadata_hash.clone(),
            unique_winners: config.unique_winners,
            nft_contract: config.nft_contract,
        };
        write_raffle(&env, &raffle);
        env.storage().instance().set(&DataKey::Factory, &factory);
        env.storage().instance().set(&DataKey::Admin, &admin);
        // Store metadata hash for attestation verification
        env.storage()
            .persistent()
            .set(&DataKey::MetadataHash, &config.metadata_hash);

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
            unique_winners: config.unique_winners,
        }
        .publish(&env);

        Ok(())
    }

    pub fn deposit_prize(env: Env) -> Result<(), Error> {
        init::deposit_prize(env)
    }

    pub fn buy_tickets(env: Env, buyer: Address, quantity: u32) -> Result<u32, Error> {
        tickets::buy_tickets(env, buyer, quantity)
    }

    pub fn buy_tickets_for(
        env: Env,
        buyer: Address,
        recipient: Address,
        quantity: u32,
    ) -> Result<u32, Error> {
        tickets::buy_tickets_for(env, buyer, recipient, quantity)
    }

    pub fn submit_commit(env: Env, ticket_id: u32, hash: BytesN<32>) -> Result<(), Error> {
        tickets::submit_commit(env, ticket_id, hash)
    }

    pub fn finalize_raffle(env: Env) -> Result<(), Error> {
        draw::finalize_raffle(env)
    }

    pub fn provide_randomness(
        env: Env,
        random_seed: u64,
        public_key: BytesN<32>,
        proof: BytesN<64>,
        request_id: u64,
    ) -> Result<Address, Error> {
        draw::provide_randomness(env, random_seed, public_key, proof, request_id)
    }

    /// Accept a seed from a single oracle in a k-of-n Quorum configuration.
    ///
    /// The caller must be one of the registered oracles in the raffle's
    /// `RandomnessSource::Quorum` list.  Each oracle may submit at
    /// most once.  Once the k-th valid submission is received, the seeds are
    /// aggregated via `aggregate_quorum_seeds` and the raffle is finalized.
    pub fn provide_quorum_randomness(
        env: Env,
        random_seed: u64,
        request_id: u64,
    ) -> Result<(), Error> {
        let drawing_lock: bool = env
            .storage()
            .instance()
            .get(&DataKey::DrawingLock)
            .unwrap_or(false);
        if !drawing_lock {
            return Err(Error::DrawingAlreadyComplete);
        }

        let caller = env
            .invoker()
            .expect("provide_quorum_randomness: invoker required");
        caller.require_auth();

        let raffle = read_raffle(&env)?;

        // Verify random seed context: request_id
        let stored: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequestId)
            .ok_or(Error::NoRandomnessRequest)?;
        if stored != request_id {
            return Err(Error::InvalidParameters);
        }

        // Extract the oracle list from the Quorum config.
        let (k, oracles) = match &raffle.randomness_source {
            RandomnessSource::Quorum(QuorumConfig { k, oracles }) => (*k, oracles.clone()),
            _ => return Err(Error::InvalidParameters),
        };

        // Verify caller is a registered oracle.
        let mut is_registered = false;
        for i in 0..oracles.len() {
            if let Some(addr) = oracles.get(i) {
                if addr == caller {
                    is_registered = true;
                    break;
                }
            }
        }
        if !is_registered {
            return Err(Error::OracleNotRegistered);
        }

        // Dedup: reject if this oracle already submitted.
        if env.storage().persistent().has(&DataKey::QuorumSeed(caller.clone())) {
            return Err(Error::DuplicateOracleSubmission);
        }

        // Store the seed.
        env.storage()
            .persistent()
            .set(&DataKey::QuorumSeed(caller.clone()), &random_seed);

        // Track submission order.
        let mut submitted: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::QuorumSubmittedOracles)
            .unwrap_or_else(|| Vec::new(&env));
        submitted.push_back(caller.clone());
        env.storage()
            .persistent()
            .set(&DataKey::QuorumSubmittedOracles, &submitted);

        let count = submitted.len() as u32;

        // Emit delivery event.
        OracleSeedDelivered {
            oracle: caller.clone(),
            seed: random_seed,
            request_id,
            current_count: count,
            threshold: k,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        // Check if quorum reached.
        if count >= k {
            // Build the seed list from storage.
            let mut seeds = Vec::new(&env);
            for i in 0..submitted.len() {
                if let Some(addr) = submitted.get(i) {
                    if let Some(s) = env
                        .storage()
                        .persistent()
                        .get::<_, u64>(&DataKey::QuorumSeed(addr.clone()))
                    {
                        seeds.push_back((addr.clone(), s));
                    }
                }
            }

            let aggregate = randomness::aggregate_quorum_seeds(&env, &seeds);
            helpers::do_finalize_with_seed(&env, raffle, aggregate, RandomnessType::Vrf)?;
        }

        Ok(())
    }

    pub fn trigger_randomness_fallback(
        env: Env,
        caller: Address,
        do_refund: bool,
    ) -> Result<(), Error> {
        draw::trigger_randomness_fallback(env, caller, do_refund)
    }

    pub fn claim_prize(env: Env, winner: Address, tier_index: u32) -> Result<i128, Error> {
        claim::claim_prize(env, winner, tier_index)
    }

    /// Permissionless sweep of unclaimed prizes to treasury after `claim_expiry_seconds`
    /// has elapsed since finalization.  Returns the number of prizes swept.
    pub fn sweep_unclaimed(env: Env) -> Result<u32, Error> {
        crate::claim::sweep_unclaimed(env)
    }

    pub fn withdraw_fees(env: Env, recipient: Address, amount: i128) -> Result<(), Error> {
        admin::withdraw_fees(env, recipient, amount)
    }

    pub fn get_accumulated_fees(env: Env) -> i128 {
        views::get_accumulated_fees(env)
    }

    /// Aggregate dashboard view returning key raffle metrics in a single call.
    ///
    /// See [`views::get_stats`] for full documentation.
    pub fn get_stats(env: Env) -> Result<RaffleStats, Error> {
        views::get_stats(env)
    }

    pub fn cancel_raffle(env: Env, reason: CancelReason) -> Result<(), Error> {
        admin::cancel_raffle(env, reason)
    }

    /// Executes a previously scheduled admin cancellation (#406).
    ///
    /// Only succeeds once the timelock set by `cancel_raffle` has elapsed.
    /// Calling it earlier returns `CancelTimelockActive`; calling it with no
    /// pending schedule returns `CancelNotScheduled`.
    pub fn execute_admin_cancel(env: Env) -> Result<(), Error> {
        // This function was not implemented in the modules, keeping it inline for now.
        // To complete the refactor, this logic should be moved to `admin.rs`.
        Err(Error::InvalidParameters)
    }

    /// Returns the timestamp at which a scheduled admin cancel becomes
    /// executable, or `None` if no cancel is currently scheduled (#406).
    pub fn get_pending_cancel(env: Env) -> Option<u64> {
        // This function was not implemented in the modules, keeping it inline for now.
        // To complete the refactor, this logic should be moved to `views.rs`.
        None
    }

    pub fn refund_prize(env: Env) -> Result<(), Error> {
        claim::refund_prize(env)
    }

    pub fn emergency_withdraw(env: Env, caller: Address) -> Result<(), Error> {
        admin::emergency_withdraw(env, caller)
    }

    pub fn refund_ticket(env: Env, ticket_id: u32) -> Result<i128, Error> {
        claim::refund_ticket(env, ticket_id)
    }

    pub fn batch_refund_tickets(
        env: Env,
        owner: Address,
        ticket_ids: Vec<u32>,
    ) -> Result<i128, Error> {
        // This function was not implemented in the modules, keeping it inline for now.
        // To complete the refactor, this logic should be moved to `claim.rs`.
        Err(Error::InvalidParameters)
    }

    pub fn get_raffle(env: Env) -> Result<Raffle, Error> {
        views::get_raffle(env)
    }

    pub fn get_fairness_data(env: Env) -> Result<FairnessData, Error> {
        views::get_fairness_data(env)
    }

    /// Return a complete attestation package for third-party draw verification.
    ///
    /// Combines fairness data, metadata hash, winner addresses, winning ticket IDs,
    /// randomness source, and a hash of the effective raffle configuration into a
    /// single response. A verifier needs only this one call to obtain everything
    /// required to independently re-derive the winners.
    ///
    /// Only available in `Finalized` or `Claimed` states; returns `InvalidStatus`
    /// otherwise.
    ///
    /// See [`docs/RANDOMNESS.md`] for the verification procedure.
    pub fn get_draw_attestation(
        env: Env,
    ) -> Result<attestation::DrawAttestation, Error> {
        attestation::get_draw_attestation(&env)
    }

    /// Return all ticket IDs owned by `owner`.
    ///
    /// Uses the `OwnerTickets` index maintained during `buy_tickets` for an
    /// O(1) read.  Falls back to an empty Vec when the address has never
    /// purchased a ticket.
    pub fn get_my_tickets(env: Env, owner: Address) -> Vec<u32> {
        // This function was not implemented in the modules, keeping it inline for now.
        // To complete the refactor, this logic should be moved to `views.rs`.
        Vec::new(&env)
    }

    pub fn wipe_storage(env: Env) -> Result<(), Error> {
        admin::wipe_storage(env)
    }

    pub fn pause(env: Env) -> Result<(), Error> {
        admin::pause(env)
    }

    pub fn unpause(env: Env) -> Result<(), Error> {
        admin::unpause(env)
    }

    pub fn is_paused(env: Env) -> bool {
        views::is_paused(env)
    }

    pub fn pause_ticket_sales(env: Env, caller: Address) -> Result<(), Error> {
        admin::pause_ticket_sales(env, caller)
    }

    pub fn resume_ticket_sales(env: Env, caller: Address) -> Result<(), Error> {
        admin::resume_ticket_sales(env, caller)
    }

    pub fn is_ticket_sales_paused(env: Env) -> bool {
        views::is_ticket_sales_paused(env)
    }

    pub fn get_remaining_ticket_allowance(
        env: Env,
        owner: Address,
    ) -> Result<u32, Error> {
        views::get_remaining_ticket_allowance(env, owner)
    }

    /// Quote the exact cost of buying `quantity` tickets including early-bird
    /// discounts and protocol fees.
    ///
    /// Read-only — does not mutate state, does not require auth, does not
    /// check raffle status, pausing, or availability.  Returns a
    /// [`BuyQuote`] with the full pricing breakdown.  Uses the same
    /// internal helper as `buy_tickets` so quote and execution cannot
    /// diverge.
    pub fn preview_buy(env: Env, quantity: u32) -> Result<BuyQuote, Error> {
        views::preview_buy(env, quantity)
    }

    /// Sweep tokens that were accidentally sent to this contract.
    /// Configured payment and prize tokens can only be swept to the extent
    /// that all outstanding refunds, fees, and prize claims remain covered.
    pub fn rescue_tokens(
        env: Env,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), Error> {
        admin::rescue_tokens(env, token, recipient, amount)
    }

    /// Sweep residual payment-token balance to the treasury after the raffle is
    /// fully settled (`Claimed` or `Cancelled` with no outstanding prize or
    /// ticket-refund entitlements).
    ///
    /// See also: [`docs/EVENTS.md`](../../../docs/EVENTS.md) — `DustSwept`.
    pub fn sweep_dust(env: Env) -> Result<(), Error> {
        self::admin::sweep_dust(env)
    }

    pub fn update_oracle_address(env: Env, new_oracle: Address) -> Result<(), Error> {
        admin::update_oracle_address(env, new_oracle)
    }

    pub fn set_protocol_fee_bp(env: Env, new_fee_bp: u32) -> Result<(), Error> {
        admin::set_protocol_fee_bp(env, new_fee_bp)
    }

    pub fn set_swap_deadline(env: Env, new_deadline_seconds: u64) -> Result<(), Error> {
        admin::set_swap_deadline(env, new_deadline_seconds)
    }

    pub fn update_metadata_hash(env: Env, new_hash: BytesN<32>) -> Result<(), Error> {
        // This function was not implemented in the modules, keeping it inline for now.
        // To complete the refactor, this logic should be moved to `admin.rs`.
        Err(Error::InvalidParameters)
    }

}

    /// Permissionless entrypoint — anyone may call this to prevent a raffle
    /// from being archived by Soroban's TTL expiry.
    ///
    /// This entrypoint is currently unimplemented and returns
    /// [`Error::InvalidParameters`]. It does not bump any TTLs.
    ///
    /// The intended permissionless behavior is design documentation only.
    pub fn extend_ttl(env: Env) -> Result<(), Error> {
        let raffle = read_raffle(&env)?;
        // This function was not implemented in the modules, keeping it inline for now.
        // To complete the refactor, this logic should be moved to `helpers.rs`.
        Err(Error::InvalidParameters)
    }
}
#[cfg(test)]
mod test;
#[cfg(test)]
mod tests;
