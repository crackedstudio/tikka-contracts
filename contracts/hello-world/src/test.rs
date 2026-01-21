#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, vec, Address, Env, IntoVal, String,
};

/// Helper to reduce boilerplate in every test
fn setup_raffle_env(
    env: &Env,
    allow_multiple: bool,
) -> (u64, Address, Address, ContractClient<'static>, i128) {
    let creator = Address::generate(env);
    let buyer = Address::generate(env);
    let token_admin = Address::generate(env);

    // Get the contract object
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    // Extract the address specifically
    let token_address = token_contract.address();

    // Pass the address to the client
    token::StellarAssetClient::new(env, &token_address).mint(&buyer, &1000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);

    let ticket_price = 10i128;
    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(env, "Test Raffle"),
        &2000,
        &100,
        &allow_multiple,
        &ticket_price,
        &token_address, // Use token_address here
        &100,
    );

    (raffle_id, buyer, token_address, client, ticket_price)
}

#[test]
fn test_basic_raffle_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&creator, &1_000);
    token_admin_client.mint(&buyer, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Demo Raffle"),
        &0u64,
        &10u32,
        &false,
        &10i128,
        &token_id,
        &100i128,
    );

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);
    let winner = client.finalize_raffle(&raffle_id);
    client.claim_prize(&raffle_id, &winner);

    let winner_balance = token_client.balance(&winner);
    let creator_balance = token_client.balance(&creator);

    assert_eq!(winner_balance, 1_090);
    assert_eq!(creator_balance, 900);
}

#[test]
fn test_event_emits_on_single_purchase() {
    let env = Env::default();
    env.mock_all_auths();
    let (raffle_id, buyer, _, client, _) = setup_raffle_env(&env, true);

    client.buy_ticket(&raffle_id, &buyer);

    let last_event = env.events().all().last().expect("No event emitted");
    let event_data: TicketPurchasedEvent = last_event.2.into_val(&env);

    assert_eq!(event_data.quantity, 1);
}

// 2. Requirement: Event emits correctly for batch purchases
#[test]
fn test_event_emits_on_batch_purchase() {
    let env = Env::default();
    env.mock_all_auths();
    let (raffle_id, buyer, _, client, _) = setup_raffle_env(&env, true);

    let quantity = 5;
    client.buy_tickets_batch(&raffle_id, &buyer, &quantity);

    let last_event = env.events().all().last().expect("No event emitted");
    let event_data: TicketPurchasedEvent = last_event.2.into_val(&env);

    assert_eq!(event_data.quantity, 5);
}

// 3. Requirement: Event includes all ticket IDs purchased
#[test]
fn test_event_contains_correct_ticket_ids() {
    let env = Env::default();
    env.mock_all_auths();
    let (raffle_id, buyer, _, client, _) = setup_raffle_env(&env, true);

    // Buy 2 then buy 3. The second batch should have IDs [2, 3, 4]
    client.buy_tickets_batch(&raffle_id, &buyer, &2);
    client.buy_tickets_batch(&raffle_id, &buyer, &3);

    let last_event = env.events().all().last().unwrap();
    let event_data: TicketPurchasedEvent = last_event.2.into_val(&env);

    assert_eq!(event_data.ticket_ids, vec![&env, 2, 3, 4]);
}

// 4. Requirement: Total_paid calculation matches tickets × price
#[test]
fn test_event_total_paid_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let (raffle_id, buyer, _, client, ticket_price) = setup_raffle_env(&env, true);

    let quantity = 4;
    client.buy_tickets_batch(&raffle_id, &buyer, &quantity);

    let last_event = env.events().all().last().unwrap();
    let event_data: TicketPurchasedEvent = last_event.2.into_val(&env);

    assert_eq!(event_data.total_paid, ticket_price * (quantity as i128));
}

// 5. Requirement: Timestamp reflects transaction timing
#[test]
fn test_event_timestamp_accuracy() {
    let env = Env::default();
    env.mock_all_auths();
    let (raffle_id, buyer, _, client, _) = setup_raffle_env(&env, true);

    let custom_time = 1500;
    env.ledger().set_timestamp(custom_time);

    client.buy_ticket(&raffle_id, &buyer);

    let last_event = env.events().all().last().unwrap();
    let event_data: TicketPurchasedEvent = last_event.2.into_val(&env);

    assert_eq!(event_data.timestamp, custom_time);
}
