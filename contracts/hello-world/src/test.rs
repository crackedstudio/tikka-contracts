#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Env, IntoVal, String, Symbol,
};

/// HELPER: Standardized environment setup
fn setup_raffle_env(
    env: &Env,
) -> (
    ContractClient<'_>,
    Address,
    Address,
    token::StellarAssetClient<'_>,
    u64,
) {
    let creator = Address::generate(env);
    let buyer = Address::generate(env);
    let admin = Address::generate(env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let admin_client = token::StellarAssetClient::new(env, &token_id);

    // FIXED: Added & to amounts and explicitly typed as i128
    admin_client.mint(&creator, &1_000i128);
    admin_client.mint(&buyer, &1_000i128);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(env, "Audit Raffle"),
        &0,
        &10,
        &false,
        &10i128,
        &token_id,
        &100i128,
    );

    (client, creator, buyer, admin_client, raffle_id)
}

// --- 1. FUNCTIONAL FLOW TESTS ---

#[test]
fn test_basic_raffle_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, buyer, admin_client, raffle_id) = setup_raffle_env(&env);
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);

    let winner = client.finalize_raffle(&raffle_id, &String::from_str(&env, "prng"));
    client.claim_prize(&raffle_id, &winner);

    assert_eq!(token_client.balance(&winner), 1_090i128);
    assert_eq!(token_client.balance(&creator), 900i128);
}

// --- 2. RANDOMNESS SOURCE TESTS ---

#[test]
fn test_randomness_source_prng() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, raffle_id) = setup_raffle_env(&env);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);

    let source = String::from_str(&env, "prng");
    let winner = client.finalize_raffle(&raffle_id, &source);

    assert_eq!(winner, buyer);
}

#[test]
fn test_randomness_source_oracle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, raffle_id) = setup_raffle_env(&env);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);

    let source = String::from_str(&env, "oracle");
    let winner = client.finalize_raffle(&raffle_id, &source);

    assert_eq!(winner, buyer);
}

// --- 3. EVENT AUDIT & STATE VALIDATION ---

#[test]
fn test_raffle_finalized_event_audit() {
    let env = Env::default();
    env.mock_all_auths();

    let expected_timestamp = 123456789;
    env.ledger().with_mut(|l| {
        l.timestamp = expected_timestamp;
    });

    let (client, _, buyer_1, admin_client, raffle_id) = setup_raffle_env(&env);

    let buyer_2 = Address::generate(&env);
    admin_client.mint(&buyer_2, &1_000i128);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer_1);
    client.buy_ticket(&raffle_id, &buyer_2);

    let source = String::from_str(&env, "oracle");
    let winner = client.finalize_raffle(&raffle_id, &source);

    // --- FIXED EVENT AUDIT SECTION ---
    let last_event = env.events().all().last().expect("No event emitted");

    // Topic 0 in contract is symbol_short!("finalized")
    let topic_0: Symbol = last_event.1.get(0).unwrap().into_val(&env);
    // Topic 1 in contract is raffle_id (u64)
    let topic_1: u64 = last_event.1.get(1).unwrap().into_val(&env);

    assert_eq!(topic_0, symbol_short!("finalized"));
    assert_eq!(topic_1, raffle_id);

    let event_data: RaffleFinalized = last_event.2.into_val(&env);

    assert_eq!(event_data.raffle_id, raffle_id);
    assert_eq!(event_data.winner, winner);
    assert_eq!(event_data.total_tickets_sold, 2);
    assert_eq!(event_data.randomness_source, source);
    assert_eq!(event_data.finalized_at, expected_timestamp);
    assert!(event_data.winning_ticket_id < 2);
}
