//! Events emitted by the **raffle factory** contract.
//!
//! Every struct here is a Soroban `#[contractevent]`.  Topics are scoped as
//! `tikka:<Topic>` where `<Topic>` is the struct name.
//!
//! **Index-vs-ID convention:** fields named `op_id`, `recurring_id`, `index`,
//! or `round` are identifiers/sequence numbers and are explicit about their
//! base.  See individual field docs.
//!
//! Keep this file in sync with [`docs/EVENTS.md`](../../../docs/EVENTS.md) by
//! running `python scripts/generate_event_docs.py`.

use raffle_shared::AdminOp;
pub use raffle_shared::events::{ContractPaused, ContractUnpaused};
use soroban_sdk::{contractevent, Address, BytesN};

/// Emitted when the factory deploys a new raffle instance contract.  Note:
/// this event is `#[allow(dead_code)]` in the current implementation.
#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RaffleInstanceDeployed {
    /// Address of the deployed raffle instance.
    pub instance: Address,
    /// SHA-256 wasm hash of the deployed instance code.
    pub wasm_hash: BytesN<32>,
    /// Address that deployed the instance.
    pub creator: Address,
    /// Ledger timestamp of the deployment.
    pub timestamp: u64,
}

/// Emitted when the factory contract is initialized for the first time.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct FactoryInitialized {
    /// Admin of the factory.
    pub admin: Address,
    /// Protocol fee (basis points) applied to prizes.
    pub protocol_fee_bp: u32,
    /// Treasury address that receives protocol fees.
    pub treasury: Address,
    /// Ledger timestamp of initialization.
    pub timestamp: u64,
}

/// Emitted when a new admin operation is proposed through the timelock
/// mechanism.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct AdminOpProposed {
    /// Sequential op ID (1-based) that identifies the proposed operation.
    pub op_id: u32,
    /// Operation payload being proposed.
    pub op: AdminOp,
    /// Ledger timestamp at which the op becomes executable.
    pub effective_timestamp: u64,
    /// Admin that proposed the op.
    pub proposed_by: Address,
}

/// Emitted when a proposed admin operation is executed after its timelock
/// elapsed.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct AdminOpExecuted {
    /// Sequential op ID (1-based) of the executed operation.
    pub op_id: u32,
    /// Operation payload that was executed.
    pub op: AdminOp,
    /// Admin that executed the op.
    pub executed_by: Address,
    /// Ledger timestamp of execution.
    pub executed_at: u64,
}

/// Emitted when the factory treasury address is changed.  Note: this event is
/// `#[allow(dead_code)]` in the current implementation.
#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct TreasuryChanged {
    /// Treasury before the change.
    pub old_treasury: Address,
    /// Treasury after the change.
    pub new_treasury: Address,
    /// Topic: address that changed the treasury.
    #[topic]
    pub changed_by: Address,
    /// Ledger timestamp of the change.
    pub timestamp: u64,
}

/// Emitted when a proposed timelocked admin operation is cancelled.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct AdminOpCancelled {
    /// Sequential op ID (1-based) of the cancelled operation.
    pub op_id: u32,
    /// Admin that cancelled the op.
    pub cancelled_by: Address,
    /// Ledger timestamp of the cancellation.
    pub cancelled_at: u64,
}

/// Emitted when raffle creation is paused for the whole factory.
#[derive(Clone)]
#[contractevent]
pub struct CreationPaused {
    /// Address that paused raffle creation.
    pub paused_by: Address,
    /// Ledger timestamp of the pause.
    pub timestamp: u64,
}

/// Emitted when raffle creation is resumed after being paused.
#[derive(Clone)]
#[contractevent]
pub struct CreationUnpaused {
    /// Address that unpaused raffle creation.
    pub unpaused_by: Address,
    /// Ledger timestamp of the resume.
    pub timestamp: u64,
}

/// Emitted when the entire factory (creation, pauses, and every live raffle)
/// is put into emergency pause.
#[derive(Clone)]
#[contractevent]
pub struct GlobalEmergencyPaused {
    /// Address that paused the whole factory.
    pub paused_by: Address,
    /// Free-text reason for the emergency pause.
    pub reason: soroban_sdk::String,
    /// Ledger timestamp of the pause.
    pub timestamp: u64,
}

/// Emitted when the factory-wide emergency pause is lifted.
#[derive(Clone)]
#[contractevent]
pub struct GlobalEmergencyUnpaused {
    /// Address that lifted the emergency pause.
    pub unpaused_by: Address,
    /// Ledger timestamp of the resume.
    pub timestamp: u64,
}

/// Emitted when an admin proposes transferring factory admin to a new address.
#[derive(Clone)]
#[contractevent]
pub struct AdminTransferProposed {
    /// Current admin.
    pub current_admin: Address,
    /// Admin proposed as replacement.
    pub proposed_admin: Address,
    /// Ledger timestamp of the proposal.
    pub timestamp: u64,
}

