//! Events emitted by the **raffle instance** contract.
//!
//! Every struct here is a Soroban `#[contractevent]`.  Topics are scoped as
//! `tikka:<Topic>` where `<Topic>` is the struct name.
//!
//! **Index-vs-ID convention:** fields named `ticket_id`, `ticket_ids`, or
//! `ticket_number` hold **1-based ticket IDs**.  Fields named `*_index` or
//! `winning_ticket_ids` hold **0-based positions** into an array (they are not
//! ticket IDs) unless stated otherwise.
//!
//! Keep this file in sync with [`docs/EVENTS.md`](../../../docs/EVENTS.md) by
//! running `python scripts/generate_event_docs.py`.

pub use raffle_shared::events::{ContractPaused, ContractUnpaused};
use raffle_shared::{CancelReason, FailureReason, RandomnessSource, RandomnessType};
use soroban_sdk::{contractevent, Address, BytesN, String, Vec};

/// Emitted when a new raffle instance is created with its initial
/// configuration.
#[derive(Clone)]
#[contractevent]
pub struct RaffleCreated {
    /// Instance contract address of the new raffle.
    pub raffle_id: Address,
    /// Address that created the raffle.
    pub creator: Address,
    /// Ledger timestamp at which the raffle is scheduled to close.
    pub end_time: u64,
    /// Total number of tickets that can ever be sold (1-based ticket IDs run
    /// `1..=max_tickets`).
    pub max_tickets: u32,
    /// Nominal price (in `payment_token`) of a single ticket.
    pub ticket_price: i128,
    /// Token in which tickets are paid for.
    pub payment_token: Address,
    /// Total prize pool deposited into the raffle.
    pub prize_amount: i128,
    /// Prize tier weights (basis points), 0-based supported levels.
    pub prizes: Vec<u32>,
    /// Free-text description set by the creator.
    pub description: String,
    /// Entry point that will drive the draw randomness.
    pub randomness_source: RandomnessSource,
    /// Topic: SHA-256 of the metadata the description resolves to.
    #[topic]
    pub metadata_hash: BytesN<32>,
    /// Whether each address may win at most once.
    pub unique_winners: bool,
    /// Seconds after finalization before unclaimed prizes may be swept.
    pub claim_expiry_seconds: u64,
}

/// Emitted when the metadata hash backing a raffle's description is updated.
#[derive(Clone)]
#[contractevent]
pub struct MetadataHashUpdated {
    /// Metadata hash before the update.
    pub old_hash: BytesN<32>,
    /// Metadata hash after the update.
    pub new_hash: BytesN<32>,
    /// Address that performed the update.
    pub updated_by: Address,
    /// Ledger timestamp of the update.
    pub timestamp: u64,
}

/// Emitted when the prize pool is deposited into the raffle.
#[derive(Clone)]
#[contractevent]
pub struct PrizeDeposited {
    /// Address that deposited the prize (usually the creator).
    pub creator: Address,
    /// Amount deposited.
    pub amount: i128,
    /// Token the prize is held in.
    pub token: Address,
    /// Ledger timestamp of the deposit.
    pub timestamp: u64,
}

/// Emitted when the prize pool is refunded back to the creator.
#[derive(Clone)]
#[contractevent]
pub struct PrizeRefunded {
    /// Address that received the refund (usually the creator).
    pub creator: Address,
    /// Amount refunded.
    pub amount: i128,
    /// Token the refund was paid in.
    pub token: Address,
    /// Ledger timestamp of the refund.
    pub timestamp: u64,
}

/// Emitted when a ticket purchase succeeds and tickets are minted.
#[derive(Clone)]
#[contractevent]
pub struct TicketPurchased {
    /// Address whose account(s) paid for the tickets.
    pub buyer: Address,
    /// 1-based ticket IDs minted; length equals `quantity`.
    pub ticket_ids: Vec<u32>,
    /// Number of tickets purchased in this transaction.
    pub quantity: u32,
    /// Nominal unit price used for reserved seats.
    pub ticket_price: i128,
    /// Effective unit price actually charged (equals `ticket_price` when no
    /// discount applies).
    pub effective_ticket_price: i128,
    /// Total paid: `effective_ticket_price * quantity` after any discounts.
    pub total_paid: i128,
    /// Protocol fee (basis points of `total_paid`) withheld and forwarded to
    /// the treasury.
    pub protocol_fee: i128,
    /// Ledger timestamp of the purchase.
    pub timestamp: u64,
}

