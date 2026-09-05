use proptest::prelude::*;
use crate::{
    assert_solvent, calculate_tier_prize, DataKey, Raffle, RaffleStatus, Ticket, MAX_PRIZE_AMOUNT,
    MIN_TICKET_PRICE,
};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

fn valid_prize_weights() -> impl Strategy<Value = std::vec::Vec<u32>> {
    prop::collection::vec(0u32..=10_000, 0..=99)
        .prop_filter("basis points must leave room for the final tier", |weights| {
            weights.iter().copied().sum::<u32>() <= 10_000
        })
        .prop_map(|mut weights| {
            let allocated = weights.iter().copied().sum::<u32>();
            weights.push(10_000 - allocated);
            weights
        })
}

fn test_raffle(env: &Env, weights: &[u32], prize_amount: i128) -> Raffle {
    let mut prizes = Vec::new(env);
    for weight in weights {
        prizes.push_back(*weight);
    }

    Raffle {
        creator: Address::generate(env),
        description: String::from_str(env, "tier invariant"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        max_tickets_per_address: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: Address::generate(env),
        prize_token: Address::generate(env),
        prize_amount,
        prizes,
        tickets_sold: 0,
        status: RaffleStatus::PendingPrize,
        prize_deposited: false,
        winners: Vec::new(env),
        claimed_winners: Vec::new(env),
        randomness_source: raffle_shared::RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        finalized_at: None,
        claim_lockup_seconds: 0,
        claim_expiry_seconds: 1,
        swap_deadline_seconds: 0,
        ticket_sales_paused: false,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        metadata_hash: BytesN::from_array(env, &[1; 32]),
        unique_winners: false,
        nft_contract: None,
    }
}

fn assert_tier_sum(weights: &[u32], prize_amount: i128) {
    let env = Env::default();
    let raffle = test_raffle(&env, weights, prize_amount);
    let mut total = 0i128;

    for index in 0..raffle.prizes.len() {
        let amount = calculate_tier_prize(&raffle, index).unwrap();
        assert!(amount >= 0, "tier {index} computed a negative prize");
        total += amount;
    }

    assert_eq!(total, prize_amount);
}

proptest! {
    #[test]
    fn tier_prizes_sum_to_prize_amount(
        weights in valid_prize_weights(),
        prize_amount in MIN_TICKET_PRICE..=MAX_PRIZE_AMOUNT,
    ) {
        assert_tier_sum(&weights, prize_amount);
    }
}

#[test]
fn one_hundred_equal_tiers_sum_exactly() {
    assert_tier_sum(&[100; 100], 1_000_003);
}

#[test]
fn one_tier_receives_the_entire_prize() {
    assert_tier_sum(&[10_000], MAX_PRIZE_AMOUNT);
}

#[test]
fn final_tier_absorbs_maximum_rounding_dust() {
    assert_tier_sum(
        &[101; 99].iter().copied().chain([1]).collect::<std::vec::Vec<_>>(),
        10_000,
    );
}

/// Refund solvency invariant (#827).
///
/// After any refund operation the contract must hold at least as much as it
/// still owes to ticket holders (`per_ticket_refund` for each not-yet-refunded
/// ticket id) plus any un-refunded prize escrowed on behalf of the creator.
///
/// Called by the refund-path lifecycle tests (`tests/claim.rs`) after every
/// refund/prize-recovery operation, and asserted inline by the fuzz harness
/// (`fuzz/fuzz_targets/real_harness.rs::refund_cancel`).
pub fn assert_refund_solvency(
    env: &Env,
    contract_id: &Address,
    payment_token: &Address,
    prize_token: &Address,
    ticket_ids_owing: &[u32],
    per_ticket_refund: i128,
    prize_owing: i128,
) {
    let payment_balance = soroban_sdk::token::Client::new(env, payment_token).balance(contract_id);
    // When prize and payment tokens are the same address (the current wiring)
    // the prize pot is part of the payment balance and must not be counted twice.
    let prize_balance = if payment_token == prize_token {
        0
    } else {
        soroban_sdk::token::Client::new(env, prize_token).balance(contract_id)
    };
    let held = payment_balance + prize_balance;
    let outstanding = ticket_ids_owing.len() as i128 * per_ticket_refund + prize_owing;
    assert!(
        held >= outstanding,
        "refund solvency violated: contract holds {held} but owes {outstanding} \
         ({} tickets outstanding, prize owing {prize_owing}, payment {}, prize {})",
        ticket_ids_owing.len(),
        payment_balance,
        prize_balance,
    );
}
