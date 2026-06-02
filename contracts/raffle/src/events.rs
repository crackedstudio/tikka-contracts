use raffle_shared::AdminOp;
use soroban_sdk::{contractevent, u128, Address, BytesN};

#[derive(Clone)]
#[contractevent]
pub struct FactoryInitialized {
    pub admin: Address,
    pub protocol_fee_bp: u32,
    pub treasury: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct AdminOpProposed {
    pub op_id: u32,
    pub op: AdminOp,
    pub effective_timestamp: u64,
    pub proposed_by: Address,
}

#[derive(Clone)]
#[contractevent]
pub struct AdminOpExecuted {
    pub op_id: u32,
    pub op: AdminOp,
    pub executed_by: Address,
    pub executed_at: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct AdminOpCancelled {
    pub op_id: u32,
    pub cancelled_by: Address,
    pub cancelled_at: u64,
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
pub struct AdminTransferProposed {
    pub current_admin: Address,
    pub proposed_admin: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct AdminTransferAccepted {
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contractevent]
pub struct CheckpointCreated {
    pub index: u32,
    pub raffle_count: u32,
    pub ledger_timestamp: u64,
    pub aggregate_hash: BytesN<32>,
}

#[derive(Clone)]
#[contractevent]
pub struct RaffleCleanedUp {
    pub raffle_address: Address,
    pub cleaned_by: Address,
    pub finish_time: u64,
    pub cleaned_at: u64,
}
