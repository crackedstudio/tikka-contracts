#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, BytesN, Env,
};

#[test]
fn test_oracle_fallback_with_ledger_delays() {
    let env = Env::default();
    env.mock_all_auths();

    // 1. Setup factory, admin, creator
    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &100_000_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // 2. Initialize Raffle with External Randomness
    let config = RaffleConfig {
        description: String::from_str(&env, "Test Raffle"),
        end_time: 0,
        max_tickets: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 10_000,
        payment_token: payment_token.clone(),
        prize_amount: 10_000,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle.clone()),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Remove factory from storage so buy_tickets skips the factory code path
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    // 3. Deposit prize and buy ticket
    client.deposit_prize();
    client.buy_tickets(&creator, &10);

    // 4. Finalize raffle (requests randomness)
    client.finalize_raffle();

    // 5. Ensure it's in Drawing state and requested randomness
    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Drawing);

    // 6. Attempt fallback too early
    let result = client.try_trigger_randomness_fallback(&creator, &false);
    assert_eq!(result.err(), Some(Ok(Error::FallbackTooEarly)));

    // 7. Simulate ledger delays
    env.ledger().with_mut(|l| {
        l.sequence_number += ORACLE_TIMEOUT_LEDGERS + 1;
        l.timestamp += 86400; // 1 day
    });

    // 8. Trigger fallback successfully (no refund — finalize)
    client.trigger_randomness_fallback(&creator, &false);

    // 9. Verify finalized state
    let raffle_after = client.get_raffle();
    assert_eq!(raffle_after.status, RaffleStatus::Finalized);

    // We can also verify the fairness data
    let fairness = client.get_fairness_data();
    assert_eq!(fairness.randomness_source, RandomnessSource::External);
}