/// Emitted when tickets are bought for another address (a gift).
#[derive(Clone)]
#[contractevent]
pub struct TicketGifted {
    /// Address that paid for the tickets.
    pub buyer: Address,
    /// Address that received the tickets (owner of record).
    pub recipient: Address,
    /// 1-based ticket IDs minted; length equals `quantity`.
    pub ticket_ids: Vec<u32>,
    /// Number of tickets gifted in this transaction.
    pub quantity: u32,
    /// Nominal unit price used for reserved seats.
    pub ticket_price: i128,
    /// Effective unit price actually charged (equals `ticket_price` when no
    /// discount applies).
    pub effective_ticket_price: i128,
    /// Total paid by `buyer`.
    pub total_paid: i128,
    /// Protocol fee (basis points of `total_paid`).
    pub protocol_fee: i128,
    /// Ledger timestamp of the gift.
    pub timestamp: u64,
}

/// Emitted when a ticket changes ownership.  Note: this event is
/// `#[allow(dead_code)]` in the current implementation.
#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
pub struct TicketTransferred {
    /// 1-based ticket ID transferred.
    pub ticket_id: u32,
    /// Previous owner.
    pub from: Address,
    /// New owner.
    pub to: Address,
    /// Ledger timestamp of the transfer.
    pub timestamp: u64,
}

/// Emitted when the draw is triggered (usually by the last ticket purchase).
#[derive(Clone)]
#[contractevent]
pub struct DrawTriggered {
    /// Address that triggered the draw (usually the last buyer).
    pub caller: Address,
    /// Number of tickets sold when the draw was triggered.
    pub total_tickets_sold: u32,
    /// Ledger timestamp of the trigger.
    pub timestamp: u64,
}

/// Emitted when a randomness request is sent to the configured oracle.
#[derive(Clone)]
#[contractevent]
pub struct RandomnessRequested {
    /// Oracle address the request was sent to.
    pub oracle: Address,
    /// Correlation ID used to match the later delivery.
    pub request_id: u64,
    /// Ledger timestamp of the request.
    pub timestamp: u64,
}

/// Emitted when the oracle delivers a seed for the draw.
#[derive(Clone)]
#[contractevent]
pub struct RandomnessReceived {
    /// Oracle address that delivered the seed.
    pub oracle: Address,
    /// Raw seed value returned by the oracle.
    pub seed: u64,
    /// Correlation ID of the original request.
    pub request_id: u64,
    /// Ledger timestamp of the delivery.
    pub timestamp: u64,
}

