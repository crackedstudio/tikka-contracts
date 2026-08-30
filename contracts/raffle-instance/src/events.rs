pub use raffle_shared::events::{ContractPaused, ContractUnpaused};
use raffle_shared::{CancelReason, FailureReason, RandomnessSource, RandomnessType};
use soroban_sdk::{contractevent, Address, BytesN, String, Vec};

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RaffleCreated {
    pub raffle_id: Address,
    pub creator: Address,
    pub end_time: u64,
    pub max_tickets: u32,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub prize_amount: i128,
    pub prizes: Vec<u32>,
    pub description: String,
    pub randomness_source: RandomnessSource,
    #[topic]
    pub metadata_hash: BytesN<32>,
    pub unique_winners: bool,
}

#[derive(Clone)]
#[contractevent]
pub struct MetadataHashUpdated {
    pub old_hash: BytesN<32>,
    pub new_hash: BytesN<32>,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct PrizeDeposited {
    pub creator: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct PrizeRefunded {
    pub creator: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct TicketPurchased {
    pub buyer: Address,
    pub ticket_ids: Vec<u32>,
    pub quantity: u32,
    pub ticket_price: i128,
    pub effective_ticket_price: i128,
    pub total_paid: i128,
    pub protocol_fee: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct TicketGifted {
    pub buyer: Address,
    pub recipient: Address,
    pub ticket_ids: Vec<u32>,
    pub quantity: u32,
    pub ticket_price: i128,
    pub effective_ticket_price: i128,
    pub total_paid: i128,
    pub protocol_fee: i128,
    pub timestamp: u64,
}

#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct TicketTransferred {
    pub ticket_id: u32,
    pub from: Address,
    pub to: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct DrawTriggered {
    pub caller: Address,
    pub total_tickets_sold: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RandomnessRequested {
    pub oracle: Address,
    pub request_id: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RandomnessReceived {
    pub oracle: Address,
    pub seed: u64,
    pub request_id: u64,
    pub timestamp: u64,
}

/// Emitted when the raffle is finalized with all winners selected.
///
/// ## Ticket-ID convention
///
/// `winning_ticket_ids` contains **1-indexed ticket IDs** — one per prize
/// tier, in prize-tier order, parallel to `winners`.  These IDs match
/// `TicketPurchased.ticket_ids` and `WinnerDrawn.ticket_id`, and can be used
/// directly as `DataKey::Ticket(id)` storage keys for independent verification.
///
/// Internal draw machinery uses zero-based indices for winner selection (stored
/// in `FairnessMetadata.winning_ticket_indices` for deterministic replay), but
/// this public-facing event always presents 1-indexed IDs.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RaffleFinalized {
    pub raffle_id: Address,
    pub winners: Vec<Address>,
    /// 1-indexed ticket IDs for each winning tier, in prize-tier order,
    /// parallel to `winners`.  Index 0 is the first-tier winner's ticket ID,
    /// index 1 is the second-tier winner's ticket ID, and so on.
    pub winning_ticket_ids: Vec<u32>,
    pub total_tickets_sold: u32,
    pub randomness_source: RandomnessSource,
    pub randomness_type: RandomnessType,
    pub finalized_at: u64,
    pub unique_winners: bool,
}

/// Emitted for each winner drawn during finalization (one event per prize tier).
///
/// ## Ticket-ID convention
///
/// All numeric ticket fields in this event use **1-indexed ticket IDs**,
/// matching the IDs under which tickets are stored and issued to buyers
/// (see `tickets.rs`: "Tickets are numbered starting at 1").
///
/// `ticket_id` is **not** a zero-based array index — it is the exact ticket
/// number that appears in `TicketPurchased.ticket_ids` for the winning buyer.
/// The on-chain lookup that resolves this ticket to its owner also uses the
/// 1-indexed ID (`DataKey::Ticket(ticket_id)`), so this value can be used
/// directly as a storage key for independent verification.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct WinnerDrawn {
    pub winner: Address,
    /// 1-indexed ticket ID of the winning ticket.
    ///
    /// This matches the IDs emitted in `TicketPurchased.ticket_ids` and
    /// stored under `DataKey::Ticket(ticket_id)`.  It is **not** a
    /// zero-based index.
    pub ticket_id: u32,
    /// Prize tier index (0-based, matching the `prizes` array from `RaffleCreated`).
    pub tier_index: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RaffleCancelled {
    pub creator: Address,
    pub reason: CancelReason,
    pub tickets_sold: u32,
    pub prize_refunded: bool,
    pub timestamp: u64,
}

/// Emitted when an admin schedules a cancellation of a raffle that has already
/// sold tickets. The actual cancel only executes via `execute_admin_cancel`
/// once `cancel_at` has passed. Ticket holders may refund immediately as soon
/// as this event is emitted (#406).
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct CancelScheduled {
    pub creator: Address,
    pub scheduled_by: Address,
    pub tickets_sold: u32,
    /// Unix timestamp at which the cancel becomes executable.
    pub cancel_at: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RaffleFailed {
    pub creator: Address,
    pub reason: FailureReason,
    pub tickets_sold: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct TicketRefunded {
    pub buyer: Address,
    pub ticket_number: u32,
    pub amount: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct PrizeClaimed {
    pub winner: Address,
    pub tier_index: u32,
    pub payment_token: Address,
    pub gross_amount: i128,
    pub net_amount: i128,
    pub platform_fee: i128,
    pub claimed_at: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct FeesWithdrawn {
    pub recipient: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RandomnessFallbackTriggered {
    pub triggered_by: Address,
    pub seed_used: u64,
    pub request_ledger: u32,
    pub fallback_ledger: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RaffleStatusChanged {
    pub old_status: raffle_shared::RaffleStatus,
    pub new_status: raffle_shared::RaffleStatus,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct ContractPaused {
    pub paused_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct ContractUnpaused {
    pub unpaused_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct TicketSalesPaused {
    pub paused_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct TicketSalesResumed {
    pub resumed_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct TokensRescued {
    pub rescued_by: Address,
    pub token: Address,
    pub recipient: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct DustSwept {
    pub swept_by: Address,
    pub token: Address,
    pub treasury: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct StorageWiped {
    pub wiped_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct OracleAddressUpdated {
    pub old_oracle: Option<Address>,
    pub new_oracle: Address,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct ProtocolFeeUpdated {
    pub old_fee_bp: u32,
    pub new_fee_bp: u32,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct SwapDeadlineUpdated {
    pub old_deadline_seconds: u64,
    pub new_deadline_seconds: u64,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct EndTimeExtended {
    pub old_end_time: u64,
    pub new_end_time: u64,
    pub extended_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct EmergencyWithdrawn {
    pub withdrawn_by: Address,
    pub to: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct AdminChanged {
    pub old_admin: Address,
    pub new_admin: Address,
    #[topic]
    pub changed_by: Address,
    pub timestamp: u64,
}

/// Emitted once per NFT receipt is successfully minted
/// by the configured `nft_contract`.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct TicketNftMinted {
    /// The address that received the NFT (the ticket buyer).
    pub recipient: Address,
    /// The ticket ID within this raffle (1-indexed).
    pub ticket_id: u32,
    /// The raffle instance contract address (NFT namespace).
    pub raffle_id: Address,
    /// The NFT contract that performed the mint.
    pub nft_contract: Address,
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
