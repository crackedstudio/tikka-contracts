use proptest::prelude::*;
use std::collections::HashMap;

// A minimal model of raffle state for property testing.
#[derive(Debug, Clone)]
struct Model {
    max_tickets: u32,
    tickets_sold: u32,
    allow_multiple: bool,
    buyer_counts: HashMap<u8, u32>,
}

impl Model {
    fn new(max_tickets: u32, allow_multiple: bool) -> Self {
        Self {
            max_tickets,
            tickets_sold: 0,
            allow_multiple,
            buyer_counts: HashMap::new(),
        }
    }

    /// Attempt to buy `qty` tickets for `buyer`. Returns Ok(()) on success,
    /// Err(()) if the purchase should be rejected by the contract logic.
    fn try_buy(&mut self, buyer: u8, qty: u32) -> Result<(), ()> {
        // Reject if would exceed max
        if self.tickets_sold + qty > self.max_tickets {
            return Err(());
        }
        let existing = *self.buyer_counts.get(&buyer).unwrap_or(&0);
        if !self.allow_multiple && existing > 0 && qty > 0 {
            return Err(());
        }
        // Accept: apply updates
        self.tickets_sold += qty;
        *self.buyer_counts.entry(buyer).or_insert(0) += qty;
        Ok(())
    }

    fn invariants_hold(&self) -> bool {
        // tickets_sold must not exceed cap
        if self.tickets_sold > self.max_tickets {
            return false;
        }
        // if multiple not allowed, no buyer may have >1 tickets
        if !self.allow_multiple {
            for &cnt in self.buyer_counts.values() {
                if cnt > 1 {
                    return false;
                }
            }
        }
        true
    }
}

// Property: For many random sequences of buy attempts (valid and invalid),
// the model enforces invariants and rejects only the operations that would
// violate them. This test generates sequences of operations and asserts
// the post-conditions.
proptest! {
    #[test]
    fn random_purchase_sequences(
        max_tickets in 1u32..500u32,
        allow_multiple in any::<bool>(),
        ops in prop::collection::vec((0u8..16u8, 0u32..5u32), 1..200),
    ) {
        let mut model = Model::new(max_tickets, allow_multiple);

        for (buyer, qty) in ops {
            // Keep a copy to verify rejected attempts leave state unchanged
            let before = model.clone();
            let res = model.try_buy(buyer, qty);

            match res {
                Ok(()) => {
                    // After successful buy, invariants must hold
                    prop_assert!(model.invariants_hold(), "Invariants violated after successful buy: {:?}", model);
                }
                Err(()) => {
                    // If rejected, state must be unchanged
                    prop_assert_eq!(model.tickets_sold, before.tickets_sold);
                    prop_assert_eq!(model.buyer_counts, before.buyer_counts);
                }
            }
        }

        // Final check: invariants still hold
        prop_assert!(model.invariants_hold(), "Final invariants violated: {:?}", model);
    }
}
