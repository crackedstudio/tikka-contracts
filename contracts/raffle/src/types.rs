use soroban_sdk::{contracttype, Address, Vec};

use crate::instance::Ticket;

pub const DEFAULT_PAGE_LIMIT: u32 = 100;
pub const MAX_PAGE_LIMIT: u32 = 200;

#[derive(Clone)]
#[contracttype]
pub struct PaginationParams {
    pub limit: u32,
    pub offset: u32,
}

#[allow(non_camel_case_types)]
#[derive(Clone)]
#[contracttype]
pub struct PageResult_Raffles {
    pub items: Vec<Address>,
    pub total: u32,
    pub has_more: bool,
}

#[allow(non_camel_case_types)]
#[derive(Clone)]
#[contracttype]
pub struct PageResult_Tickets {
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
