use soroban_sdk::{contracttype, Address, String, Vec};

use crate::instance::{CancelReason, RaffleStatus, RandomnessSource};

// ============================================================================
// LIFECYCLE EVENTS
// ============================================================================

/// Emitted when a new raffle instance is initialized
#[derive(Clone)]
#[contracttype]
pub struct RaffleCreated {
    pub creator: Address,
    pub end_time: u64,
    pub max_tickets: u32,
    pub ticket_price: i128,
    pub payment_token: Address,
    pub prize_amount: i128,
    pub prizes: Vec<u32>,
    pub description: String,
    pub randomness_source: RandomnessSource,
    pub timestamp: u64,
}

/// Emitted when the creator deposits the prize pool
#[derive(Clone)]
#[contracttype]
pub struct PrizeDeposited {
    pub creator: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

/// Emitted when a user purchases a ticket
#[derive(Clone)]
#[contracttype]
pub struct TicketPurchased {
    pub buyer: Address,
    pub ticket_ids: Vec<u32>,
    pub quantity: u32,
    pub ticket_price: i128,
    pub total_paid: i128,
    pub timestamp: u64,
}

/// Emitted when the draw process is triggered
#[derive(Clone)]
#[contracttype]
pub struct DrawTriggered {
    pub triggered_by: Address,
    pub total_tickets_sold: u32,
    pub timestamp: u64,
}

/// Emitted when external randomness is requested from oracle
#[derive(Clone)]
#[contracttype]
pub struct RandomnessRequested {
    pub oracle: Address,
    pub timestamp: u64,
}

/// Emitted when external randomness is received from oracle
#[derive(Clone)]
#[contracttype]
pub struct RandomnessReceived {
    pub oracle: Address,
    pub seed: u64,
    pub timestamp: u64,
}

/// Emitted when the raffle winner is determined
#[derive(Clone)]
#[contracttype]
pub struct RaffleFinalized {
    pub winners: Vec<Address>,
    pub winning_ticket_ids: Vec<u32>,
    pub total_tickets_sold: u32,
    pub randomness_source: RandomnessSource,
    pub finalized_at: u64,
}

/// Emitted when a raffle is cancelled
#[derive(Clone)]
#[contracttype]
pub struct RaffleCancelled {
    pub creator: Address,
    pub reason: CancelReason,
    pub tickets_sold: u32,
    pub prize_refunded: bool,
    pub timestamp: u64,
}

/// Emitted when a ticket holder receives a refund
#[derive(Clone)]
#[contracttype]
pub struct TicketRefunded {
    pub buyer: Address,
    pub ticket_number: u32,
    pub amount: i128,
    pub timestamp: u64,
}

/// Emitted when a winner claims their prize
#[derive(Clone)]
#[contracttype]
pub struct PrizeClaimed {
    pub winner: Address,
    pub tier_index: u32,
    pub payment_token: Address,
    pub gross_amount: i128,
    pub net_amount: i128,
    pub platform_fee: i128,
    pub claimed_at: u64,
}

/// Emitted when platform fees are automatically swapped and burned
#[derive(Clone)]
#[contracttype]
pub struct BuybackAndBurnExecuted {
    pub router: Address,
    pub payment_token: Address,
    pub tikka_token: Address,
    pub amount_in: i128,
    pub amount_out: i128,
    pub timestamp: u64,
}

// ============================================================================
// ADMIN EVENTS
// ============================================================================

/// Emitted when the oracle address is updated
#[derive(Clone)]
#[contracttype]
pub struct OracleAddressUpdated {
    pub old_oracle: Option<Address>,
    pub new_oracle: Address,
    pub updated_by: Address,
    pub timestamp: u64,
}

/// Emitted when the protocol fee is updated
#[derive(Clone)]
#[contracttype]
pub struct FeeUpdated {
    pub old_fee_bp: u32,
    pub new_fee_bp: u32,
    pub updated_by: Address,
    pub timestamp: u64,
}

/// Emitted when the treasury address is updated
#[derive(Clone)]
#[contracttype]
pub struct TreasuryUpdated {
    pub old_treasury: Option<Address>,
    pub new_treasury: Address,
    pub updated_by: Address,
    pub timestamp: u64,
}

/// Emitted when accumulated fees are withdrawn to the treasury
#[derive(Clone)]
#[contracttype]
pub struct FeesWithdrawn {
    pub recipient: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

/// Emitted when the contract is paused
#[derive(Clone)]
#[contracttype]
pub struct ContractPaused {
    pub paused_by: Address,
    pub timestamp: u64,
}

/// Emitted when the contract is unpaused
#[derive(Clone)]
#[contracttype]
pub struct ContractUnpaused {
    pub unpaused_by: Address,
    pub timestamp: u64,
}

/// Emitted when an admin transfer is proposed
#[derive(Clone)]
#[contracttype]
pub struct AdminTransferProposed {
    pub current_admin: Address,
    pub proposed_admin: Address,
    pub timestamp: u64,
}

/// Emitted when an admin transfer is accepted
#[derive(Clone)]
#[contracttype]
pub struct AdminTransferAccepted {
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

// ============================================================================
// FACTORY EVENTS
// ============================================================================

/// Emitted when the factory is initialized
#[derive(Clone)]
#[contracttype]
pub struct FactoryInitialized {
    pub admin: Address,
    pub protocol_fee_bp: u32,
    pub treasury: Address,
    pub timestamp: u64,
}

/// Emitted when a new raffle instance is deployed by the factory
#[derive(Clone)]
#[contracttype]
pub struct RaffleDeployed {
    pub raffle_address: Address,
    pub creator: Address,
    pub timestamp: u64,
}

/// Emitted when the factory protocol fee or treasury is updated via set_config
#[derive(Clone)]
#[contracttype]
pub struct FactoryConfigUpdated {
    pub protocol_fee_bp: u32,
    pub treasury: Address,
    pub updated_by: Address,
    pub timestamp: u64,
}

// ============================================================================
// INTERNAL STATE CHANGE EVENT
// ============================================================================

/// Emitted on every raffle status transition
#[derive(Clone)]
#[contracttype]
pub struct StatusChanged {
    pub old_status: RaffleStatus,
    pub new_status: RaffleStatus,
    pub timestamp: u64,
}