/// Emitted when the proposed admin accepts the admin transfer.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct AdminTransferAccepted {
    /// Admin before the transfer.
    pub old_admin: Address,
    /// Admin after the transfer.
    pub new_admin: Address,
    /// Ledger timestamp of the acceptance.
    pub timestamp: u64,
}

/// Emitted when an admin transfer proposal fails.  Note: this event is
/// `#[allow(dead_code)]` in the current implementation.
#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct AdminTransferFailed {
    /// Current admin at the time of the failed transfer.
    pub current_admin: Address,
    /// Admin that was proposed.
    pub proposed_admin: Address,
    /// Numeric failure reason code.
    pub reason_code: u32,
    /// Ledger timestamp of the failure.
    pub timestamp: u64,
}

/// Emitted periodically to create a verifiable state checkpoint of all tracked
/// raffles.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct CheckpointCreated {
    /// 1-based checkpoint sequence number (`raffle_count / CHECKPOINT_INTERVAL`).
    pub index: u32,
    /// Number of raffles recorded when the checkpoint was taken.
    pub raffle_count: u32,
    /// Ledger timestamp of the checkpoint.
    pub ledger_timestamp: u64,
    /// SHA-256 over the checkpoint inputs (raffle count, ledger sequence,
    /// timestamp).
    pub aggregate_hash: BytesN<32>,
}

/// Emitted when a Stellar Asset Contract (SAC) token's support status is
/// updated.  Note: this event is `#[allow(dead_code)]` in the current
/// implementation.
#[allow(dead_code)]
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct SupportedSacUpdated {
    /// Token whose SAC support flag changed.
    pub token: Address,
    /// Whether the token is supported for SAC-assisted settlement.
    pub supported: bool,
    /// Address that updated the flag.
    pub updated_by: Address,
    /// Ledger timestamp of the update.
    pub timestamp: u64,
}

/// Emitted when a finished raffle instance's storage is cleaned up from the
/// factory.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct RaffleCleanedUp {
    /// Address of the raffle instance that was cleaned up.
    pub raffle_address: Address,
    /// Address that performed the cleanup.
    pub cleaned_by: Address,
    /// Timestamp at which the raffle was finalized.
    pub finish_time: u64,
    /// Ledger timestamp of the cleanup.
    pub cleaned_at: u64,
}

/// Emitted when a creator is rate-limited from creating new raffles.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct CreationRateLimited {
    /// Creator that was rate-limited.
    pub creator: Address,
    /// Ledger timestamp at which creation is allowed again.
    pub unlock_timestamp: u64,
    /// Ledger timestamp of the rate-limit event.
    pub timestamp: u64,
}

/// Emitted when tokens are rescued out of the factory contract.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct FactoryTokensRescued {
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

/// Emitted when the factory contract is upgraded to new wasm code.
#[derive(Clone)]
#[contractevent]
#[soroban_sdk::contracttype]
pub struct FactoryUpgraded {
    /// Admin that triggered the upgrade.
    pub admin: Address,
    /// SHA-256 wasm hash of the new contract code.
    pub new_wasm_hash: BytesN<32>,
    /// Ledger timestamp of the upgrade.
    pub timestamp: u64,
}

/// Emitted when a recurring raffle schedule is created.
#[derive(Clone)]
#[contractevent]
pub struct RecurringRaffleCreated {
    /// Unique ID (1-based) of the recurring raffle schedule.
    pub recurring_id: u32,
    /// Creator that owns the recurring schedule.
    pub creator: Address,
    /// Seconds between consecutive rounds.
    pub interval_seconds: u64,
    /// Maximum number of rounds; `0` means unlimited.
    pub max_rounds: u32,
    /// Whether prize funding for each round happens automatically.
    pub auto_fund: bool,
    /// Ledger timestamp of the next scheduled round.
    pub next_due: u64,
    /// Ledger timestamp of the schedule creation.
    pub timestamp: u64,
}

/// Emitted each time a recurring raffle schedule fires a new round.
#[derive(Clone)]
#[contractevent]
pub struct RecurringRoundTriggered {
    /// Unique ID of the recurring raffle schedule.
    pub recurring_id: u32,
    /// 1-based round number that just fired.
    pub round: u32,
    /// Address of the raffle instance created for this round.
    pub raffle_address: Address,
    /// Ledger timestamp of the next scheduled round.
    pub next_due: u64,
    /// Ledger timestamp of the trigger.
    pub timestamp: u64,
}

/// Emitted when a recurring raffle schedule is cancelled.
#[derive(Clone)]
#[contractevent]
pub struct RecurringRaffleCancelled {
    /// Unique ID of the recurring raffle schedule.
    pub recurring_id: u32,
    /// Address that cancelled the schedule.
    pub cancelled_by: Address,
    /// Total number of rounds completed before cancellation.
    pub rounds_completed: u32,
    /// Ledger timestamp of the cancellation.
    pub timestamp: u64,
}
