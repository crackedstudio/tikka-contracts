use soroban_sdk::{contracttype, Address, Vec};

use crate::instance::{Ticket, RandomnessSource};

pub const DEFAULT_PAGE_LIMIT: u32 = 100;
pub const MAX_PAGE_LIMIT: u32 = 200;

/// Fairness data returned by get_fairness_data
/// Contains all data used to select the winner for transparency
#[derive(Clone)]
#[contracttype]
pub struct FairnessData {
    /// The initial randomness seed used for the draw
    pub seed: u64,
    /// The randomness source used (Internal or External)
    pub randomness_source: RandomnessSource,
    /// All ticket IDs that participated in the draw
    pub ticket_ids: Vec<u32>,
    /// Winning ticket indices selected
    pub winning_ticket_indices: Vec<u32>,
    /// Ledger timestamp when the draw occurred
    pub draw_timestamp: u64,
    /// Ledger sequence when the draw occurred
    pub draw_sequence: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct PaginationParams {
    pub limit: u32,
    pub offset: u32,
}

#[derive(Clone)]
#[contracttype]
pub struct PageResultRaffles {
    pub items: Vec<Address>,
    pub total: u32,
    pub has_more: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct PageResultTickets {
    pub items: Vec<Ticket>,
    pub total: u32,
    pub has_more: bool,
}

/// Returns the effective page limit for a requested value:
/// - `0` → `DEFAULT_PAGE_LIMIT` (100)
/// - `> MAX_PAGE_LIMIT` → `MAX_PAGE_LIMIT` (200)
/// - otherwise → `requested` unchanged
pub fn effective_limit(requested: u32) -> u32 {
    if requested == 0 {
        DEFAULT_PAGE_LIMIT
    } else if requested > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT
    } else {
        requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: paginated-query-system, Property 1: effective_limit correctness
    // Validates: Requirements 1.3, 1.4, 1.5, 1.6
    proptest! {
        #[test]
        fn prop_effective_limit_correctness(limit: u32) {
            if limit == 0 {
                prop_assert_eq!(effective_limit(limit), DEFAULT_PAGE_LIMIT);
            } else if limit > MAX_PAGE_LIMIT {
                prop_assert_eq!(effective_limit(limit), MAX_PAGE_LIMIT);
            } else {
                prop_assert_eq!(effective_limit(limit), limit);
            }
        }
    }
}
