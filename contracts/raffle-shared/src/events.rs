//! Events shared between the **raffle factory** and **raffle instance**
//! contracts.  Each consumer re-exports these via
//! `pub use raffle_shared::events::{ContractPaused, ContractUnpaused};` and
//! emits them with the identical payload shape, so a single event topic
//! (`tikka:ContractPaused`, `tikka:ContractUnpaused`) is used by both contracts.
//!
//! Keep this file in sync with [`docs/EVENTS.md`](../../../docs/EVENTS.md) by
//! running `python scripts/generate_event_docs.py`.

use soroban_sdk::{contractevent, Address};

/// Emitted when either the factory or an instance contract is paused.
#[derive(Clone)]
#[contractevent]
pub struct ContractPaused {
    /// Address that paused the contract.
    pub paused_by: Address,
    /// Ledger timestamp of the pause.
    pub timestamp: u64,
}

/// Emitted when either the factory or an instance contract is unpaused.
#[derive(Clone)]
#[contractevent]
pub struct ContractUnpaused {
    /// Address that unpaused the contract.
    pub unpaused_by: Address,
    /// Ledger timestamp of the resume.
    pub timestamp: u64,
}
