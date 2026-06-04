use raffle_shared::{CancelReason, RandomnessSource, RandomnessType};
use soroban_sdk::{contractevent, Address, BytesN, String, Vec};

#[derive(Clone)]
#[contractevent]
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
}

#[derive(Clone)]
#[contractevent]
pub struct PrizeDeposited {
    pub creator: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct PrizeRefunded {
    pub creator: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct TicketPurchased {
    pub buyer: Address,
    pub ticket_ids: Vec<u32>,
    pub quantity: u32,
    pub ticket_price: i128,
    pub total_paid: i128,
    pub protocol_fee: i128,
    pub timestamp: u64,
}

#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
pub struct TicketTransferred {
    pub ticket_id: u32,
    pub from: Address,
    pub to: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct DrawTriggered {
    pub caller: Address,
    pub total_tickets_sold: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct RandomnessRequested {
    pub oracle: Address,
    pub request_id: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct RandomnessReceived {
    pub oracle: Address,
    pub seed: u64,
    pub request_id: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct RaffleFinalized {
    pub raffle_id: Address,
    pub winners: Vec<Address>,
    pub winning_ticket_ids: Vec<u32>,
    pub total_tickets_sold: u32,
    pub randomness_source: RandomnessSource,
    pub randomness_type: RandomnessType,
    pub finalized_at: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct WinnerDrawn {
    pub winner: Address,
    pub ticket_id: u32,
    pub tier_index: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct RaffleCancelled {
    pub creator: Address,
    pub reason: CancelReason,
    pub tickets_sold: u32,
    pub prize_refunded: bool,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct TicketRefunded {
    pub buyer: Address,
    pub ticket_number: u32,
    pub amount: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
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
pub struct FeesWithdrawn {
    pub recipient: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct RandomnessFallbackTriggered {
    pub triggered_by: Address,
    pub seed_used: u64,
    pub request_ledger: u32,
    pub fallback_ledger: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct RaffleStatusChanged {
    pub old_status: raffle_shared::RaffleStatus,
    pub new_status: raffle_shared::RaffleStatus,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct ContractPaused {
    pub paused_by: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct ContractUnpaused {
    pub unpaused_by: Address,
    pub timestamp: u64,
}


#[derive(Clone)]
#[contractevent]
pub struct EmergencyWithdrawn {
    pub withdrawn_by: Address,
    pub to: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct AdminChanged {
    pub old_admin: Address,
    pub new_admin: Address,
    #[topic]
    pub changed_by: Address,
    pub timestamp: u64,
}
