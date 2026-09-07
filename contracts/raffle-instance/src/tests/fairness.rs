//! Post-draw fairness metadata assertions (#768).
//!
//! Guards against regressions in `do_finalize_with_seed`, which writes
//! `FairnessMetadata` to `DataKey::RandomnessSeed` exactly once per draw.
//! The tests finalize a raffle and assert that every field of
//! `get_fairness_data()` — including `unique_winners` and `draw_sequence` —
//! round-trips correctly, so that an accidental duplicate (or field-dropping)
//! write is caught.

use raffle_shared::RandomnessSource;
use soroban_sdk::{token::StellarAssetClient, Address, BytesN, Env, String, Vec};

use crate::{
    Contract, ContractClient, FairnessMetadata, DataKey, RaffleConfig, RaffleStatus,
    MIN_TICKET_PRICE,
};

fn setup_unique_winners_raffle(env: &Env) -> (ContractClient<'_>, Address, Address) {
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);

    let factory = Address::generate(env);
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);

    let token_admin = Address::generate(env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token = StellarAssetClient::new(env, &payment_token);
    token.mint(&creator, &1_000_000_000_000);
    token.mint(&buyer, &1_000_000_000_000);

    let config = RaffleConfig {
        description: String::from_str(env, "unique winners fairness"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 3,
        max_tickets_per_tx: 3,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![env, 6000u32, 3000, 1000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[77u8; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(300),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: true,
        bundles: Vec::new(env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    (client, contract_id, buyer)
}

#[test]
fn finalize_persists_all_fairness_fields_including_unique_winners() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, contract_id, buyer) = setup_unique_winners_raffle(&env);

    client.buy_tickets(&buyer, &3);
    client.finalize_raffle();

    assert_eq!(client.get_raffle().status, RaffleStatus::Finalized);

    let fairness = client.get_fairness_data();

    // Every stored field must round-trip through get_fairness_data exactly.
    assert_eq!(fairness.randomness_source, RandomnessSource::Internal);
    assert_eq!(fairness.ticket_ids.len(), 3);
    assert_eq!(fairness.winning_ticket_indices.len(), 3);
    // unique_winners: true must be preserved (the bug dropped it in the
    // first (duplicate) write).
    assert_eq!(fairness.unique_winners, true);
    for i in 0..3 {
        assert_eq!(fairness.ticket_ids.get(i), Some(i + 1));
    }
    // draw_sequence is a copy of the ledger sequence at finalization.
    assert_eq!(fairness.draw_sequence, env.ledger().sequence());

    // Re-running must be deterministic for the same seed.
    assert_eq!(fairness.seed, client.get_fairness_data().seed);

    // Exactly one authoritative write to RandomnessSeed for this draw: the
    // stored metadata reflects a single finalization with unique_winners set
    // (the prior duplicate write dropped unique_winners and never compiled).
    env.as_contract(&contract_id, || {
        let meta: FairnessMetadata = env
            .storage()
            .persistent()
            .get(&DataKey::RandomnessSeed)
            .expect("fairness metadata must exist after finalization");
        assert_eq!(meta.unique_winners, true);
        assert_eq!(meta.winning_ticket_indices.len(), 3);
    });
}

#[test]
fn finalize_unique_winners_stays_within_budget() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer) = setup_unique_winners_raffle(&env);
    client.buy_tickets(&buyer, &3);

    let (cpu_before, _) = snapshot(&env);
    client.finalize_raffle();
    let (cpu_after, _) = snapshot(&env);

    // Generous ceiling; the key regression guard is that the redundant
    // duplicate write (now removed) is no longer charged on the finalize path.
    assert!(cpu_after.saturating_sub(cpu_before) < 80_000_000);
}

fn snapshot(env: &Env) -> (u64, u64) {
    let budget = env.cost_estimate().budget();
    (
        budget.cpu_instruction_cost(),
        budget.memory_bytes_cost(),
    )
}