/// Emitted each time an oracle in the quorum submits its random seed.
#[derive(Clone)]
#[contractevent]
pub struct OracleSeedDelivered {
    /// Oracle address that submitted its random seed for quorum aggregation.
    pub oracle: Address,
    /// Seed value delivered by this oracle.
    pub seed: u64,
    /// Correlation ID of the original quorum request.
    pub request_id: u64,
    /// Number of distinct seeds collected so far (1-based count).
    pub current_count: u32,
    /// Quorum threshold (`k`) required before aggregation happens.
    pub threshold: u32,
    /// Ledger timestamp of the delivery.
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct RaffleFinalized {
    /// Instance contract address of the finalized raffle.
    pub raffle_id: Address,
    /// Winner addresses; parallel to `winning_ticket_ids`.
    pub winners: Vec<Address>,
    /// 0-based winning positions within the ticket pool (1-based ticket IDs
    /// are `winning_ticket_ids[i] + 1`); parallel to `winners`.  These are
    /// **positions**, not ticket IDs.
    pub winning_ticket_ids: Vec<u32>,
    /// Number of tickets that were sold for the raffle.
    pub total_tickets_sold: u32,
    /// Randomness source that produced `randomness_type`.
    pub randomness_source: RandomnessSource,
    /// Concrete randomness mode used for this draw.
    pub randomness_type: RandomnessType,
    /// Ledger timestamp of finalization.
    pub finalized_at: u64,
    /// Whether unique-winner selection was applied.
    pub unique_winners: bool,
}

/// Emitted once per winner selected during the draw.
#[derive(Clone)]
#[contractevent]
pub struct WinnerDrawn {
    /// Winner address for this prize tier.
    pub winner: Address,
    /// 1-based ticket ID of the winning ticket (pool position + 1).
    pub ticket_id: u32,
    /// 0-based prize tier index this win corresponds to (`prizes[i]`).
    pub tier_index: u32,
    /// Ledger timestamp of the draw.
    pub timestamp: u64,
}

/// Emitted when a raffle is cancelled and (if applicable) prizes refunded.
#[derive(Clone)]
#[contractevent]
pub struct RaffleCancelled {
    /// Address that created the raffle.
    pub creator: Address,
    /// Machine-readable cancel reason.
    pub reason: CancelReason,
    /// Number of tickets sold before cancellation.
    pub tickets_sold: u32,
    /// Whether the deposited prize was returned to the creator.
    pub prize_refunded: bool,
    /// Ledger timestamp of the cancellation.
    pub timestamp: u64,
}

/// Emitted when an admin schedules a cancellation of a raffle that has already
/// sold tickets. The actual cancel only executes via `execute_admin_cancel`
/// once `cancel_at` has passed. Ticket holders may refund immediately as soon
/// as this event is emitted (#406).
#[derive(Clone)]
#[contractevent]
pub struct CancelScheduled {
    /// Address that created the raffle.
    pub creator: Address,
    /// Admin address that scheduled the cancellation.
    pub scheduled_by: Address,
    /// Number of tickets sold when the cancel was scheduled.
    pub tickets_sold: u32,
    /// Ledger timestamp at which the cancel becomes executable.
    pub cancel_at: u64,
    /// Ledger timestamp of the schedule.
    pub timestamp: u64,
}

/// Emitted when the raffle fails during its lifecycle and is wound down.
#[derive(Clone)]
#[contractevent]
pub struct RaffleFailed {
    /// Address that created the raffle.
    pub creator: Address,
    /// Machine-readable failure reason.
    pub reason: FailureReason,
    /// Number of tickets sold before the failure.
    pub tickets_sold: u32,
    /// Ledger timestamp of the failure.
    pub timestamp: u64,
}

/// Emitted when a ticket is refunded (only applies to refundable ticket
/// states).
#[derive(Clone)]
#[contractevent]
pub struct TicketRefunded {
    /// Address that was refunded (the ticket owner at refund time).
    pub buyer: Address,
    /// 1-based ticket ID that was refunded.
    pub ticket_number: u32,
    /// Amount refunded, denominated in the payment token.
    pub amount: i128,
    /// Ledger timestamp of the refund.
    pub timestamp: u64,
}

/// Emitted when a winner claims their prize.
#[derive(Clone)]
#[contractevent]
pub struct PrizeClaimed {
    /// Winner address claiming the prize.
    pub winner: Address,
    /// 0-based prize tier index being claimed (`prizes[i]`).
    pub tier_index: u32,
    /// Token the prize is paid in.
    pub payment_token: Address,
    /// Prize amount before platform fee deduction.
    pub gross_amount: i128,
    /// Amount actually transferred to the winner.
    pub net_amount: i128,
    /// Platform fee withheld from the prize.
    pub platform_fee: i128,
    /// Ledger timestamp of the claim.
    pub claimed_at: u64,
}

/// Emitted when accumulated protocol fees are withdrawn to the treasury.
#[derive(Clone)]
#[contractevent]
pub struct FeesWithdrawn {
    /// Address the accumulated fees were sent to.
    pub recipient: Address,
    /// Amount withdrawn.
    pub amount: i128,
    /// Token the fees were held in.
    pub token: Address,
    /// Ledger timestamp of the withdrawal.
    pub timestamp: u64,
}

/// Emitted when the randomness fallback path is used to finalize a draw.
#[derive(Clone)]
#[contractevent]
pub struct RandomnessFallbackTriggered {
    /// Address that triggered the fallback.
    pub triggered_by: Address,
    /// Seed value used to finalize the draw.
    pub seed_used: u64,
    /// Ledger sequence at which randomness was originally requested.
    pub request_ledger: u32,
    /// Ledger sequence at which the fallback fired.
    pub fallback_ledger: u32,
    /// Ledger timestamp of the fallback.
    pub timestamp: u64,
}

/// Emitted on every raffle status transition.
#[derive(Clone)]
#[contractevent]
pub struct RaffleStatusChanged {
    /// Status before the transition.
    pub old_status: raffle_shared::RaffleStatus,
    /// Status after the transition.
    pub new_status: raffle_shared::RaffleStatus,
    /// Ledger timestamp of the transition.
    pub timestamp: u64,
}

/// Emitted when ticket sales are paused for the raffle.
#[derive(Clone)]
#[contractevent]
pub struct TicketSalesPaused {
    /// Address that paused sales.
    pub paused_by: Address,
    /// Ledger timestamp of the pause.
    pub timestamp: u64,
}

/// Emitted when ticket sales are resumed for the raffle.
#[derive(Clone)]
#[contractevent]
pub struct TicketSalesResumed {
    /// Address that resumed sales.
    pub resumed_by: Address,
    /// Ledger timestamp of the resume.
    pub timestamp: u64,
}

/// Emitted when tokens are rescued out of the raffle contract.
#[derive(Clone)]
#[contractevent]
pub struct TokensRescued {
    /// Address that rescued the tokens.
    pub rescued_by: Address,
    /// Token that was rescued.
    pub token: Address,
    /// Address the rescued funds were sent to.
    pub recipient: Address,
    /// Amount rescued.
    pub amount: i128,
    /// Ledger timestamp of the rescue.
    pub timestamp: u64,
}

/// Emitted when residual dust balances are swept to the treasury.
#[derive(Clone)]
#[contractevent]
pub struct DustSwept {
    /// Address that triggered the sweep.
    pub swept_by: Address,
    /// Token whose dust balance was swept.
    pub token: Address,
    /// Address the dust was swept to.
    pub treasury: Address,
    /// Amount swept.
    pub amount: i128,
    /// Ledger timestamp of the sweep.
    pub timestamp: u64,
}

/// Emitted when all contract storage for the raffle is wiped.
#[derive(Clone)]
#[contractevent]
pub struct StorageWiped {
    /// Address that wiped the storage.
    pub wiped_by: Address,
    /// Ledger timestamp of the wipe.
    pub timestamp: u64,
}

/// Emitted when the configured randomness oracle address is updated.
#[derive(Clone)]
#[contractevent]
pub struct OracleAddressUpdated {
    /// Previous oracle address, if one was configured.
    pub old_oracle: Option<Address>,
    /// New oracle address.
    pub new_oracle: Address,
    /// Address that performed the update.
    pub updated_by: Address,
    /// Ledger timestamp of the update.
    pub timestamp: u64,
}

/// Emitted when the protocol fee (basis points) is updated.
#[derive(Clone)]
#[contractevent]
pub struct ProtocolFeeUpdated {
    /// Protocol fee (basis points) before the update.
    pub old_fee_bp: u32,
    /// Protocol fee (basis points) after the update.
    pub new_fee_bp: u32,
    /// Address that performed the update.
    pub updated_by: Address,
    /// Ledger timestamp of the update.
    pub timestamp: u64,
}

/// Emitted when the swap deadline (seconds) is updated.
#[derive(Clone)]
#[contractevent]
pub struct SwapDeadlineUpdated {
    /// Swap deadline (seconds) before the update.
    pub old_deadline_seconds: u64,
    /// Swap deadline (seconds) after the update.
    pub new_deadline_seconds: u64,
    /// Address that performed the update.
    pub updated_by: Address,
    /// Ledger timestamp of the update.
    pub timestamp: u64,
}

/// Emitted when the raffle end time is extended.
#[derive(Clone)]
#[contractevent]
pub struct EndTimeExtended {
    /// End time before the extension.
    pub old_end_time: u64,
    /// End time after the extension.
    pub new_end_time: u64,
    /// Address that extended the end time.
    pub extended_by: Address,
    /// Ledger timestamp of the extension.
    pub timestamp: u64,
}

/// Emitted when an emergency withdrawal is executed after the delay elapses.
#[derive(Clone)]
#[contractevent]
pub struct EmergencyWithdrawn {
    /// Address that issued the emergency withdrawal.
    pub withdrawn_by: Address,
    /// Address the funds were withdrawn to.
    pub to: Address,
    /// Amount withdrawn.
    pub amount: i128,
    /// Token withdrawn.
    pub token: Address,
    /// Ledger timestamp of the withdrawal.
    pub timestamp: u64,
}

/// Emitted when the raffle admin is changed.  Note: this event is
/// `#[allow(dead_code)]` in the current implementation.
#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
pub struct AdminChanged {
    /// Admin before the change.
    pub old_admin: Address,
    /// Admin after the change.
    pub new_admin: Address,
    /// Topic: address that changed the admin.
    #[topic]
    pub changed_by: Address,
    /// Ledger timestamp of the change.
    pub timestamp: u64,
}

/// Emitted once per NFT receipt is successfully minted
/// by the configured `nft_contract`.
#[derive(Clone)]
#[contractevent]
pub struct TicketNftMinted {
    /// The address that received the NFT (the ticket buyer).
    pub recipient: Address,
    /// The ticket ID within this raffle (1-indexed).
    pub ticket_id: u32,
    /// The raffle instance contract address (NFT namespace).
    pub raffle_id: Address,
    /// The NFT contract that performed the mint.
    pub nft_contract: Address,
    /// Ledger timestamp of the mint.
    pub timestamp: u64,
}

/// Emitted once per unclaimed winner when `sweep_unclaimed` runs after
/// `claim_expiry_seconds` has elapsed since finalization.  The prize share
/// is transferred to the raffle's `treasury_address`.
#[derive(Clone)]
#[contractevent]
pub struct PrizeSwept {
    /// Original winner address whose unclaimed prize was swept.
    pub winner: Address,
    /// Prize tier index (0-based, matches `prizes` array).
    pub tier_index: u32,
    /// Treasury address that received the swept prize.
    pub treasury: Address,
    /// Amount transferred to treasury.
    pub amount: i128,
    /// Ledger timestamp of the sweep.
    pub swept_at: u64,
}
